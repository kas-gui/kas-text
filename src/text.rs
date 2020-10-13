// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use std::convert::{AsMut, AsRef};
use std::ops::Bound;

use crate::conv::to_u32;
use crate::display::{Action, Effect, MarkerPosIter, PrepareAction, TextDisplay};
use crate::fonts::FontId;
use crate::parser::FormatData;
use crate::{Environment, FormattedString, UpdateEnv};
use crate::{Glyph, Vec2};

/// Text, prepared for display in a given enviroment
///
/// This struct is composed of two parts: a representation of the text being
/// displayed, and a [`TextDisplay`] object.
/// See also documentation of [`TextDisplay`].
#[derive(Clone, Debug)]
pub struct Text {
    /// Contiguous text in logical order
    text: String,
    fmt: Box<dyn FormatData>,
    display: TextDisplay,
}

impl Default for Text {
    fn default() -> Self {
        Text::new(Environment::default(), "".into())
    }
}

impl Text {
    /// Construct from an environment and a text model
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    pub fn new(env: Environment, text: FormattedString) -> Self {
        Text {
            text: text.text,
            fmt: text.fmt,
            display: TextDisplay::new(env),
        }
    }

    /// Construct from a default environment (single-line) and text
    ///
    /// The environment is default-constructed, with [`Environment::wrap`]
    /// turned off.
    #[inline]
    pub fn new_single(text: FormattedString) -> Self {
        let mut env = Environment::new();
        env.wrap = false;
        Self::new(env, text)
    }

    /// Construct from a default environment (multi-line) and text
    ///
    /// The environment is default-constructed (line-wrap on).
    #[inline]
    pub fn new_multi(text: FormattedString) -> Self {
        Self::new(Environment::new(), text)
    }

    /// Clone the formatted text
    pub fn clone_text(&self) -> FormattedString {
        let text = self.text.clone();
        let fmt = self.fmt.clone();
        FormattedString { text, fmt }
    }

    /// Extract text object, discarding the rest
    #[inline]
    pub fn take_text(self) -> FormattedString {
        let text = self.text;
        let fmt = self.fmt;
        FormattedString { text, fmt }
    }

    /// Clone the unformatted text as a `String`
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
        self.text.as_str()
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
    pub fn insert_char(&mut self, index: usize, c: char) -> PrepareAction {
        self.text.insert(index, c);
        self.fmt.insert_range(to_u32(index), to_u32(c.len_utf8()));
        self.display.action = Action::Runs;
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
    pub fn replace_range<R>(&mut self, range: R, replace_with: &str) -> PrepareAction
    where
        R: std::ops::RangeBounds<usize> + std::iter::ExactSizeIterator + Clone,
    {
        let start = match range.start_bound() {
            Bound::Included(x) => *x,
            Bound::Excluded(x) => *x + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(x) => *x + 1,
            Bound::Excluded(x) => *x,
            Bound::Unbounded => usize::MAX,
        };
        self.text.replace_range(start..end, replace_with);
        self.fmt.remove_range(to_u32(start), to_u32(end));
        self.fmt
            .insert_range(to_u32(start), to_u32(replace_with.len()));
        self.display.action = Action::Runs;
        true.into()
    }

    /// Set text to a raw `String`
    ///
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    pub fn set_string(&mut self, string: String) -> PrepareAction {
        self.text = string;
        self.fmt.remove_range(0, u32::MAX);
        self.display.action = Action::Runs;
        true.into()
    }

    /// Swap the raw text with a `String`
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    ///
    /// Currently this is not significantly more efficent than
    /// [`Text::set_text`]. This may change in the future (TODO).
    pub fn swap_string(&mut self, string: &mut String) -> PrepareAction {
        std::mem::swap(&mut self.text, string);
        self.fmt.remove_range(0, u32::MAX);
        self.display.action = Action::Runs;
        true.into()
    }

    /// Set the text
    pub fn set_text(&mut self, text: FormattedString) -> PrepareAction {
        /* TODO: enable if we have a way of testing equality (a hash?)
        if self.text == text {
            return self.action.into(); // no change
        }
         */

        self.text = text.text;
        self.fmt = text.fmt;
        self.display.action = Action::Runs;
        true.into()
    }
}

/// Wrappers around [`TextDisplay`] methods
impl Text {
    /// Read the environment
    #[inline]
    pub fn env(&self) -> &Environment {
        self.display.env()
    }

    /// Update the environment and prepare for display
    ///
    /// Wraps [`TextDisplay::update_env`], passing text representation as
    /// parameters. This calls [`TextDisplay::prepare`] when necessary.
    #[inline]
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) {
        if self.display.update_env(f).prepare() {
            self.display.prepare(&self.text, &*self.fmt);
        }
    }

    /// Prepare text for display
    ///
    /// Wraps [`TextDisplay::prepare`], passing text representation as parameters.
    #[inline]
    pub fn prepare(&mut self) {
        self.display.prepare(&self.text, &*self.fmt);
    }

    /// Get size requirements
    ///
    /// Wraps [`TextDisplay::required_size`].
    #[inline]
    pub fn required_size(&self) -> Vec2 {
        self.display.required_size()
    }

    /// Get the number of lines
    ///
    /// Wraps [`TextDisplay::num_lines`].
    #[inline]
    pub fn num_lines(&self) -> usize {
        self.display.num_lines()
    }

    /// Find the line containing text `index`
    ///
    /// Wraps [`TextDisplay::find_line`].
    #[inline]
    pub fn find_line(&self, index: usize) -> Option<(usize, std::ops::Range<usize>)> {
        self.display.find_line(index)
    }

    /// Get the range of a line, by line number
    ///
    /// Wraps [`TextDisplay::line_range`].
    #[inline]
    pub fn line_range(&self, line: usize) -> Option<std::ops::Range<usize>> {
        self.display.line_range(line)
    }

    /// Get the directionality of the current line
    ///
    /// Wraps [`TextDisplay::line_is_ltr`].
    #[inline]
    pub fn line_is_ltr(&self, line: usize) -> bool {
        self.display.line_is_ltr(line)
    }

    /// Get the directionality of the current line
    ///
    /// Wraps [`TextDisplay::line_is_rtl`].
    #[inline]
    pub fn line_is_rtl(&self, line: usize) -> bool {
        self.display.line_is_rtl(line)
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// Wraps [`TextDisplay::text_index_nearest`].
    #[inline]
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        self.display.text_index_nearest(pos)
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// Wraps [`TextDisplay::line_index_nearest`].
    #[inline]
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Option<usize> {
        self.display.line_index_nearest(line, x)
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// Wraps [`TextDisplay::text_glyph_pos`].
    pub fn text_glyph_pos(&self, index: usize) -> MarkerPosIter {
        self.display.text_glyph_pos(index)
    }

    /// Get the number of glyphs
    ///
    /// Wraps [`TextDisplay::num_glyphs`].
    #[inline]
    pub fn num_glyphs(&self) -> usize {
        self.display.num_glyphs()
    }

    /// Yield a sequence of positioned glyphs
    ///
    /// Wraps [`TextDisplay::glyphs`].
    pub fn glyphs<F: FnMut(FontId, f32, f32, Glyph)>(&self, f: F) {
        self.display.glyphs(f)
    }

    /// Like [`TextDisplay::glyphs`] but with added effects
    ///
    /// Wraps [`TextDisplay::glyphs_with_effects`].
    pub fn glyphs_with_effects<X, F, G>(&self, effects: &[Effect<X>], f: F, g: G)
    where
        X: Copy + Default,
        F: FnMut(FontId, f32, f32, Glyph, X),
        G: FnMut(f32, f32, f32, f32, X),
    {
        self.display.glyphs_with_effects(effects, f, g)
    }

    /// Yield a sequence of rectangles to highlight a given range, by lines
    ///
    /// Wraps [`TextDisplay::highlight_lines`].
    pub fn highlight_lines(&self, range: std::ops::Range<usize>) -> Vec<(Vec2, Vec2)> {
        self.display.highlight_lines(range)
    }

    /// Yield a sequence of rectangles to highlight a given range, by runs
    ///
    /// Wraps [`TextDisplay::highlight_runs`].
    #[inline]
    pub fn highlight_runs(&self, range: std::ops::Range<usize>) -> Vec<(Vec2, Vec2)> {
        self.display.highlight_runs(range)
    }
}

impl AsRef<TextDisplay> for Text {
    fn as_ref(&self) -> &TextDisplay {
        &self.display
    }
}

impl AsMut<TextDisplay> for Text {
    fn as_mut(&mut self) -> &mut TextDisplay {
        &mut self.display
    }
}
