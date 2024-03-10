// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use crate::display::{Effect, MarkerPosIter, NotReady, TextDisplay};
use crate::fonts::{fonts, FaceId, FontId, InvalidFontId};
use crate::format::{EditableText, FormattableText};
use crate::{Align, Direction, Glyph, Status, Vec2};
use std::ops::{Deref, DerefMut};

/// Text, prepared for display in a given environment
///
/// This struct is composed of two parts: a representation
/// of the [`FormattableText`] being displayed, and a [`TextDisplay`] object.
///
/// Most Functionality is implemented via the [`TextApi`] and [`TextApiExt`]
/// traits.
#[derive(Clone, Debug, Default)]
pub struct Text<T: FormattableText + ?Sized> {
    font_id: FontId,
    dpem: f32,
    direction: Direction,
    wrap_width: f32,
    /// Alignment (`horiz`, `vert`)
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Self::direction`]), and vertical alignment
    /// is to the top.
    align: (Align, Align),
    /// Bounds to use for alignment
    bounds: Vec2,

    display: TextDisplay,
    text: T,
}

impl<T: FormattableText> Text<T> {
    /// Construct from a text model
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    #[inline]
    pub fn new(text: T) -> Self {
        Text {
            font_id: FontId::default(),
            dpem: 16.0,
            direction: Direction::default(),
            wrap_width: f32::INFINITY,
            align: Default::default(),
            bounds: Vec2::INFINITY,
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
    pub fn into_parts(self) -> (TextDisplay, T) {
        (self.display, self.text)
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
        self.display.set_max_status(Status::Configured);
    }
}

impl<T: FormattableText + ?Sized> Text<T> {
    #[inline]
    fn prepare_runs(&mut self) -> Result<(), NotReady> {
        match self.display.status() {
            Status::New => Err(NotReady),
            Status::Configured => self
                .display
                .prepare_runs(&self.text, self.direction, self.font_id, self.dpem)
                .map_err(|_| {
                    debug_assert!(false, "font_id should be validated by configure");
                    NotReady
                }),
            Status::ResizeLevelRuns => Ok(self.display.resize_runs(&self.text, self.dpem)),
            _ => Ok(()),
        }
    }
}

/// Trait over a sub-set of [`Text`] functionality
///
/// This allows dynamic dispatch over [`Text`]'s type parameters.
pub trait TextApi {
    /// Check whether the status is at least `status`
    fn check_status(&self, status: Status) -> Result<(), NotReady>;

    /// Read the [`TextDisplay`], without checking status
    fn unchecked_display(&self) -> &TextDisplay;

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

    /// Get the default font
    fn get_font(&self) -> FontId;

    /// Get text (horizontal, vertical) alignment
    fn get_align(&self) -> (Align, Align);

    /// Set text alignment
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    fn set_align(&mut self, align: (Align, Align));

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

    /// Get the base text direction
    fn get_direction(&self) -> Direction;

    /// Set the base text direction
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    fn set_direction(&mut self, direction: Direction);

    /// Get the text wrap width
    fn get_wrap_width(&self) -> f32;

    /// Set wrap width or disable line wrapping
    ///
    /// By default, this is [`f32::INFINITY`] and text lines are not wrapped.
    /// If set to some positive finite value, text lines will be wrapped at that
    /// width.
    ///
    /// Either way, explicit line-breaks such as `\n` still result in new lines.
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    fn set_wrap_width(&mut self, wrap_width: f32);

    /// Get text bounds
    fn get_bounds(&self) -> Vec2;

    /// Set text bounds
    ///
    /// These are used for alignment. They are not used for wrapping; see
    /// instead [`Self::set_wrap_width`].
    ///
    /// It is expected that `bounds` are finite.
    fn set_bounds(&mut self, bounds: Vec2);

    /// Set font properties
    ///
    /// This has the same effect as calling the following functions separately:
    ///
    /// -   [`Self::set_direction`]
    /// -   [`Self::set_font`]
    /// -   [`Self::set_font_size`]
    /// -   [`Self::set_wrap_width`]
    fn set_font_properties(
        &mut self,
        direction: Direction,
        font_id: FontId,
        dpem: f32,
        wrap_width: f32,
    );

    /// Get the base directionality of the text
    ///
    /// This does not require that the text is prepared.
    fn text_is_rtl(&self) -> bool;

    /// Configure text
    ///
    /// Text objects must be configured before used.
    fn configure(&mut self) -> Result<(), InvalidFontId>;

    /// Returns the height of horizontal text
    ///
    /// Returns an error if called before [`Self::configure`].
    ///
    /// This depends on the font and font size, but is independent of the text.
    fn line_height(&self) -> Result<f32, NotReady>;

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
    fn measure_height(&mut self) -> Result<f32, NotReady>;

    /// Prepare text for display, as necessary
    ///
    /// [`Self::configure`] and [`Self::set_bounds`] must be called before this
    /// method.
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text. Text is aligned within the given `bounds`.
    ///
    /// Returns true if at least some action is performed *and* the text exceeds
    /// the [bounds][Self::set_bounds].
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
    fn check_status(&self, status: Status) -> Result<(), NotReady> {
        if self.display.status() >= status {
            Ok(())
        } else {
            Err(NotReady)
        }
    }

    #[inline]
    fn unchecked_display(&self) -> &TextDisplay {
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
    fn get_font(&self) -> FontId {
        self.font_id
    }

    #[inline]
    fn set_font(&mut self, font_id: FontId) {
        if font_id != self.font_id {
            self.font_id = font_id;
            self.set_max_status(Status::Configured);
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
            self.set_max_status(Status::ResizeLevelRuns);
        }
    }

    #[inline]
    fn get_direction(&self) -> Direction {
        self.direction
    }

    #[inline]
    fn set_direction(&mut self, direction: Direction) {
        if direction != self.direction {
            self.direction = direction;
            self.set_max_status(Status::Configured);
        }
    }

    #[inline]
    fn get_wrap_width(&self) -> f32 {
        self.wrap_width
    }

    #[inline]
    fn set_wrap_width(&mut self, wrap_width: f32) {
        assert!(self.wrap_width >= 0.0);
        if wrap_width != self.wrap_width {
            self.wrap_width = wrap_width;
            self.set_max_status(Status::LevelRuns);
        }
    }

    #[inline]
    fn get_align(&self) -> (Align, Align) {
        self.align
    }

    #[inline]
    fn set_align(&mut self, align: (Align, Align)) {
        if align != self.align {
            if align.0 == self.align.0 {
                self.set_max_status(Status::Wrapped);
            } else {
                self.set_max_status(Status::LevelRuns);
            }
            self.align = align;
        }
    }

    #[inline]
    fn get_bounds(&self) -> Vec2 {
        self.bounds
    }

    #[inline]
    fn set_bounds(&mut self, bounds: Vec2) {
        debug_assert!(bounds.is_finite());
        if bounds != self.bounds {
            if bounds.0 != self.bounds.0 {
                self.set_max_status(Status::LevelRuns);
            } else {
                self.set_max_status(Status::Wrapped);
            }
            self.bounds = bounds;
        }
    }

    fn set_font_properties(
        &mut self,
        direction: Direction,
        font_id: FontId,
        dpem: f32,
        wrap_width: f32,
    ) {
        self.set_font(font_id);
        self.set_font_size(dpem);
        self.set_direction(direction);
        self.set_wrap_width(wrap_width);
    }

    #[inline]
    fn text_is_rtl(&self) -> bool {
        let cached_is_rtl = match self.line_is_rtl(0) {
            Ok(None) => Some(self.direction == Direction::Rtl),
            Ok(Some(is_rtl)) => Some(is_rtl),
            Err(NotReady) => None,
        };
        #[cfg(not(debug_assertions))]
        if let Some(cached) = cached_is_rtl {
            return cached;
        }

        let is_rtl = self.display.text_is_rtl(self.as_str(), self.direction);
        if let Some(cached) = cached_is_rtl {
            debug_assert_eq!(cached, is_rtl);
        }
        is_rtl
    }

    #[inline]
    fn configure(&mut self) -> Result<(), InvalidFontId> {
        // Validate default_font_id
        let _ = fonts().first_face_for(self.font_id)?;

        self.display.configure()
    }

    fn line_height(&self) -> Result<f32, NotReady> {
        self.check_status(Status::Configured)?;

        fonts()
            .get_first_face(self.get_font())
            .map(|face| face.height(self.get_font_size()))
            .map_err(|_| {
                debug_assert!(false, "font_id should be validated by configure");
                NotReady
            })
    }

    fn measure_width(&mut self, max_width: f32) -> Result<f32, NotReady> {
        self.prepare_runs()?;

        Ok(self.display.measure_width(max_width))
    }

    fn measure_height(&mut self) -> Result<f32, NotReady> {
        if self.display.status() >= Status::Wrapped {
            let (tl, br) = self.display.bounding_box();
            return Ok(br.1 - tl.1);
        }

        self.prepare_runs()?;
        Ok(self.display.measure_height(self.wrap_width))
    }

    #[inline]
    fn prepare(&mut self) -> Result<bool, NotReady> {
        self.prepare_runs()?;

        let status = self.display.status();
        debug_assert!(status >= Status::LevelRuns);

        if status == Status::LevelRuns {
            self.display
                .prepare_lines(self.wrap_width, self.bounds.0, self.align.0);
        }

        Ok(if status <= Status::Wrapped {
            debug_assert!(self.display.status() >= Status::Wrapped);
            let bound = self.display.vertically_align(self.bounds.1, self.align.1);
            !(bound.0 <= self.bounds.0 && bound.1 <= self.bounds.1)
        } else {
            false
        })
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
        self.check_status(Status::Ready).is_ok()
    }

    /// Read the [`TextDisplay`], if fully prepared
    #[inline]
    fn display(&self) -> Result<&TextDisplay, NotReady> {
        self.check_status(Status::Ready)?;
        Ok(self.unchecked_display())
    }

    /// Read the [`TextDisplay`], if at least wrapped
    #[inline]
    fn wrapped_display(&self) -> Result<&TextDisplay, NotReady> {
        self.check_status(Status::Wrapped)?;
        Ok(self.unchecked_display())
    }

    /// Set font size
    ///
    /// This is an alternative to [`TextApi::set_font_size`]. It is assumed
    /// that 72 Points = 1 Inch and the base screen resolution is 96 DPI.
    /// (Note: MacOS uses a different definition where 1 Point = 1 Pixel.)
    #[inline]
    fn set_font_size_pt(&mut self, pt_size: f32, scale_factor: f32) {
        self.set_font_size(pt_size * scale_factor * (96.0 / 72.0));
    }

    /// Get the size of the required bounding box
    ///
    /// This is the position of the upper-left and lower-right corners of a
    /// bounding box on content.
    /// Alignment and input bounds do affect the result.
    #[inline]
    fn bounding_box(&self) -> Result<(Vec2, Vec2), NotReady> {
        Ok(self.wrapped_display()?.bounding_box())
    }

    /// Get the number of lines (after wrapping)
    ///
    /// See [`TextDisplay::num_lines`].
    #[inline]
    fn num_lines(&self) -> Result<usize, NotReady> {
        Ok(self.wrapped_display()?.num_lines())
    }

    /// Find the line containing text `index`
    ///
    /// See [`TextDisplay::find_line`].
    #[inline]
    fn find_line(&self, index: usize) -> Result<Option<(usize, std::ops::Range<usize>)>, NotReady> {
        Ok(self.wrapped_display()?.find_line(index))
    }

    /// Get the range of a line, by line number
    ///
    /// See [`TextDisplay::line_range`].
    #[inline]
    fn line_range(&self, line: usize) -> Result<Option<std::ops::Range<usize>>, NotReady> {
        Ok(self.wrapped_display()?.line_range(line))
    }

    /// Get the directionality of the current line
    ///
    /// See [`TextDisplay::line_is_rtl`].
    #[inline]
    fn line_is_rtl(&self, line: usize) -> Result<Option<bool>, NotReady> {
        Ok(self.wrapped_display()?.line_is_rtl(line))
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// See [`TextDisplay::text_index_nearest`].
    #[inline]
    fn text_index_nearest(&self, pos: Vec2) -> Result<usize, NotReady> {
        Ok(self.display()?.text_index_nearest(pos))
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// See [`TextDisplay::line_index_nearest`].
    #[inline]
    fn line_index_nearest(&self, line: usize, x: f32) -> Result<Option<usize>, NotReady> {
        Ok(self.wrapped_display()?.line_index_nearest(line, x))
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// See [`TextDisplay::text_glyph_pos`].
    fn text_glyph_pos(&self, index: usize) -> Result<MarkerPosIter, NotReady> {
        Ok(self.display()?.text_glyph_pos(index))
    }

    /// Get the number of glyphs
    ///
    /// See [`TextDisplay::num_glyphs`].
    #[inline]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "num_glyphs")))]
    #[cfg(feature = "num_glyphs")]
    fn num_glyphs(&self) -> Result<usize, NotReady> {
        Ok(self.wrapped_display()?.num_glyphs())
    }

    /// Yield a sequence of positioned glyphs
    ///
    /// See [`TextDisplay::glyphs`].
    fn glyphs<F: FnMut(FaceId, f32, Glyph)>(&self, f: F) -> Result<(), NotReady> {
        Ok(self.display()?.glyphs(f))
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
        Ok(self
            .display()?
            .glyphs_with_effects(effects, default_aux, f, g))
    }

    /// Yield a sequence of rectangles to highlight a given text range
    ///
    /// Calls `f(top_left, bottom_right)` for each highlighting rectangle.
    fn highlight_range<F>(&self, range: std::ops::Range<usize>, mut f: F) -> Result<(), NotReady>
    where
        F: FnMut(Vec2, Vec2),
    {
        Ok(self.display()?.highlight_range(range, &mut f))
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
        self.display.set_max_status(Status::Configured);
    }

    #[inline]
    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.text.replace_range(range, replace_with);
        self.display.set_max_status(Status::Configured);
    }

    #[inline]
    fn set_string(&mut self, string: String) {
        self.text.set_string(string);
        self.display.set_max_status(Status::Configured);
    }

    #[inline]
    fn swap_string(&mut self, string: &mut String) {
        self.text.swap_string(string);
        self.display.set_max_status(Status::Configured);
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
