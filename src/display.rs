// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use crate::conv::to_usize;
use crate::{shaper, Action, Vec2};

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{Effect, EffectFlags, MarkerPos, MarkerPosIter};
pub(crate) use text_runs::{LineRun, RunSpecial};
use wrap_lines::{Line, RunPart};

/// Error returned on operations if not ready
///
/// This error is returned if `prepare` must be called.
#[derive(Clone, Copy, Default, Debug, thiserror::Error)]
#[error("not ready")]
pub struct NotReady;

/// Text display, without source text representation
///
/// This struct stores glyph locations and intermediate values used to calculate
/// glyph layout. It does not contain the source text itself or "environment"
/// state used during layout.
///
/// In general, it is recommended to use [`crate::Text`] instead, which includes
/// a representation of the source text and environmental state.
///
/// ### Preparation
///
/// This struct tracks the state of preparation ([`Self::required_action`]).
/// Methods will return a [`NotReady`] error if called without sufficient
/// preparation.
///
/// The struct may be default-constructed.
///
/// Text preparation proceeds as follows:
///
/// 1.  [`Self::prepare_runs`] breaks the source text into runs which may be fed
///     to the shaping algorithm. Each run has a single bidi embedding level
///     (direction) and uses a single font face, and contains no explicit line
///     break (except as a terminator).
///
///     Each run is then fed through the text shaper, resulting in a sequence of
///     type-set glyphs.
/// 2.  Optionally, [`Self::measure_width`] may be used to calculate the
///     required width (mostly useful for short texts which will not wrap).
/// 3.  [`Self::prepare_lines`] takes the output of the first step and
///     applies line wrapping, line re-ordering (for bi-directional lines) and
///     alignment.
///
///     This step is separate primarily to allow faster re-wrapping should the
///     text's wrap width change.
///
/// ### Text navigation
///
/// Despite lacking a copy of the underlying text, text-indices may be mapped to
/// glyphs and lines, and vice-versa.
///
/// The text range is `0..self.text_len()`. Any index within this range
/// (inclusive of end point) is valid for usage in all methods taking an index.
/// Multiple indices may map to the same glyph (e.g. within multi-byte chars,
/// with combining-diacritics, and with ligatures). In some cases a single index
/// corresponds to multiple glyph positions (due to line-wrapping or change of
/// direction in bi-directional text).
///
/// Navigating to the start or end of a line can be done with
/// [`TextDisplay::find_line`] and [`TextDisplay::line_range`].
///
/// Navigating left or right should be done via a library such as
/// [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation)
/// which provides a
/// [`GraphemeCursor`](https://unicode-rs.github.io/unicode-segmentation/unicode_segmentation/struct.GraphemeCursor.html)
/// to step back or forward one "grapheme", in logical order. Navigating glyphs
/// in display-order is not currently supported. Optionally, the direction may
/// be reversed for right-to-left lines [`TextDisplay::line_is_rtl`], but note
/// that the result may be confusing since not all text on the line follows the
/// line's base direction and adjacent lines may have different directions.
///
/// To navigate "up" and "down" lines, use [`TextDisplay::text_glyph_pos`] to
/// get the position of the cursor, [`TextDisplay::find_line`] to get the line
/// number, then [`TextDisplay::line_index_nearest`] to find the new index.
#[derive(Clone, Debug)]
pub struct TextDisplay {
    // NOTE: typical numbers of elements:
    // Simple labels: runs=1, line_runs=1, wrapped_runs=1, lines=1
    // Longer texts wrapped over n lines: runs=1, line_runs=1, wrapped_runs=n, lines=n
    // Justified wrapped text: similar, but wrapped_runs is the word count
    // Simple texts with explicit breaks over n lines: all=n
    // Single-line bidi text: runs=n, line_runs=1, wrapped_runs=n, lines=1
    // Complex bidi or formatted texts: all=many
    // Conclusion: SmallVec<[T; 1]> saves allocations in many cases.
    //
    /// Level runs within the text, in logical order
    runs: SmallVec<[shaper::GlyphRun; 1]>,
    /// Subsets of runs forming a line, with line direction
    line_runs: SmallVec<[LineRun; 1]>,
    pub(crate) action: Action,
    /// Contiguous runs, in logical order
    ///
    /// Within a line, runs may not be in visual order due to BIDI reversals.
    wrapped_runs: SmallVec<[RunPart; 1]>,
    /// Visual (wrapped) lines, in visual and logical order
    lines: SmallVec<[Line; 1]>,
    #[cfg(feature = "num_glyphs")]
    num_glyphs: u32,
    l_bound: f32,
    r_bound: f32,
}

#[cfg(test)]
#[test]
fn size_of_elts() {
    use std::mem::size_of;
    assert_eq!(size_of::<SmallVec<[u8; 0]>>(), 32);
    assert_eq!(size_of::<shaper::GlyphRun>(), 128);
    assert_eq!(size_of::<LineRun>(), 12);
    assert_eq!(size_of::<RunPart>(), 24);
    assert_eq!(size_of::<Line>(), 24);
}

impl Default for TextDisplay {
    fn default() -> Self {
        TextDisplay {
            runs: Default::default(),
            line_runs: Default::default(),
            action: Action::All, // highest value
            wrapped_runs: Default::default(),
            lines: Default::default(),
            #[cfg(feature = "num_glyphs")]
            num_glyphs: 0,
            l_bound: 0.0,
            r_bound: 0.0,
        }
    }
}

impl TextDisplay {
    /// Get required action
    #[inline]
    pub fn required_action(&self) -> Action {
        self.action
    }

    /// Require an action
    ///
    /// Required actions are tracked internally. This combines internal action
    /// state with that input via `max`. It may be used, for example, to mark
    /// that fonts need resizing due to change in environment.
    #[inline]
    pub fn require_action(&mut self, action: Action) {
        self.action = self.action.max(action);
    }

    /// Get the number of lines (after wrapping)
    pub fn num_lines(&self) -> Result<usize, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }
        Ok(self.lines.len())
    }

    /// Get the size of the required bounding box
    ///
    /// This is the position of the upper-left and lower-right corners of a
    /// bounding box on content.
    /// Alignment and input bounds do affect the result.
    pub fn bounding_box(&self) -> Result<(Vec2, Vec2), NotReady> {
        if self.action > Action::VAlign {
            return Err(NotReady);
        }

        if self.lines.is_empty() {
            return Ok((Vec2::ZERO, Vec2::ZERO));
        }

        let top = self.lines.first().unwrap().top;
        let bottom = self.lines.last().unwrap().bottom;
        Ok((Vec2(self.l_bound, top), Vec2(self.r_bound, bottom)))
    }

    /// Find the line containing text `index`
    ///
    /// Returns the line number and the text-range of the line.
    ///
    /// Returns `None` in case `index` does not line on or at the end of a line
    /// (which means either that `index` is beyond the end of the text or that
    /// `index` is within a mult-byte line break).
    pub fn find_line(
        &self,
        index: usize,
    ) -> Result<Option<(usize, std::ops::Range<usize>)>, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }

        let mut first = None;
        for (n, line) in self.lines.iter().enumerate() {
            if line.text_range.end() == index {
                // When line wrapping, this also matches the start of the next
                // line which is the preferred location. At the end of other
                // lines it does not match any other location.
                first = Some((n, line.text_range.to_std()));
            } else if line.text_range.includes(index) {
                return Ok(Some((n, line.text_range.to_std())));
            }
        }
        Ok(first)
    }

    /// Get the range of a line, by line number
    pub fn line_range(&self, line: usize) -> Result<Option<std::ops::Range<usize>>, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }
        Ok(self.lines.get(line).map(|line| line.text_range.to_std()))
    }

    /// Get the directionality of the current line
    ///
    /// Returns:
    ///
    /// - `Err(NotReady)` if text is not prepared
    /// - `Ok(None)` if text is empty
    /// - `Ok(Some(line_is_right_to_left))` otherwise
    ///
    /// Note: indeterminate lines (e.g. empty lines) have their direction
    /// determined from the passed environment, by default left-to-right.
    pub fn line_is_rtl(&self, line: usize) -> Result<Option<bool>, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }
        if let Some(line) = self.lines.get(line) {
            let first_run = line.run_range.start();
            let glyph_run = to_usize(self.wrapped_runs[first_run].glyph_run);
            Ok(Some(self.runs[glyph_run].level.is_rtl()))
        } else {
            Ok(None)
        }
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// Note: if the font's `rect` does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    pub fn text_index_nearest(&self, pos: Vec2) -> Result<usize, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }
        let mut n = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.top > pos.1 {
                break;
            }
            n = i;
        }
        // Expected to return Some(..) value but None has been observed:
        self.line_index_nearest(n, pos.0)
            .map(|opt_line| opt_line.unwrap_or(0))
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// This is similar to [`TextDisplay::text_index_nearest`], but allows the
    /// line to be specified explicitly. Returns `None` only on invalid `line`.
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Result<Option<usize>, NotReady> {
        if !self.action.is_ready() {
            return Err(NotReady);
        }
        if line >= self.lines.len() {
            return Ok(None);
        }
        let line = &self.lines[line];
        let run_range = line.run_range.to_std();

        let mut best = line.text_range.start;
        let mut best_dist = f32::INFINITY;
        let mut try_best = |dist, index| {
            if dist < best_dist {
                best = index;
                best_dist = dist;
            }
        };

        for run_part in &self.wrapped_runs[run_range] {
            let glyph_run = &self.runs[to_usize(run_part.glyph_run)];
            let rel_pos = x - run_part.offset.0;

            let end_index;
            if glyph_run.level.is_ltr() {
                for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                    let dist = (glyph.position.0 - rel_pos).abs();
                    try_best(dist, glyph.index);
                }
                end_index = run_part.text_end;
            } else {
                let mut index = run_part.text_end;
                for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                    let dist = (glyph.position.0 - rel_pos).abs();
                    try_best(dist, index);
                    index = glyph.index
                }
                end_index = index;
            }

            let end_pos = if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                glyph_run.glyphs[run_part.glyph_range.end()].position.0
            } else {
                glyph_run.caret
            };
            try_best((end_pos - rel_pos).abs(), end_index);
        }

        Ok(Some(to_usize(best)))
    }
}
