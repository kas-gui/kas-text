// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;
use std::ops::{BitOr, BitOrAssign};

use crate::conv::to_usize;
use crate::format::FormattableText;
use crate::{shaper, Vec2};
use crate::{Environment, UpdateEnv};

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{Effect, EffectFlags, MarkerPos, MarkerPosIter};
pub(crate) use text_runs::{LineRun, Run};
use wrap_lines::{Line, RunPart};

/// Type used to indicate whether the [`TextDisplay`] object is ready for use
///
/// The user doesn't need to *do* anything with this value when returned from
/// [`TextDisplay`] methods, but if [`PrepareAction::prepare`] returns true then
/// the user must call [`TextDisplay::prepare`] before calling most other methods.
///
/// Multiple instances may be combined via the `|` (bit or) and `|=` operators.
#[must_use]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct PrepareAction(bool);

impl PrepareAction {
    /// Construct an instance not requiring prepare
    ///
    /// This may be useful when optionally calling multiple update methods:
    /// ```
    /// # use kas_text::{PrepareAction, Text};
    /// fn update_text(text: &mut Text<String>, opt_new_text: Option<String>) {
    ///     let mut prepare = PrepareAction::none();
    ///     if let Some(new_text) = opt_new_text {
    ///         prepare |= text.set_text(new_text.into());
    ///     }
    ///     if prepare.prepare() {
    ///         text.prepare();
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn none() -> Self {
        PrepareAction(false)
    }

    /// When true, [`TextDisplay::prepare`] must be called
    #[inline]
    pub fn prepare(self) -> bool {
        self.0
    }
}

impl BitOr for PrepareAction {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        PrepareAction(self.0 || rhs.0)
    }
}

impl BitOrAssign for PrepareAction {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 = self.0 || rhs.0;
    }
}

impl From<bool> for PrepareAction {
    #[inline]
    fn from(prepare: bool) -> Self {
        PrepareAction(prepare)
    }
}

impl From<Action> for PrepareAction {
    #[inline]
    fn from(action: Action) -> Self {
        PrepareAction(action != Action::None)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(crate) enum Action {
    None,  // do nothing
    Wrap,  // do line-wrapping and alignment
    Shape, // do text shaping and above
    Dpem,  // update font size, redo shaping and above
    Runs,  // do splitting into runs, BIDI and above
}

impl Action {
    fn is_none(&self) -> bool {
        *self == Action::None
    }
}

/// Text display, without source text representation
///
/// In general, it is recommended to use [`crate::Text`] instead, which includes
/// a representation of the source text.
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
    pub(crate) env: Environment,
    /// Level runs within the text, in logical order
    runs: SmallVec<[Run; 1]>,
    /// Subsets of runs forming a line, with line direction
    line_runs: SmallVec<[LineRun; 1]>,
    pub(crate) action: Action,
    required: Vec2,
    /// Runs of glyphs (same order as `runs` sequence)
    glyph_runs: Vec<shaper::GlyphRun>,
    /// Contiguous runs, in logical order
    ///
    /// Within a line, runs may not be in visual order due to BIDI reversals.
    wrapped_runs: Vec<RunPart>,
    /// Visual (wrapped) lines, in visual and logical order
    lines: Vec<Line>,
    num_glyphs: u32,
}

impl Default for TextDisplay {
    fn default() -> Self {
        TextDisplay::new(Environment::default())
    }
}

impl TextDisplay {
    /// Construct from an environment
    ///
    /// This struct must be made ready for usage by calling [`TextDisplay::prepare`].
    pub fn new(env: Environment) -> Self {
        TextDisplay {
            env,
            runs: Default::default(),
            line_runs: Default::default(),
            action: Action::Runs, // highest value
            required: Default::default(),
            glyph_runs: Default::default(),
            wrapped_runs: Default::default(),
            lines: Default::default(),
            num_glyphs: Default::default(),
        }
    }

    /// Read the environment
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Update the environment
    ///
    /// If the only prepare action required after updating the environment is to
    /// re-wrap text, this is done immediately. In other cases,
    /// [`TextDisplay::prepare`] must be called and the returned
    /// [`PrepareAction`] indicates this.
    #[inline]
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) -> PrepareAction {
        let mut update = UpdateEnv::new(&mut self.env);
        f(&mut update);
        self.action = update.finish().max(self.action);
        if self.action > Action::Wrap {
            return PrepareAction(true);
        }
        if self.action == Action::Wrap {
            self.wrap_lines();
            self.action = Action::None;
        }
        PrepareAction(false)
    }

    /// Prepare text for display
    ///
    /// The required preparation/update steps are tracked internally; this
    /// method only performs the required steps. Updating line-wrapping due to
    /// changes in available width is significantly faster than updating the
    /// source text.
    pub fn prepare<F: FormattableText>(&mut self, text: &F) {
        let action = self.action;
        if action == Action::None {
            return;
        }

        if action >= Action::Runs {
            self.prepare_runs(text);
        }

        if action == Action::Dpem {
            // Note: this is only needed if we didn't just call prepare_runs()
            self.update_run_dpem(text);
        }

        if action >= Action::Shape {
            self.glyph_runs = self
                .runs
                .iter()
                .map(|run| shaper::shape(text.as_str(), &run))
                .collect();
        }

        if action >= Action::Wrap {
            self.wrap_lines();
        }

        self.action = Action::None;
    }

    /// Prepare (wrap only)
    ///
    /// This performs a sub-set of [`TextDisplay::prepare`]. It panics if any
    /// action other than re-wrapping is required.
    #[inline]
    pub fn prepare_wrap(&mut self) {
        if self.action == Action::None {
            return;
        }

        if self.action > Action::Wrap {
            panic!("kas-text::TextDisplay::prepare_wrap: action > wrap required");
        }

        self.wrap_lines();
        self.action = Action::None;
    }

    /// Get size requirements
    ///
    /// One must set initial size bounds and call [`TextDisplay::prepare`]
    /// before this method. Note that initial size bounds may cause wrapping
    /// and may cause parts of the text outside the bounds to be cut off.
    pub fn required_size(&self) -> Vec2 {
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
        self.required
    }

    /// Get the number of lines
    pub fn num_lines(&self) -> usize {
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
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
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
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
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
        self.lines.get(line).map(|line| line.text_range.to_std())
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for left-to-right lines, `false` for RTL.
    ///
    /// Panics if `line >= self.num_lines()`.
    pub fn line_is_ltr(&self, line: usize) -> bool {
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
        let first_run = self.lines[line].run_range.start();
        let glyph_run = to_usize(self.wrapped_runs[first_run].glyph_run);
        self.glyph_runs[glyph_run].level.is_ltr()
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for right-to-left lines, `false` for LTR.
    ///
    /// Panics if `line >= self.num_lines()`.
    #[inline]
    pub fn line_is_rtl(&self, line: usize) -> bool {
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
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
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
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
        assert!(self.action.is_none(), "kas-text::TextDisplay: not ready");
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
            let glyph_run = &self.glyph_runs[to_usize(run_part.glyph_run)];
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
