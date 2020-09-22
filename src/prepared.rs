// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use ab_glyph::PxScale;
use smallvec::SmallVec;
use std::ops::{BitOr, BitOrAssign};

use crate::{rich, shaper, FontId, Glyph, Vec2};
use crate::{Environment, UpdateEnv};

mod glyph_pos;
mod text_runs;
mod wrap_lines;
pub use glyph_pos::{MarkerPos, MarkerPosIter};
pub(crate) use text_runs::{LineRun, Run};
use wrap_lines::{Line, RunPart};

/// Type used to indicate whether the [`Text`] object is ready for use
///
/// The user doesn't need to *do* anything with this value when returned from
/// [`Text`] methods, but if [`Prepare::prepare`] returns true then the user
/// must call [`Text::prepare`] before calling most other methods.
/// Exceptions are [`Text::update_env`] (which can be used instead of `prepare`)
/// and methods operating on the environment or the source text.
///
/// Multiple instances may be combined via the `|` (bit or) and `|=` operators.
#[must_use]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Prepare(bool);

impl Prepare {
    /// Construct an instance not requiring prepare
    ///
    /// This may be useful when optionally calling multiple update methods:
    /// ```
    /// # use kas_text::prepared::{Prepare, Text};
    /// fn update_text(text: &mut Text, opt_new_text: Option<String>) {
    ///     let mut prepare = Prepare::none();
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
        Prepare(false)
    }

    /// When true, [`Text::prepare`] must be called
    #[inline]
    pub fn prepare(self) -> bool {
        self.0
    }
}

impl BitOr for Prepare {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Prepare(self.0 || rhs.0)
    }
}

impl BitOrAssign for Prepare {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 = self.0 || rhs.0;
    }
}

impl From<bool> for Prepare {
    #[inline]
    fn from(prepare: bool) -> Self {
        Prepare(prepare)
    }
}

impl From<Action> for Prepare {
    #[inline]
    fn from(action: Action) -> Self {
        Prepare(action != Action::None)
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

impl Default for Action {
    fn default() -> Self {
        Action::None
    }
}

impl Action {
    fn is_none(&self) -> bool {
        *self == Action::None
    }
}

/// Text, prepared for display in a given enviroment
///
/// Text is laid out for display in a box with the given size bounds.
///
/// The type can be default-constructed with no text.
///
/// The text range is `0..self.text_len()`. Any index within this range
/// (inclusive of end point) is a code-point boundary is valid for use in all
/// functions taking an index. (Most of the time, non-code-point boundaries
/// are also accepted.)
///
/// Navigating to the start or end of a line can be done with [`Text::find_line`].
///
/// Navigating left or right should be done via a library such as
/// [`unicode-segmentation`](https://github.com/unicode-rs/unicode-segmentation)
/// which provides a
/// [`GraphemeCursor`](https://unicode-rs.github.io/unicode-segmentation/unicode_segmentation/struct.GraphemeCursor.html)
/// to step back or forward one "grapheme".
///
/// Navigating "up" and "down" is trickier; use [`Text::text_glyph_pos`]
/// to get the position of the cursor, [`Text::find_line`] to get the line
/// number, then [`Text::line_index_nearest`] to find the new index.
#[derive(Clone, Debug, Default)]
pub struct Text {
    env: Environment,
    /// Contiguous text in logical order
    text: String,
    formatting: SmallVec<[rich::FormatSpec; 1]>,
    /// Level runs within the text, in logical order
    runs: SmallVec<[Run; 1]>,
    /// Subsets of runs forming a line, with line direction
    line_runs: SmallVec<[LineRun; 1]>,
    action: Action,
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

impl Text {
    /// Construct from an environment and a text model
    ///
    /// This struct must be made ready for use before
    /// To do so, call [`Text::prepare`].
    pub fn new(env: Environment, text: rich::Text) -> Text {
        let mut result = Text {
            env,
            ..Default::default()
        };
        let _ = result.set_text(text);
        result
    }

    /// Construct from a default environment (single-line) and text
    ///
    /// The environment is default-constructed, with [`Environment::wrap`]
    /// turned off.
    #[inline]
    pub fn new_single(text: rich::Text) -> Text {
        let mut env = Environment::new();
        env.wrap = false;
        Self::new(env, text)
    }

    /// Construct from a default environment (multi-line) and text
    ///
    /// The environment is default-constructed (line-wrap on).
    #[inline]
    pub fn new_multi(text: rich::Text) -> Text {
        Self::new(Environment::new(), text)
    }

    /// Reconstruct the [`rich::Text`] model defining this `Text`
    ///
    /// FIXME: currently this removes all formatting
    pub fn clone_text(&self) -> rich::Text {
        rich::Text {
            text: self.text.clone(),
            formatting: Default::default(),
        }
    }

    /// Clone the raw string
    pub fn clone_string(&self) -> String {
        self.text.clone()
    }

    /// Length of text
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    #[inline]
    pub fn text_len(&self) -> usize {
        self.text.len()
    }

    /// Access to the raw text
    ///
    /// This is the contiguous raw text without formatting information.
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Insert a char at the given position
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// Formatting is adjusted: any specifiers starting at or after `index` are
    /// delayed by the length of `c`.
    ///
    /// Currently this is not significantly more efficent than
    /// [`Text::set_text`]. This may change in the future (TODO).
    pub fn insert_char(&mut self, index: usize, c: char) -> Prepare {
        self.text.insert(index, c);

        let index = index as u32;
        let len = c.len_utf8() as u32;
        for spec in &mut self.formatting {
            if spec.start >= index {
                spec.start += len;
            }
        }

        self.action = Action::Runs;
        true.into()
    }

    /// Replace a section of text
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// Formatting is adjusted: any specifiers within the replaced text are
    /// pushed back to the end of the replacement, and the position of any
    /// specifiers after the replaced section is adjusted as appropriate.
    ///
    /// Currently this is not significantly more efficent than
    /// [`Text::set_text`]. This may change in the future (TODO).
    #[inline]
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str) -> Prepare
    where
        R: std::ops::RangeBounds<usize> + std::iter::ExactSizeIterator + Clone,
    {
        self.text.replace_range(range.clone(), replace_with);

        use std::ops::Bound;
        let start = match range.start_bound() {
            Bound::Included(b) => *b as u32,
            Bound::Excluded(_) => unreachable!(),
            Bound::Unbounded => 0,
        };
        let src_len = range.len() as u32;
        let dst_len = replace_with.len() as u32;
        let old_end = start + src_len;
        let new_end = start + dst_len;

        self.fix_formatting_replace(start, old_end, new_end);
        true.into()
    }

    fn fix_formatting_replace(&mut self, start: u32, old_end: u32, new_end: u32) {
        let diff = new_end.wrapping_sub(old_end);
        let mut last = None;
        let mut i = 0;
        while i < self.formatting.len() {
            let spec = &mut self.formatting[i];
            if spec.start >= start {
                if spec.start < old_end {
                    spec.start = new_end;
                } else {
                    // wrapping_add effectively allows subtraction despite unsigned type
                    spec.start = spec.start.wrapping_add(diff);
                }
                if let Some((index, start)) = last {
                    if start == spec.start {
                        self.formatting.remove(index as usize);
                        continue;
                    }
                }
                last = Some((i, spec.start));
            }
            i += 1;
        }

        self.action = Action::Runs;
    }

    /// Swap the raw text with a `String`
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// Warning: formatting information is not adjusted, even if existing
    /// formatting does not fit the new text.
    ///
    /// Currently this is not significantly more efficent than
    /// [`Text::set_text`]. This may change in the future (TODO).
    pub fn swap_string(&mut self, string: &mut String) -> Prepare {
        std::mem::swap(&mut self.text, string);
        self.action = Action::Runs;
        true.into()
    }

    /// Set the text
    pub fn set_text(&mut self, text: rich::Text) -> Prepare {
        /* TODO(opt): no change if both text and formatting is unchanged
        if self.text == text.text {
            return self.action.into(); // no change
        }
        */

        self.text = text.text;
        text.formatting.compile(&self.env, &mut self.formatting);
        self.action = Action::Runs;
        true.into()
    }

    /// Read the environment
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Update the environment and prepare for display
    ///
    /// This calls [`Text::prepare`] internally.
    #[inline]
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) {
        let mut update = UpdateEnv::new(&mut self.env);
        f(&mut update);
        self.action = update.finish().max(self.action);
        self.prepare();
    }

    /// Prepare text for display
    ///
    /// The required preparation/update steps are tracked internally; this
    /// method only performs the required steps. Updating line-wrapping due to
    /// changes in available width is significantly faster than updating the
    /// source text.
    pub fn prepare(&mut self) {
        let action = self.action;
        if action == Action::None {
            return;
        }

        if action >= Action::Runs {
            self.prepare_runs();
        }

        if action == Action::Dpem {
            // Note: this is only needed if we didn't just call prepare_runs()
            self.update_run_dpem();
        }

        if action >= Action::Shape {
            self.glyph_runs = self
                .runs
                .iter()
                .map(|run| shaper::shape(&self.text, &run))
                .collect();
        }

        if action >= Action::Wrap {
            self.wrap_lines();
        }

        self.action = Action::None;
    }

    /// Produce a list of positioned glyphs
    ///
    /// One must call [`Text::prepare`] before this method.
    ///
    /// The function `f` may be used to apply required type transformations.
    /// Note that since internally this `Text` type assumes text is between the
    /// origin and the bound given by the environment, the function may also
    /// wish to translate glyphs.
    ///
    /// Glyphs are passed in logical order: that is, `glyph.index` monotonically
    /// increases. `glyph.position` may change in any direction.
    ///
    /// Although glyph positions are already calculated prior to calling this
    /// method, it is still computationally-intensive enough that it may be
    /// worth caching the result for reuse. Since the type is defined by the
    /// function `f`, caching is left to the caller.
    pub fn positioned_glyphs<G, F: FnMut(&str, FontId, PxScale, Glyph) -> G>(
        &self,
        mut f: F,
    ) -> Vec<G> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        let text = &self.text;

        let mut glyphs = Vec::with_capacity(self.num_glyphs as usize);
        // self.wrapped_runs is in logical order
        for run_part in self.wrapped_runs.iter().cloned() {
            let run = &self.glyph_runs[run_part.glyph_run as usize];
            let font_id = run.font_id;
            let font_scale = run.font_scale;

            // Pass glyphs in logical order: this allows more optimal evaluation of effects.
            if run.level.is_ltr() {
                for mut glyph in run.glyphs[run_part.glyph_range.to_std()].iter().cloned() {
                    glyph.position = glyph.position + run_part.offset;
                    glyphs.push(f(text, font_id, font_scale.into(), glyph));
                }
            } else {
                for mut glyph in run.glyphs[run_part.glyph_range.to_std()]
                    .iter()
                    .rev()
                    .cloned()
                {
                    glyph.position = glyph.position + run_part.offset;
                    glyphs.push(f(text, font_id, font_scale.into(), glyph));
                }
            }
        }
        glyphs
    }

    /// Calculate size requirements
    ///
    /// One must set initial size bounds and call [`Text::prepare`]
    /// before this method. Note that initial size bounds may cause wrapping
    /// and may cause parts of the text outside the bounds to be cut off.
    pub fn required_size(&self) -> Vec2 {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        self.required
    }

    /// Get the number of lines
    pub fn num_lines(&self) -> usize {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
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
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
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
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        self.lines.get(line).map(|line| line.text_range.to_std())
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for left-to-right lines, `false` for RTL.
    ///
    /// Panics if `line >= self.num_lines()`.
    pub fn line_is_ltr(&self, line: usize) -> bool {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        let first_run = self.lines[line].run_range.start();
        let glyph_run = self.wrapped_runs[first_run].glyph_run as usize;
        self.glyph_runs[glyph_run].level.is_ltr()
    }

    /// Get the directionality of the current line
    ///
    /// Returns `true` for right-to-left lines, `false` for LTR.
    ///
    /// Panics if `line >= self.num_lines()`.
    #[inline]
    pub fn line_is_rtl(&self, line: usize) -> bool {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
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
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
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
    /// This is similar to [`Text::text_index_nearest`], but allows the line to
    /// be specified explicitly. Returns `None` only on invalid `line`.
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Option<usize> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
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
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
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

        Some(best as usize)
    }
}
