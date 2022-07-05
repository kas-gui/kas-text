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
use crate::{Direction, Environment};

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
    fn prepare_runs(&mut self);

    /// Measure required width, up to some `limit`
    ///
    /// This calls [`Self::prepare_runs`] where necessary, but does not fully
    /// prepare text for display.
    fn measure_width(&mut self, limit: f32) -> f32;

    /// Measure required vertical height, wrapping as configured
    ///
    /// This fully prepares text for display.
    fn measure_height(&mut self) -> f32;

    /// Prepare lines ("wrap")
    ///
    /// Prepares text, returning bottom-right corner of bounding box on output.
    ///
    /// This differs slightly from [`TextDisplay::prepare_lines`] in that it does all preparation
    /// necessary. It differs from [`Self::prepare`] in that this method always (re)calculates
    /// text wrapping and returns size requirements.
    fn prepare_lines(&mut self) -> Vec2;

    /// Prepare text for display, as necessary
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text.
    ///
    /// Required preparation actions are tracked internally, but cannot
    /// notice changes in the environment. In case the environment has changed
    /// one should either call [`TextDisplay::require_action`] before this method.
    ///
    /// Returns bottom-right corner of bounding box on output
    /// when an update action (other than vertical alignment) occurred.
    /// Returns `None` if no significant action was required
    /// (since requirements are computed as a
    /// side-effect of line-wrapping, and presumably in this case the existing
    /// allocation is sufficient). One may force calculation of this value by
    /// calling `text.require_action(Action::Wrap)`.
    fn prepare(&mut self) -> Option<Vec2>;

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
    fn prepare_runs(&mut self) {
        self.display.prepare_runs(
            &self.text,
            self.env.direction,
            self.env.font_id,
            self.env.dpem,
        );
    }

    fn measure_width(&mut self, limit: f32) -> f32 {
        if self.display.required_action() > Action::Wrap {
            self.prepare_runs();
        }

        self.display.measure_width(limit).unwrap()
    }

    fn measure_height(&mut self) -> f32 {
        let action = self.display.required_action();
        if action > Action::Wrap {
            self.prepare_runs();
        }

        if action >= Action::Wrap {
            let result = self
                .display
                .prepare_lines(self.env.bounds, self.env.wrap, self.env.align)
                .unwrap();
            result.1
        } else if action == Action::VAlign {
            self.display
                .vertically_align(self.env.bounds.1, self.env.align.1)
                .unwrap()
        } else {
            self.display.bounding_box().unwrap().1
        }
    }

    #[inline]
    fn prepare_lines(&mut self) -> Vec2 {
        self.prepare_runs();

        self.display
            .prepare_lines(self.env.bounds, self.env.wrap, self.env.align)
            .unwrap()
    }

    #[inline]
    fn prepare(&mut self) -> Option<Vec2> {
        self.prepare_runs();

        let action = self.display.required_action();
        if action == Action::Wrap {
            self.display
                .prepare_lines(self.env.bounds, self.env.wrap, self.env.align)
                .ok()
        } else if action == Action::VAlign {
            self.display
                .vertically_align(self.env.bounds.1, self.env.align.1)
                .unwrap();
            None
        } else {
            None
        }
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
    /// Returns the bottom-right corner of bounding box on output.
    ///
    /// This prepares text as necessary. It always performs line-wrapping.
    fn update_env(&mut self, env: Environment) -> Option<Vec2> {
        self.set_env(env);
        self.prepare()
    }

    /// Get the size of the required bounding box
    fn bounding_box(&mut self) -> Vec2 {
        self.prepare();
        self.display().bounding_box().unwrap()
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

    /// Yield a sequence of rectangles to highlight a given range, by lines
    ///
    /// See [`TextDisplay::highlight_lines`].
    fn highlight_lines(
        &self,
        range: std::ops::Range<usize>,
    ) -> Result<Vec<(Vec2, Vec2)>, NotReady> {
        self.display().highlight_lines(range)
    }

    /// Yield a sequence of rectangles to highlight a given range, by runs
    ///
    /// See [`TextDisplay::highlight_runs`].
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
