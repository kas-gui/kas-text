// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use crate::conv::to_usize;
#[allow(unused)]
use crate::Text;
use crate::{shaper, Direction, Vec2};
use smallvec::SmallVec;

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{Effect, EffectFlags, MarkerPos, MarkerPosIter};
pub(crate) use text_runs::RunSpecial;
use wrap_lines::{Line, RunPart};

/// Error returned on operations if not ready
///
/// This error is returned if `configure` or `prepare` must be called.
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
/// Stages of preparation are as follows:
///
/// 1.  Ensure all required [fonts](crate::fonts) are loaded.
/// 2.  Call [`Self::prepare_runs`] to break text into level runs, then shape
///     these runs into glyph runs (unwrapped but with weak break points).
///
///     This method must be called again if the `text`, text `direction` or
///     `font_id` change. If only the text size (`dpem`) changes, it is
///     sufficient to instead call [`Self::resize_runs`].
/// 3.  Optionally, [`Self::measure_width`] and [`Self::measure_height`] may be
///     used at this point to determine size requirements.
/// 4.  Call [`Self::prepare_lines`] to wrap text and perform re-ordering (where
///     lines are bi-directional) and horizontal alignment.
///
///     This must be called again if any of `wrap_width`, `width_bound` or
///     `h_align` change.
/// 5.  Call [`Self::vertically_align`] to set or adjust vertical alignment.
///     (Not technically required if alignment is always top.)
///
/// All methods are idempotent (that is, they may be called multiple times
/// without affecting the result). Later stages of preparation do not affect
/// earlier stages, but if an earlier stage is repeated to account for adjusted
/// configuration then later stages must also be repeated.
///
/// This struct does not track the state of preparation. It is recommended to
/// use [`Text`] or a custom wrapper for that purpose. Failure to observe the
/// correct sequence is memory-safe but may cause panic or an unexpected result.
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
/// [`TextDisplay::find_line`] and [`TextDisplay::line_range`].
///
/// Navigating forwards or backwards should be done via a library such as
/// [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation)
/// which provides a
/// [`GraphemeCursor`](https://unicode-rs.github.io/unicode-segmentation/unicode_segmentation/struct.GraphemeCursor.html)
/// to step back or forward one "grapheme", in logical text order.
/// Optionally, the direction may
/// be reversed for right-to-left lines [`TextDisplay::line_is_rtl`], but note
/// that the result may be confusing since not all text on the line follows the
/// line's base direction and adjacent lines may have different directions.
///
/// Navigating glyphs left or right in display-order is not currently supported.
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
    assert_eq!(size_of::<SmallVec<[u8; 0]>>(), 24);
    assert_eq!(size_of::<shaper::GlyphRun>(), 128);
    assert_eq!(size_of::<RunPart>(), 24);
    assert_eq!(size_of::<Line>(), 24);
    assert_eq!(size_of::<TextDisplay>(), 240);
}

impl Default for TextDisplay {
    fn default() -> Self {
        TextDisplay {
            runs: Default::default(),
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
    /// Get the number of lines (after wrapping)
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    pub fn num_lines(&self) -> usize {
        self.lines.len()
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
            if line.text_range.end() == index {
                // When line wrapping, this also matches the start of the next
                // line which is the preferred location. At the end of other
                // lines it does not match any other location.
                first = Some((n, line.text_range.to_std()));
            } else if line.text_range.includes(index) {
                return Some((n, line.text_range.to_std()));
            }
        }
        first
    }

    /// Get the range of a line, by line number
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    pub fn line_range(&self, line: usize) -> Option<std::ops::Range<usize>> {
        self.lines.get(line).map(|line| line.text_range.to_std())
    }

    /// Get the base directionality of the text
    ///
    /// [Requires status][Self#status-of-preparation]: none.
    pub fn text_is_rtl(&self, text: &str, direction: Direction) -> bool {
        let (is_auto, mut is_rtl) = match direction {
            Direction::Ltr => (false, false),
            Direction::Rtl => (false, true),
            Direction::Auto => (true, false),
            Direction::AutoRtl => (true, true),
        };

        if is_auto {
            match unicode_bidi::get_base_direction(text) {
                unicode_bidi::Direction::Ltr => is_rtl = false,
                unicode_bidi::Direction::Rtl => is_rtl = true,
                unicode_bidi::Direction::Mixed => (),
            }
        }

        is_rtl
    }

    /// Get the directionality of the current line
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// Returns:
    ///
    /// - `None` if text is empty
    /// - `Some(line_is_right_to_left)` otherwise
    ///
    /// Note: indeterminate lines (e.g. empty lines) have their direction
    /// determined from the passed environment, by default left-to-right.
    pub fn line_is_rtl(&self, line: usize) -> Option<bool> {
        if let Some(line) = self.lines.get(line) {
            let first_run = line.run_range.start();
            let glyph_run = to_usize(self.wrapped_runs[first_run].glyph_run);
            Some(self.runs[glyph_run].level.is_rtl())
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

        Some(to_usize(best))
    }
}
