// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use crate::conv::to_usize;
use crate::format::FormattableText;
use crate::{shaper, Action, EnvFlags, Environment, Vec2};

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{Effect, EffectFlags, MarkerPos, MarkerPosIter};
pub(crate) use text_runs::LineRun;
use wrap_lines::{Line, RunPart};

/// Text display, without source text representation
///
/// In general, it is recommended to use [`crate::Text`] instead, which includes
/// a representation of the source text and environment state.
///
/// Once prepared (via [`TextDisplay::prepare`]), this struct contains
/// everything needed to display text, query glyph position and size
/// requirements, and even re-wrap text lines. It cannot, however, support
/// editing or cloning the source text.
///
/// This struct tracks its state of preparation and can be default-constructed
/// in an unprepared state with no text.
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
    /// Level runs within the text, in logical order
    runs: SmallVec<[shaper::GlyphRun; 1]>,
    /// Subsets of runs forming a line, with line direction
    line_runs: SmallVec<[LineRun; 1]>,
    pub(crate) action: Action,
    required: Vec2,
    /// Contiguous runs, in logical order
    ///
    /// Within a line, runs may not be in visual order due to BIDI reversals.
    wrapped_runs: Vec<RunPart>,
    /// Visual (wrapped) lines, in visual and logical order
    lines: Vec<Line>,
    num_glyphs: u32,
    /// Required for highlight_lines; may remove later:
    width: f32,
}

impl Default for TextDisplay {
    fn default() -> Self {
        TextDisplay {
            runs: Default::default(),
            line_runs: Default::default(),
            action: Action::All, // highest value
            required: Default::default(),
            wrapped_runs: Default::default(),
            lines: Default::default(),
            num_glyphs: 0,
            width: 0.0,
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

    /// Prepare text for display, as necessary
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text.
    ///
    /// Required preparation actions are tracked internally, but cannot
    /// notice changes in the environment. In case the environment has changed
    /// one should either call [`TextDisplay::require_action`] before this
    /// method or use the [`TextDisplay::prepare_runs`],
    /// [`TextDisplay::resize_runs`] and [`TextDisplay::prepare_lines`] methods.
    pub fn prepare<F: FormattableText>(&mut self, text: &F, env: &Environment) {
        let action = self.action;
        if action == Action::None {
            return;
        }
        self.action = Action::None;

        if action >= Action::All {
            let bidi = env.flags.contains(EnvFlags::BIDI);
            self.prepare_runs(text, bidi, env.dir, env.dpp, env.pt_size);
        } else if action == Action::Resize {
            // Note: this is only needed if we didn't just call prepare_runs()
            self.resize_runs(text, env.dpp, env.pt_size);
        }

        let wrap = env.flags.contains(EnvFlags::WRAP);
        self.prepare_lines(env.bounds, wrap, env.align);
    }

    /// Get the number of lines
    pub fn num_lines(&self) -> usize {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
        self.lines.len()
    }

    /// Find the line containing text `index`
    ///
    /// Returns the line number and the text-range of the line.
    ///
    /// Returns `None` in case `index` does not line on or at the end of a line
    /// (which means either that `index` is beyond the end of the text or that
    /// `index` is within a mult-byte line break).
    pub fn find_line(&self, index: usize) -> Option<(usize, std::ops::Range<usize>)> {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
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
    pub fn line_range(&self, line: usize) -> Option<std::ops::Range<usize>> {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
        self.lines.get(line).map(|line| line.text_range.to_std())
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for left-to-right lines, `false` for RTL.
    ///
    /// Panics if `line >= self.num_lines()`.
    pub fn line_is_ltr(&self, line: usize) -> bool {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
        let first_run = self.lines[line].run_range.start();
        let glyph_run = to_usize(self.wrapped_runs[first_run].glyph_run);
        self.runs[glyph_run].level.is_ltr()
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for right-to-left lines, `false` for LTR.
    ///
    /// Panics if `line >= self.num_lines()`.
    #[inline]
    pub fn line_is_rtl(&self, line: usize) -> bool {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
        !self.line_is_ltr(line)
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// Note: if the font's rect does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
        let mut n = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.top > pos.1 {
                break;
            }
            n = i;
        }
        // We now know that `n` is a valid line (we must have at least one).
        self.line_index_nearest(n, pos.0).unwrap()
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// This is similar to [`TextDisplay::text_index_nearest`], but allows the
    /// line to be specified explicitly. Returns `None` only on invalid `line`.
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Option<usize> {
        assert!(self.action.is_ready(), "kas-text::TextDisplay: not ready");
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
