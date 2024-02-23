// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use crate::display::{Effect, MarkerPosIter, NotReady, TextDisplay};
use crate::fonts::{self, FaceId, InvalidFontId};
use crate::format::{EditableText, FormattableText};
use crate::{Action, Glyph, Vec2};
use crate::{Align, Direction, Environment};

/// Text, prepared for display in a given environment
///
/// This struct is composed of three parts: an [`Environment`], a representation
/// of the [`FormattableText`] being displayed, and a [`TextDisplay`] object.
///
/// Most Functionality is implemented via the [`TextApi`] and [`TextApiExt`]
/// traits.
#[derive(Clone, Debug, Default)]
pub struct Text<T: FormattableText + ?Sized> {
    env: Environment,
    display: TextDisplay,
    text: T,
}

impl<T: FormattableText> Text<T> {
    /// Construct from a text model
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    #[inline]
    pub fn new(text: T) -> Self {
        Text::new_env(Default::default(), text)
    }

    /// Construct from a text model and environment
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    #[inline]
    pub fn new_env(env: Environment, text: T) -> Self {
        Text {
            env,
            text,
            display: Default::default(),
        }
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

    /// Set the text and prepare (if any fonts are loaded)
    ///
    /// Sets `text` regardless of other outcomes.
    ///
    /// If fonts are not loaded, this fails fast (see [`fonts::any_loaded`]),
    /// unlike other preparation methods.
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    #[inline]
    pub fn set_and_try_prepare(&mut self, text: T) -> Result<bool, InvalidFontId> {
        self.set_text(text);
        self.try_prepare()
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
    fn env(&self) -> Environment;

    /// Set the environment
    ///
    /// Use of this method may require new preparation actions.
    /// Call [`TextApiExt::update_env`] instead to perform such actions
    /// with a single method call.
    fn set_env(&mut self, env: Environment);

    /// Read the [`TextDisplay`]
    fn display(&self) -> &TextDisplay;

    /// Require an action
    ///
    /// See [`TextDisplay::require_action`].
    fn require_action(&mut self, action: Action);

    /// Prepare text runs
    ///
    /// Wraps [`TextDisplay::prepare_runs`], passing parameters from the
    /// environment state.
    fn prepare_runs(&mut self) -> Result<(), InvalidFontId>;

    /// Measure required width, up to some `limit`
    ///
    /// This calls [`Self::prepare_runs`] where necessary, but does not fully
    /// prepare text for display. It is a significantly faster way to calculate
    /// the required line length than by fully preparing text.
    ///
    /// The return value is at most `limit` and is unaffected by alignment and
    /// wrap configuration of [`Environment`].
    fn measure_width(&mut self, limit: f32) -> Result<f32, InvalidFontId>;

    /// Measure required vertical height, wrapping as configured
    ///
    /// This partially prepares text for display. Remaining prepartion should be
    /// fast.
    fn measure_height(&mut self) -> Result<f32, InvalidFontId>;

    /// Prepare text for display, as necessary
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text.
    ///
    /// Required preparation actions are tracked internally, but cannot
    /// notice changes in the environment. In case the environment has changed
    /// one should either call [`TextDisplay::require_action`] before this method.
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    fn prepare(&mut self) -> Result<bool, InvalidFontId>;

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

impl<T: FormattableText + ?Sized> TextApi for Text<T> {
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
    fn env(&self) -> Environment {
        self.env
    }

    #[inline]
    fn set_env(&mut self, env: Environment) {
        let action;
        if env.font_id != self.env.font_id || env.direction != self.env.direction {
            action = Action::All;
        } else if env.dpem != self.env.dpem {
            action = Action::Resize;
        } else if env.bounds.0 != self.env.bounds.0
            || env.align.0 != self.env.align.0
            || env.wrap != self.env.wrap
        {
            action = Action::Wrap;
        } else if env.bounds.1 != self.env.bounds.1 || env.align.1 != self.env.align.1 {
            action = Action::VAlign;
        } else {
            debug_assert_eq!(env, self.env);
            action = Action::None;
        }
        self.display.require_action(action);
        self.env = env;
    }

    #[inline]
    fn require_action(&mut self, action: Action) {
        self.display.require_action(action);
    }

    #[inline]
    fn prepare_runs(&mut self) -> Result<(), InvalidFontId> {
        self.display.prepare_runs(
            &self.text,
            self.env.direction,
            self.env.font_id,
            self.env.dpem,
        )
    }

    fn measure_width(&mut self, limit: f32) -> Result<f32, InvalidFontId> {
        if self.display.required_action() > Action::Wrap {
            self.prepare_runs()?;
        }

        Ok(self.display.measure_width(limit).unwrap())
    }

    fn measure_height(&mut self) -> Result<f32, InvalidFontId> {
        let v_align = self.env.align.1;
        if v_align != Align::TL {
            self.env.align.1 = Align::TL;
            self.display.require_action(Action::VAlign);
        }

        let action = self.display.required_action();
        if action > Action::Wrap {
            self.prepare_runs()?;
        }

        let height = if action >= Action::Wrap {
            self.display
                .prepare_lines(self.env.bounds, self.env.wrap, self.env.align)
                .unwrap()
                .1
        } else if action == Action::VAlign {
            self.display
                .vertically_align(self.env.bounds.1, self.env.align.1)
                .unwrap()
                .1
        } else {
            (self.display.bounding_box().unwrap().1).1
        };

        if v_align != Align::TL {
            self.env.align.1 = v_align;
            self.display.require_action(Action::VAlign);
        }

        Ok(height)
    }

    #[inline]
    fn prepare(&mut self) -> Result<bool, InvalidFontId> {
        self.prepare_runs()?;

        let action = self.display.required_action();
        let bound = if action == Action::Wrap {
            self.display
                .prepare_lines(self.env.bounds, self.env.wrap, self.env.align)
                .unwrap()
        } else if action == Action::VAlign {
            self.display
                .vertically_align(self.env.bounds.1, self.env.align.1)
                .unwrap()
        } else {
            return Ok(false);
        };

        Ok(!(bound.0 <= self.env.bounds.0 && bound.1 <= self.env.bounds.1))
    }

    #[inline]
    fn effect_tokens(&self) -> &[Effect<()>] {
        self.text.effect_tokens()
    }
}

/// Extension trait over [`TextApi`]
pub trait TextApiExt: TextApi {
    /// Update the environment and do full preparation
    ///
    /// Fully prepares the text. This is equivalent to calling
    /// [`TextApi::set_env`] followed by [`TextApi::prepare`].
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    fn update_env(&mut self, env: Environment) -> Result<bool, InvalidFontId> {
        self.set_env(env);
        self.prepare()
    }

    /// Prepare text for display, failing fast if fonts are not loaded
    ///
    /// This is identical to [`TextApi::prepare`] except that it will fail fast in
    /// case no fonts have been loaded yet (see [`fonts::any_loaded`]).
    fn try_prepare(&mut self) -> Result<bool, InvalidFontId> {
        if !fonts::any_loaded() {
            return Err(InvalidFontId);
        }
        self.prepare()
    }

    /// Get the size of the required bounding box
    ///
    /// This is the position of the upper-left and lower-right corners of a
    /// bounding box on content.
    /// Alignment and input bounds do affect the result.
    fn bounding_box(&self) -> Result<(Vec2, Vec2), NotReady> {
        self.display().bounding_box()
    }

    /// Get required action
    #[inline]
    fn required_action(&self) -> Action {
        self.display().action
    }

    /// Get the number of lines (after wrapping)
    ///
    /// See [`TextDisplay::num_lines`].
    #[inline]
    fn num_lines(&self) -> Result<usize, NotReady> {
        self.display().num_lines()
    }

    /// Find the line containing text `index`
    ///
    /// See [`TextDisplay::find_line`].
    #[inline]
    fn find_line(&self, index: usize) -> Result<Option<(usize, std::ops::Range<usize>)>, NotReady> {
        self.display().find_line(index)
    }

    /// Get the range of a line, by line number
    ///
    /// See [`TextDisplay::line_range`].
    #[inline]
    fn line_range(&self, line: usize) -> Result<Option<std::ops::Range<usize>>, NotReady> {
        self.display().line_range(line)
    }

    /// Get the directionality of the first line
    fn text_is_rtl(&self) -> Result<bool, NotReady> {
        Ok(match self.display().line_is_rtl(0)? {
            None => matches!(self.env().direction, Direction::BidiRtl | Direction::Rtl),
            Some(is_rtl) => is_rtl,
        })
    }

    /// Get the directionality of the current line
    ///
    /// See [`TextDisplay::line_is_rtl`].
    #[inline]
    fn line_is_rtl(&self, line: usize) -> Result<Option<bool>, NotReady> {
        self.display().line_is_rtl(line)
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// See [`TextDisplay::text_index_nearest`].
    #[inline]
    fn text_index_nearest(&self, pos: Vec2) -> Result<usize, NotReady> {
        self.display().text_index_nearest(pos)
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// See [`TextDisplay::line_index_nearest`].
    #[inline]
    fn line_index_nearest(&self, line: usize, x: f32) -> Result<Option<usize>, NotReady> {
        self.display().line_index_nearest(line, x)
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// See [`TextDisplay::text_glyph_pos`].
    fn text_glyph_pos(&self, index: usize) -> Result<MarkerPosIter, NotReady> {
        self.display().text_glyph_pos(index)
    }

    /// Get the number of glyphs
    ///
    /// See [`TextDisplay::num_glyphs`].
    #[inline]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "num_glyphs")))]
    #[cfg(feature = "num_glyphs")]
    fn num_glyphs(&self) -> usize {
        self.display().num_glyphs()
    }

    /// Yield a sequence of positioned glyphs
    ///
    /// See [`TextDisplay::glyphs`].
    fn glyphs<F: FnMut(FaceId, f32, Glyph)>(&self, f: F) -> Result<(), NotReady> {
        self.display().glyphs(f)
    }

    /// Like [`TextDisplay::glyphs`] but with added effects
    ///
    /// See [`TextDisplay::glyphs_with_effects`].
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

    /// Yield a sequence of rectangles to highlight a given text range
    ///
    /// Calls `f(top_left, bottom_right)` for each highlighting rectangle.
    fn highlight_range<F>(&self, range: std::ops::Range<usize>, mut f: F) -> Result<(), NotReady>
    where
        F: FnMut(Vec2, Vec2),
    {
        self.display().highlight_range(range, &mut f)
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

impl<T: EditableText + ?Sized> EditableTextApi for Text<T> {
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

impl<T: FormattableText + ?Sized> AsRef<TextDisplay> for Text<T> {
    fn as_ref(&self) -> &TextDisplay {
        &self.display
    }
}

impl<T: FormattableText + ?Sized> AsMut<TextDisplay> for Text<T> {
    fn as_mut(&mut self) -> &mut TextDisplay {
        &mut self.display
    }
}
