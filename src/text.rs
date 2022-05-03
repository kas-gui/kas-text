// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use std::convert::{AsMut, AsRef};

use crate::display::{Effect, MarkerPosIter, NotReady, TextDisplay};
use crate::fonts::FaceId;
use crate::format::{EditableText, FormattableText};
use crate::{Action, Glyph, Vec2};
use crate::{EnvFlags, Environment, UpdateEnv};

/// Text, prepared for display in a given environment
///
/// This struct is composed of three parts: an [`Environment`], a representation
/// of the [`FormattableText`] being displayed, and a [`TextDisplay`] object.
///
/// Most Functionality is implemented via the [`TextApi`] and [`TextApiExt`]
/// traits.
#[derive(Clone, Debug)]
pub struct Text<T: FormattableText> {
    env: Environment,
    text: T,
    display: TextDisplay,
}

impl<T: FormattableText + Default> Default for Text<T> {
    fn default() -> Self {
        Text::new(Environment::default(), T::default())
    }
}

impl<T: FormattableText> Text<T> {
    /// Construct from an environment and a text model
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    pub fn new(env: Environment, text: T) -> Self {
        Text {
            env,
            text: text,
            display: TextDisplay::default(),
        }
    }

    /// Construct from a default environment (single-line) and text
    ///
    /// The environment is default-constructed, with line-wrapping
    /// turned off (see [`Environment::flags`] doc).
    #[inline]
    pub fn new_single(text: T) -> Self {
        let mut env = Environment::default();
        env.flags.remove(EnvFlags::WRAP);
        Self::new(env, text)
    }

    /// Construct from a default environment (multi-line) and text
    ///
    /// The environment is default-constructed (line-wrap on).
    #[inline]
    pub fn new_multi(text: T) -> Self {
        Self::new(Environment::default(), text)
    }

    /// Clone the formatted text
    pub fn clone_text(&self) -> T
    where
        T: Clone,
    {
        self.text.clone()
    }

    /// Extract text object, discarding the rest
    #[inline]
    pub fn take_text(self) -> T {
        self.text
    }

    /// Access the formattable text object
    #[inline]
    pub fn text(&self) -> &T {
        &self.text
    }

    /// Set the text
    ///
    /// One must call [`Text::prepare`] afterwards and may wish to inspect its
    /// return value to check the size allocation meets requirements.
    pub fn set_text(&mut self, text: T) {
        /* TODO: enable if we have a way of testing equality (a hash?)
        if self.text == text {
            return self.action.into(); // no change
        }
         */

        self.text = text;
        self.display.action = Action::All;
    }
}

/// Trait over a sub-set of [`Text`] functionality
///
/// This allows dynamic dispatch over [`Text`]'s type parameters.
pub trait TextApi {
    /// Length of text
    ///
    /// This is a shortcut to `self.as_str().len()`.
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    #[inline]
    fn str_len(&self) -> usize {
        self.as_str().len()
    }

    /// Access whole text as contiguous `str`
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    fn as_str(&self) -> &str;

    /// Clone the unformatted text as a `String`
    fn clone_string(&self) -> String;

    /// Read the environment
    fn env(&self) -> &Environment;

    /// Mutate the environment
    ///
    /// If using this directly, ensure that necessary preparation actions are
    /// completed afterwards. Consider using [`TextApiExt::update_env`] instead.
    fn env_mut(&mut self) -> &mut Environment;

    /// Read the [`TextDisplay`]
    fn display(&self) -> &TextDisplay;

    /// Require an action
    ///
    /// Wraps [`TextDisplay::require_action`].
    fn require_action(&mut self, action: Action);

    /// Prepare text for display
    ///
    /// Returns the required size to display text (with wrapping based on the
    /// bounds set in `env`), if any update occurred (see documentation of
    /// [`TextDisplay::prepare`]).
    ///
    /// Wraps [`TextDisplay::prepare`], passing through `env`.
    fn prepare(&mut self) -> Option<Vec2>;

    /// Prepare text runs
    ///
    /// Wraps [`TextDisplay::prepare_runs`], passing parameters from the
    /// environment state.
    fn prepare_runs(&mut self);

    /// Prepare lines ("wrap")
    ///
    /// Prepares text, returning required size, in pixels.
    ///
    /// This differs slightly from [`TextDisplay::prepare_lines`] in that it does all preparation
    /// necessary. It differs from [`Self::prepare`] in that this method always (re)calculates
    /// text wrapping and returns size requirements.
    fn prepare_lines(&mut self) -> Vec2;

    /// Get the sequence of effect tokens
    ///
    /// This method has some limitations: (1) it may only return a reference to
    /// an existing sequence, (2) effect tokens cannot be generated dependent
    /// on input state, and (3) it does not incorporate color information. For
    /// most uses it should still be sufficient, but for other cases it may be
    /// preferable not to use this method (use a dummy implementation returning
    /// `&[]` and use inherent methods on the text object via [`Text::text`]).
    fn effect_tokens(&self) -> &[Effect<()>];
}

impl<T: FormattableText> TextApi for Text<T> {
    #[inline]
    fn display(&self) -> &TextDisplay {
        &self.display
    }

    #[inline]
    fn as_str(&self) -> &str {
        self.text.as_str()
    }

    #[inline]
    fn clone_string(&self) -> String {
        self.text.as_str().to_string()
    }

    #[inline]
    fn env(&self) -> &Environment {
        &self.env
    }

    #[inline]
    fn env_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    #[inline]
    fn require_action(&mut self, action: Action) {
        self.display.require_action(action);
    }

    #[inline]
    fn prepare(&mut self) -> Option<Vec2> {
        self.display.prepare(&self.text, &self.env)
    }

    #[inline]
    fn prepare_runs(&mut self) {
        self.display.prepare_runs(
            &self.text,
            self.env.flags.contains(EnvFlags::BIDI),
            self.env.dir,
            self.env.font_id,
            self.env.dpp,
            self.env.pt_size,
        );
    }

    #[inline]
    fn prepare_lines(&mut self) -> Vec2 {
        if self.display.required_action() > Action::Wrap {
            self.display.prepare(&self.text, &self.env).unwrap()
        } else {
            self.display
                .prepare_lines(self.env.bounds, self.env.flags, self.env.align)
                .unwrap()
        }
    }

    #[inline]
    fn effect_tokens(&self) -> &[Effect<()>] {
        self.text.effect_tokens()
    }
}

/// Extension trait over [`TextApi`]
pub trait TextApiExt: TextApi {
    /// Update the environment and prepare, returning required size
    ///
    /// This prepares text as necessary. It always performs line-wrapping.
    fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) -> Vec2 {
        let mut update = UpdateEnv::new(self.env_mut());
        f(&mut update);
        let action = update.finish().max(self.display().action);
        match action {
            Action::All | Action::Resize => self.prepare_runs(),
            _ => (),
        }
        self.prepare_lines()
    }

    /// Get required action
    #[inline]
    fn required_action(&self) -> Action {
        self.display().action
    }

    /// Get the number of lines
    ///
    /// Wraps [`TextDisplay::num_lines`].
    #[inline]
    fn num_lines(&self) -> Result<usize, NotReady> {
        self.display().num_lines()
    }

    /// Find the line containing text `index`
    ///
    /// Wraps [`TextDisplay::find_line`].
    #[inline]
    fn find_line(&self, index: usize) -> Result<Option<(usize, std::ops::Range<usize>)>, NotReady> {
        self.display().find_line(index)
    }

    /// Get the range of a line, by line number
    ///
    /// Wraps [`TextDisplay::line_range`].
    #[inline]
    fn line_range(&self, line: usize) -> Result<Option<std::ops::Range<usize>>, NotReady> {
        self.display().line_range(line)
    }

    /// Get the directionality of the current line
    ///
    /// Wraps [`TextDisplay::line_is_ltr`].
    #[inline]
    fn line_is_ltr(&self, line: usize) -> Result<bool, NotReady> {
        self.display().line_is_ltr(line)
    }

    /// Get the directionality of the current line
    ///
    /// Wraps [`TextDisplay::line_is_rtl`].
    #[inline]
    fn line_is_rtl(&self, line: usize) -> Result<bool, NotReady> {
        self.display().line_is_rtl(line)
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// Wraps [`TextDisplay::text_index_nearest`].
    #[inline]
    fn text_index_nearest(&self, pos: Vec2) -> Result<usize, NotReady> {
        self.display().text_index_nearest(pos)
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// Wraps [`TextDisplay::line_index_nearest`].
    #[inline]
    fn line_index_nearest(&self, line: usize, x: f32) -> Result<Option<usize>, NotReady> {
        self.display().line_index_nearest(line, x)
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// Wraps [`TextDisplay::text_glyph_pos`].
    fn text_glyph_pos(&self, index: usize) -> Result<MarkerPosIter, NotReady> {
        self.display().text_glyph_pos(index)
    }

    /// Get the number of glyphs
    ///
    /// Wraps [`TextDisplay::num_glyphs`].
    #[inline]
    fn num_glyphs(&self) -> usize {
        self.display().num_glyphs()
    }

    /// Yield a sequence of positioned glyphs
    ///
    /// Wraps [`TextDisplay::glyphs`].
    fn glyphs<F: FnMut(FaceId, f32, Glyph)>(&self, f: F) -> Result<(), NotReady> {
        self.display().glyphs(f)
    }

    /// Like [`TextDisplay::glyphs`] but with added effects
    ///
    /// Wraps [`TextDisplay::glyphs_with_effects`].
    fn glyphs_with_effects<X, F, G>(
        &self,
        effects: &[Effect<X>],
        default_aux: X,
        f: F,
        g: G,
    ) -> Result<(), NotReady>
    where
        X: Copy,
        F: FnMut(FaceId, f32, Glyph, usize, X),
        G: FnMut(f32, f32, f32, f32, usize, X),
    {
        self.display()
            .glyphs_with_effects(effects, default_aux, f, g)
    }

    /// Yield a sequence of rectangles to highlight a given range, by lines
    ///
    /// Wraps [`TextDisplay::highlight_lines`].
    fn highlight_lines(
        &self,
        range: std::ops::Range<usize>,
    ) -> Result<Vec<(Vec2, Vec2)>, NotReady> {
        self.display().highlight_lines(range)
    }

    /// Yield a sequence of rectangles to highlight a given range, by runs
    ///
    /// Wraps [`TextDisplay::highlight_runs`].
    #[inline]
    fn highlight_runs(&self, range: std::ops::Range<usize>) -> Result<Vec<(Vec2, Vec2)>, NotReady> {
        self.display().highlight_runs(range)
    }
}

impl<T: TextApi + ?Sized> TextApiExt for T {}

/// Trait over a sub-set of [`Text`] functionality for editable text
///
/// This allows dynamic dispatch over [`Text`]'s type parameters.
pub trait EditableTextApi {
    /// Insert a char at the given position
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// Formatting is adjusted: any specifiers starting at or after `index` are
    /// delayed by the length of `c`.
    ///
    /// Currently this is not significantly more efficient than
    /// [`Text::set_text`]. This may change in the future (TODO).
    fn insert_char(&mut self, index: usize, c: char);

    /// Replace a section of text
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// One may simulate an unbounded range by via `start..usize::MAX`.
    ///
    /// Formatting is adjusted: any specifiers within the replaced text are
    /// pushed back to the end of the replacement, and the position of any
    /// specifiers after the replaced section is adjusted as appropriate.
    ///
    /// Currently this is not significantly more efficient than
    /// [`Text::set_text`]. This may change in the future (TODO).
    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str);

    /// Set text to a raw `String`
    ///
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    fn set_string(&mut self, string: String);

    /// Swap the raw text with a `String`
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    ///
    /// Currently this is not significantly more efficient than
    /// [`Text::set_text`]. This may change in the future (TODO).
    fn swap_string(&mut self, string: &mut String);
}

impl<T: EditableText> EditableTextApi for Text<T> {
    #[inline]
    fn insert_char(&mut self, index: usize, c: char) {
        self.text.insert_char(index, c);
        self.display.action = Action::All;
    }

    #[inline]
    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.text.replace_range(range, replace_with);
        self.display.action = Action::All;
    }

    #[inline]
    fn set_string(&mut self, string: String) {
        self.text.set_string(string);
        self.display.action = Action::All;
    }

    #[inline]
    fn swap_string(&mut self, string: &mut String) {
        self.text.swap_string(string);
        self.display.action = Action::All;
    }
}

impl<T: FormattableText> AsRef<TextDisplay> for Text<T> {
    fn as_ref(&self) -> &TextDisplay {
        &self.display
    }
}

impl<T: FormattableText> AsMut<TextDisplay> for Text<T> {
    fn as_mut(&mut self) -> &mut TextDisplay {
        &mut self.display
    }
}
