// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use crate::conv::to_usize;
#[allow(unused)]
use crate::{Direction, Status, Text};
use crate::{Vec2, shaper};
use smallvec::SmallVec;
use tinyvec::TinyVec;

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{GlyphRun, MarkerPos, MarkerPosIter};
pub(crate) use text_runs::RunSpecial;
pub use wrap_lines::Line;
use wrap_lines::RunPart;

/// Error returned on operations if not ready
///
/// This error is returned if `prepare` must be called.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not ready")]
pub struct NotReady;

/// Text type-setting object (low-level, without text and configuration)
///
/// This struct caches type-setting data at multiple levels of preparation.
/// Its end result is a sequence of type-set glyphs.
///
/// It is usually recommended to use [`Text`] instead, which includes
/// the source text, type-setting configuration and status tracking.
///
/// ### Status of preparation
///
/// This struct does not track the state-of-preparation internally. It is
/// recommended to use [`Text`] or a custom wrapper to do this. The [`Status`]
/// enum may be helpful here.
///
/// Methods note the expected status-of-preparation. Violating this expectation
/// is memory-safe but may cause a panic or an unexpected result.
///
/// Steps of preparation are as follows:
///
/// 1.  Run-breaking: call [`Self::prepare_runs`] to break text into runs,
///     resolve fonts and apply shaping. (This is the most expensive step,
///     especially when shaping is enabled.)
/// 2.  (Optional) Measure size requirements using [`Self::measure_width`] and
///     [`Self::measure_height`].
/// 3.  Line-wrapping: call [`Self::prepare_lines`] to perform line-wrapping at
///     the given wrap-width. This also re-orders segments (where lines are
///     bi-directional) and performs horizontal alignment.
/// 4.  (Optional) Tweak alignment (e.g. to vertically center text).
///
/// ### Text navigation
///
/// Despite lacking a copy of the underlying text, text-indices may be mapped to
/// glyphs and lines, and vice-versa.
///
/// The text range is `0..self.text_len()`. Any index within this range
/// (inclusive of end point) is valid for usage in all methods taking a text index.
/// Multiple indices may map to the same glyph (e.g. within multi-byte chars,
/// with combining-diacritics, and with ligatures). In some cases a single index
/// corresponds to multiple glyph positions (due to line-wrapping or change of
/// direction in bi-directional text).
///
/// Navigating to the start or end of a line can be done with
/// [`TextDisplay::find_line`], [`TextDisplay::get_line`] and [`Line::text_range`].
///
/// Navigating forwards or backwards should be done via a library such as
/// [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation)
/// which provides a
/// [`GraphemeCursor`](https://unicode-rs.github.io/unicode-segmentation/unicode_segmentation/struct.GraphemeCursor.html)
/// to step back or forward one "grapheme", in logical text order.
/// The direction of navigation may be reversed for [right-to-left text](Self::text_is_rtl)
/// (i.e. reversed logical-order navigation).
///
/// To navigate "up" and "down" lines, use [`TextDisplay::text_glyph_pos`] to
/// get the position of the cursor, [`TextDisplay::find_line`] to get the line
/// number, then [`TextDisplay::line_index_nearest`] to find the new index.
#[derive(Clone, Debug)]
pub struct TextDisplay {
    // NOTE: typical numbers of elements:
    // Simple labels: runs=1, wrapped_runs=1, lines=1
    // Longer texts wrapped over n lines: runs=1, wrapped_runs=n, lines=n
    // Justified wrapped text: similar, but wrapped_runs is the word count
    // Simple texts with explicit breaks over n lines: all=n
    // Single-line bidi text: runs=n, wrapped_runs=n, lines=1
    // Complex bidi or formatted texts: all=many
    // Conclusion: SmallVec<[T; 1]> saves allocations in many cases.
    //
    /// Level runs within the text, in logical order
    runs: SmallVec<[shaper::GlyphRun; 1]>,
    /// Contiguous runs, in logical order
    ///
    /// Within a line, runs may not be in visual order due to BIDI reversals.
    wrapped_runs: TinyVec<[RunPart; 1]>,
    /// Visual (wrapped) lines, in visual and logical order
    lines: TinyVec<[Line; 1]>,
    l_bound: f32,
    r_bound: f32,
}

#[cfg(test)]
#[test]
fn size_of_elts() {
    use std::mem::size_of;
    assert_eq!(size_of::<TinyVec<[u8; 0]>>(), 24);
    assert_eq!(size_of::<shaper::GlyphRun>(), 120);
    assert_eq!(size_of::<RunPart>(), 24);
    assert_eq!(size_of::<Line>(), 24);
    assert_eq!(size_of::<TextDisplay>(), 208);
}

impl Default for TextDisplay {
    fn default() -> Self {
        TextDisplay {
            runs: Default::default(),
            wrapped_runs: Default::default(),
            lines: Default::default(),
            l_bound: 0.0,
            r_bound: 0.0,
        }
    }
}

impl TextDisplay {
    /// Get the number of lines (after wrapping)
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    #[inline]
    pub fn num_lines(&self) -> usize {
        self.lines.len()
    }

    /// Get line properties
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    #[inline]
    pub fn get_line(&self, index: usize) -> Option<&Line> {
        self.lines.get(index)
    }

    /// Iterate over line properties
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    #[inline]
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    /// Get the size of the required bounding box
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// Returns the position of the upper-left and lower-right corners of a
    /// bounding box on content.
    /// Alignment and input bounds do affect the result.
    pub fn bounding_box(&self) -> (Vec2, Vec2) {
        if self.lines.is_empty() {
            return (Vec2::ZERO, Vec2::ZERO);
        }

        let top = self.lines.first().unwrap().top;
        let bottom = self.lines.last().unwrap().bottom;
        (Vec2(self.l_bound, top), Vec2(self.r_bound, bottom))
    }

    /// Find the line containing text `index`
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// Returns the line number and the text-range of the line.
    ///
    /// Returns `None` in case `index` does not line on or at the end of a line
    /// (which means either that `index` is beyond the end of the text or that
    /// `index` is within a mult-byte line break).
    pub fn find_line(&self, index: usize) -> Option<(usize, std::ops::Range<usize>)> {
        let mut first = None;
        for (n, line) in self.lines.iter().enumerate() {
            let text_range = line.text_range();
            if text_range.end == index {
                // When line wrapping, this also matches the start of the next
                // line which is the preferred location. At the end of other
                // lines it does not match any other location.
                first = Some((n, text_range));
            } else if text_range.contains(&index) {
                return Some((n, text_range));
            }
        }
        first
    }

    /// Get the base directionality of the first paragraph
    ///
    /// [Requires status][Self#status-of-preparation]: run-breaking is complete.
    ///
    /// This returns the direction inferred from the `text` and [`Direction`]
    /// used during run-breaking. See also [`Direction::text_is_rtl`].
    #[inline]
    pub fn text_is_rtl(&self) -> bool {
        self.runs
            .first()
            .map(|r| r.base_level.is_rtl())
            .unwrap_or_default()
    }

    /// Get the directionality of the current line
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// Returns:
    ///
    /// - `None` if the line is not found
    /// - `Some(line_is_right_to_left)` otherwise
    ///
    /// Note: indeterminate lines (e.g. empty lines) have their direction
    /// determined from the passed [`Direction`].
    pub fn line_is_rtl(&self, line: usize) -> Option<bool> {
        if let Some(line) = self.lines.get(line) {
            let first_run = line.run_range.start();
            let glyph_run = to_usize(self.wrapped_runs[first_run].glyph_run);
            Some(self.runs[glyph_run].base_level.is_rtl())
        } else {
            None
        }
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// Note: if the font's `rect` does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        let mut n = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.top > pos.1 {
                break;
            }
            n = i;
        }
        // Expected to return Some(..) value but None has been observed:
        self.line_index_nearest(n, pos.0).unwrap_or(0)
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// This is similar to [`TextDisplay::text_index_nearest`], but allows the
    /// line to be specified explicitly. Returns `None` only on invalid `line`.
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Option<usize> {
        if line >= self.lines.len() {
            return None;
        }
        let line = &self.lines[line];
        let run_range = line.run_range.to_std();

        let mut best = line.text_range().start;
        let mut best_dist = f32::INFINITY;
        let mut try_best = |dist, index: u32| {
            if dist < best_dist {
                best = to_usize(index);
                best_dist = dist;
            }
        };

        for run_part in &self.wrapped_runs[run_range] {
            let glyph_run = &self.runs[to_usize(run_part.glyph_run)];
            let rel_pos = x - run_part.offset.0;

            if glyph_run.level.is_ltr() {
                for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                    let dist = (glyph.position.0 - rel_pos).abs();
                    try_best(dist, glyph.index);
                }

                let end_pos = if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                    glyph_run.glyphs[run_part.glyph_range.end()].position.0
                } else {
                    glyph_run.caret
                };
                try_best((end_pos - rel_pos).abs(), run_part.text_end);
            } else {
                let mut index = run_part.text_end;
                for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                    let dist = (glyph.position.0 - rel_pos).abs();
                    try_best(dist, index);
                    index = glyph.index
                }

                let end_pos = if run_part.glyph_range.start() > 0 {
                    glyph_run.glyphs[run_part.glyph_range.start() - 1]
                        .position
                        .0
                } else {
                    glyph_run.caret
                };
                try_best((end_pos - rel_pos).abs(), index);
            }
        }

        Some(best)
    }

    #[cfg(test)]
    pub(crate) fn raw_runs(&self) -> &[shaper::GlyphRun] {
        &self.runs
    }
}
