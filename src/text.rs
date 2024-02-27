// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use crate::display::{Effect, MarkerPosIter, NotReady, TextDisplay};
use crate::fonts::{fonts, FaceId, FontId, InvalidFontId};
use crate::format::{EditableText, FormattableText};
use crate::{Action, Align, Environment, Glyph, Vec2};
use std::ops::{Deref, DerefMut};

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
    font_id: FontId,
    dpem: f32,
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
            font_id: FontId::default(),
            dpem: 16.0,
            text,
            display: Default::default(),
        }
    }

    /// Replace the [`TextDisplay`]
    ///
    /// This may be used with [`Self::new`] to reconstruct an object which was
    /// disolved [`into_parts`][Self::into_parts].
    #[inline]
    pub fn with_display(mut self, display: TextDisplay) -> Self {
        self.display = display;
        self
    }

    /// Decompose into parts
    #[inline]
    pub fn into_parts(self) -> (Environment, TextDisplay, T) {
        (self.env, self.display, self.text)
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
            return; // no change
        }
         */

        self.text = text;
        self.display.require_action(Action::Break);
    }

    /// Set the text and prepare (if any fonts are loaded)
    ///
    /// Sets `text` regardless of other outcomes.
    ///
    /// If the text has not yet been configured, this fails with `Err(NotReady)`
    /// after setting the text. This error can be ignored when the `Text` object
    /// is scheduled to be configured and prepared later.
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    #[inline]
    pub fn set_and_prepare(&mut self, text: T) -> Result<bool, NotReady> {
        self.set_text(text);
        self.prepare()
    }
}

impl<T: FormattableText + ?Sized> Text<T> {
    #[inline]
    fn prepare_runs(&mut self) -> Result<(), NotReady> {
        match self.display.required_action() {
            Action::Configure => Err(NotReady),
            Action::Break => self
                .display
                .prepare_runs(&self.text, self.env.direction, self.font_id, self.dpem)
                .map_err(|_| {
                    debug_assert!(false, "font_id should be validated by configure");
                    NotReady
                }),
            Action::Resize => Ok(self.display.resize_runs(&self.text, self.dpem)),
            _ => Ok(()),
        }
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

    /// Get the default font
    fn get_font(&self) -> FontId;

    /// Set the default [`FontId`]
    ///
    /// This `font_id` is used by all unformatted texts and by any formatted
    /// texts which don't immediately set formatting.
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    fn set_font(&mut self, font_id: FontId);

    /// Get the default font size (pixels)
    fn get_font_size(&self) -> f32;

    /// Set the default font size (pixels)
    ///
    /// This is a scaling factor used to convert font sizes, with units
    /// `pixels/Em`. Equivalently, this is the line-height in pixels.
    /// See [`crate::fonts`] documentation.
    ///
    /// To calculate this from text size in Points, use `dpem = dpp * pt_size`
    /// where the dots-per-point is usually `dpp = scale_factor * 96.0 / 72.0`
    /// on PC platforms, or `dpp = 1` on MacOS (or 2 for retina displays).
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    fn set_font_size(&mut self, dpem: f32);

    /// Read the [`TextDisplay`]
    fn display(&self) -> &TextDisplay;

    /// Configure text
    ///
    /// Text objects must be configured before used.
    fn configure(&mut self) -> Result<(), InvalidFontId>;

    /// Measure required width, up to some `max_width`
    ///
    /// [`configure`][Self::configure] must be called before this method.
    ///
    /// This method partially prepares the [`TextDisplay`] as required.
    ///
    /// This method allows calculation of the width requirement of a text object
    /// without full wrapping and glyph placement. Whenever the requirement
    /// exceeds `max_width`, the algorithm stops early, returning `max_width`.
    ///
    /// The return value is unaffected by alignment and wrap configuration.
    fn measure_width(&mut self, max_width: f32) -> Result<f32, NotReady>;

    /// Measure required vertical height, wrapping as configured
    ///
    /// [`configure`][Self::configure] must be called before this method.
    ///
    /// This method performs most required preparation steps of the
    /// [`TextDisplay`]. Remaining prepartion should be fast.
    fn measure_height(&mut self) -> Result<f32, NotReady>;

    /// Prepare text for display, as necessary
    ///
    /// [`configure`][Self::configure] must be called before this method.
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text.
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    fn prepare(&mut self) -> Result<bool, NotReady>;

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
        if env.direction != self.env.direction {
            action = Action::Break;
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
    fn get_font(&self) -> FontId {
        self.font_id
    }

    #[inline]
    fn set_font(&mut self, font_id: FontId) {
        if font_id != self.font_id {
            self.font_id = font_id;
            self.require_action(Action::Break);
        }
    }

    #[inline]
    fn get_font_size(&self) -> f32 {
        self.dpem
    }

    #[inline]
    fn set_font_size(&mut self, dpem: f32) {
        if dpem != self.dpem {
            self.dpem = dpem;
            self.require_action(Action::Resize);
        }
    }

    #[inline]
    fn configure(&mut self) -> Result<(), InvalidFontId> {
        // Validate default_font_id
        let _ = fonts().first_face_for(self.font_id)?;

        self.display.configure()
    }

    fn measure_width(&mut self, max_width: f32) -> Result<f32, NotReady> {
        self.prepare_runs()?;

        Ok(self.display.measure_width(max_width).unwrap())
    }

    fn measure_height(&mut self) -> Result<f32, NotReady> {
        let v_align = self.env.align.1;
        if v_align != Align::TL {
            self.env.align.1 = Align::TL;
            self.display.require_action(Action::VAlign);
        }

        self.prepare_runs()?;

        let action = self.display.required_action();
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
    fn prepare(&mut self) -> Result<bool, NotReady> {
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
    /// Check whether the text is fully prepared and ready for usage
    #[inline]
    fn is_prepared(&self) -> bool {
        self.display().required_action().is_ready()
    }

    /// Set font size
    ///
    /// This is an alternative to [`TextApi::set_font_size`]. It is assumed
    /// that 72 Points = 1 Inch and the base screen resolution is 96 DPI.
    /// (Note: MacOS uses a different definition where 1 Point = 1 Pixel.)
    fn set_font_size_pt(&mut self, pt_size: f32, scale_factor: f32) {
        self.set_font_size(pt_size * scale_factor * (96.0 / 72.0));
    }

    /// Returns the height of horizontal text
    ///
    /// Returns an error if called before [`configure`][TextApi::configure].
    ///
    /// This depends on the font and font size, but is independent of the text.
    #[inline]
    fn line_height(&self) -> Result<f32, NotReady> {
        if self.display().required_action() >= Action::Configure {
            return Err(NotReady);
        }

        fonts()
            .get_first_face(self.get_font())
            .map(|face| face.height(self.get_font_size()))
            .map_err(|_| {
                debug_assert!(false, "font_id should be validated by configure");
                NotReady
            })
    }

    /// Update the environment and do full preparation
    ///
    /// Fully prepares the text. This is equivalent to calling
    /// [`TextApi::set_env`] followed by [`TextApi::prepare`].
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the allocated bounds ([`Environment::bounds`]).
    fn update_env(&mut self, env: Environment) -> Result<bool, NotReady> {
        self.set_env(env);
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

    /// Get the base directionality of the text
    ///
    /// This does not require that the text is prepared.
    #[inline]
    fn text_is_rtl(&self) -> bool {
        self.display()
            .text_is_rtl(self.as_str(), self.env().direction)
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
        self.display.require_action(Action::Break);
    }

    #[inline]
    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.text.replace_range(range, replace_with);
        self.display.require_action(Action::Break);
    }

    #[inline]
    fn set_string(&mut self, string: String) {
        self.text.set_string(string);
        self.display.require_action(Action::Break);
    }

    #[inline]
    fn swap_string(&mut self, string: &mut String) {
        self.text.swap_string(string);
        self.display.require_action(Action::Break);
    }
}

impl<T: FormattableText + ?Sized> Deref for Text<T> {
    type Target = TextDisplay;

    fn deref(&self) -> &TextDisplay {
        &self.display
    }
}

impl<T: FormattableText + ?Sized> DerefMut for Text<T> {
    fn deref_mut(&mut self) -> &mut TextDisplay {
        &mut self.display
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
