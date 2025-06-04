// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text object

use crate::display::{Effect, MarkerPosIter, NotReady, TextDisplay};
use crate::fonts::{FaceId, FontSelector, NoFontMatch};
use crate::format::{EditableText, FormattableText};
use crate::{Align, Direction, Glyph, GlyphRun, Status, Vec2};

/// Text type-setting object (high-level API)
///
/// This struct contains:
/// -   A [`FormattableText`]
/// -   A [`TextDisplay`]
/// -   A [`FontSelector`]
/// -   Font size; this defaults to 16px (the web default).
/// -   Text direction and alignment; by default this is inferred from the text.
/// -   Line-wrap width; see [`Text::set_wrap_width`].
/// -   The bounds used for alignment; these [must be set][Text::set_bounds].
///
/// This struct tracks the [`TextDisplay`]'s
/// [state of preparation][TextDisplay#status-of-preparation] and will perform
/// steps as required. To use this struct:
/// ```
/// use kas_text::{Text, Vec2};
/// use std::path::Path;
///
/// let mut text = Text::new("Hello, world!");
/// text.set_bounds(Vec2(200.0, 50.0));
/// text.prepare().unwrap();
///
/// for run in text.runs().unwrap() {
///     let (face, dpem) = (run.face_id(), run.dpem());
///     for glyph in run.glyphs() {
///         println!("{face:?} - {dpem}px - {glyph:?}");
///     }
/// }
/// ```
#[derive(Clone, Debug)]
pub struct Text<T: FormattableText + ?Sized> {
    /// Bounds to use for alignment
    bounds: Vec2,
    font: FontSelector,
    dpem: f32,
    wrap_width: f32,
    /// Alignment (`horiz`, `vert`)
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Self::direction`]), and vertical alignment
    /// is to the top.
    align: (Align, Align),
    direction: Direction,
    status: Status,

    display: TextDisplay,
    text: T,
}

impl<T: Default + FormattableText + ?Sized> Default for Text<T> {
    #[inline]
    fn default() -> Self {
        Text::new(T::default())
    }
}

/// Constructors and other methods requiring `T: Sized`
impl<T: FormattableText> Text<T> {
    /// Construct from a text model
    ///
    /// This struct must be made ready for usage by calling [`Text::prepare`].
    #[inline]
    pub fn new(text: T) -> Self {
        Text {
            bounds: Vec2::INFINITY,
            font: FontSelector::default(),
            dpem: 16.0,
            wrap_width: f32::INFINITY,
            align: Default::default(),
            direction: Direction::default(),
            status: Status::New,
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
        if self.text == text {
            return; // no change
        }

        self.text = text;
        self.set_max_status(Status::New);
    }
}

/// Text, font and type-setting getters and setters
impl<T: FormattableText + ?Sized> Text<T> {
    /// Length of text
    ///
    /// This is a shortcut to `self.as_str().len()`.
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    #[inline]
    pub fn str_len(&self) -> usize {
        self.as_str().len()
    }

    /// Access whole text as contiguous `str`
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    #[inline]
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    /// Clone the unformatted text as a `String`
    #[inline]
    pub fn clone_string(&self) -> String {
        self.text.as_str().to_string()
    }

    /// Get the font selector
    #[inline]
    pub fn font(&self) -> FontSelector {
        self.font
    }

    /// Set the font selector
    ///
    /// This font selector is used by all unformatted texts and by formatted
    /// texts which don't immediately replace the selector.
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    #[inline]
    pub fn set_font(&mut self, font: FontSelector) {
        if font != self.font {
            self.font = font;
            self.set_max_status(Status::New);
        }
    }

    /// Get the default font size (pixels)
    #[inline]
    pub fn font_size(&self) -> f32 {
        self.dpem
    }

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
    #[inline]
    pub fn set_font_size(&mut self, dpem: f32) {
        if dpem != self.dpem {
            self.dpem = dpem;
            self.set_max_status(Status::ResizeLevelRuns);
        }
    }

    /// Set font size
    ///
    /// This is an alternative to [`Text::set_font_size`]. It is assumed
    /// that 72 Points = 1 Inch and the base screen resolution is 96 DPI.
    /// (Note: MacOS uses a different definition where 1 Point = 1 Pixel.)
    #[inline]
    pub fn set_font_size_pt(&mut self, pt_size: f32, scale_factor: f32) {
        self.set_font_size(pt_size * scale_factor * (96.0 / 72.0));
    }

    /// Get the base text direction
    #[inline]
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Set the base text direction
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    #[inline]
    pub fn set_direction(&mut self, direction: Direction) {
        if direction != self.direction {
            self.direction = direction;
            self.set_max_status(Status::New);
        }
    }

    /// Get the text wrap width
    #[inline]
    pub fn wrap_width(&self) -> f32 {
        self.wrap_width
    }

    /// Set wrap width or disable line wrapping
    ///
    /// By default, this is [`f32::INFINITY`] and text lines are not wrapped.
    /// If set to some positive finite value, text lines will be wrapped at that
    /// width.
    ///
    /// Either way, explicit line-breaks such as `\n` still result in new lines.
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    #[inline]
    pub fn set_wrap_width(&mut self, wrap_width: f32) {
        debug_assert!(wrap_width >= 0.0);
        if wrap_width != self.wrap_width {
            self.wrap_width = wrap_width;
            self.set_max_status(Status::LevelRuns);
        }
    }

    /// Get text (horizontal, vertical) alignment
    #[inline]
    pub fn align(&self) -> (Align, Align) {
        self.align
    }

    /// Set text alignment
    ///
    /// It is necessary to [`prepare`][Self::prepare] the text after calling this.
    #[inline]
    pub fn set_align(&mut self, align: (Align, Align)) {
        if align != self.align {
            if align.0 == self.align.0 {
                self.set_max_status(Status::Wrapped);
            } else {
                self.set_max_status(Status::LevelRuns);
            }
            self.align = align;
        }
    }

    /// Get text bounds
    #[inline]
    pub fn bounds(&self) -> Vec2 {
        self.bounds
    }

    /// Set text bounds
    ///
    /// These are used for alignment. They are not used for wrapping; see
    /// instead [`Self::set_wrap_width`].
    ///
    /// It is expected that `bounds` are finite.
    #[inline]
    pub fn set_bounds(&mut self, bounds: Vec2) {
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

    /// Get the base directionality of the text
    ///
    /// This does not require that the text is prepared.
    pub fn text_is_rtl(&self) -> bool {
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

    /// Get the sequence of effect tokens
    ///
    /// This method has some limitations: (1) it may only return a reference to
    /// an existing sequence, (2) effect tokens cannot be generated dependent
    /// on input state, and (3) it does not incorporate color information. For
    /// most uses it should still be sufficient, but for other cases it may be
    /// preferable not to use this method (use a dummy implementation returning
    /// `&[]` and use inherent methods on the text object via [`Text::text`]).
    #[inline]
    pub fn effect_tokens(&self) -> &[Effect<()>] {
        self.text.effect_tokens()
    }
}

/// Type-setting operations and status
impl<T: FormattableText + ?Sized> Text<T> {
    /// Check whether the status is at least `status`
    #[inline]
    pub fn check_status(&self, status: Status) -> Result<(), NotReady> {
        if self.status >= status {
            Ok(())
        } else {
            Err(NotReady)
        }
    }

    /// Check whether the text is fully prepared and ready for usage
    #[inline]
    pub fn is_prepared(&self) -> bool {
        self.status == Status::Ready
    }

    /// Adjust status to indicate a required action
    ///
    /// This is used to notify that some step of preparation may need to be
    /// repeated. The internally-tracked status is set to the minimum of
    /// `status` and its previous value.
    #[inline]
    fn set_max_status(&mut self, status: Status) {
        self.status = self.status.min(status);
    }

    /// Read the [`TextDisplay`], without checking status
    #[inline]
    pub fn unchecked_display(&self) -> &TextDisplay {
        &self.display
    }

    /// Read the [`TextDisplay`], if fully prepared
    #[inline]
    pub fn display(&self) -> Result<&TextDisplay, NotReady> {
        self.check_status(Status::Ready)?;
        Ok(self.unchecked_display())
    }

    /// Read the [`TextDisplay`], if at least wrapped
    #[inline]
    pub fn wrapped_display(&self) -> Result<&TextDisplay, NotReady> {
        self.check_status(Status::Wrapped)?;
        Ok(self.unchecked_display())
    }

    #[inline]
    fn prepare_runs(&mut self) -> Result<(), NoFontMatch> {
        match self.status {
            Status::New => {
                self.display
                    .prepare_runs(&self.text, self.direction, self.font, self.dpem)?
            }
            Status::ResizeLevelRuns => self.display.resize_runs(&self.text, self.dpem),
            _ => (),
        }

        self.status = Status::LevelRuns;
        Ok(())
    }

    /// Measure required width, up to some `max_width`
    ///
    /// This method partially prepares the [`TextDisplay`] as required.
    ///
    /// This method allows calculation of the width requirement of a text object
    /// without full wrapping and glyph placement. Whenever the requirement
    /// exceeds `max_width`, the algorithm stops early, returning `max_width`.
    ///
    /// The return value is unaffected by alignment and wrap configuration.
    pub fn measure_width(&mut self, max_width: f32) -> Result<f32, NoFontMatch> {
        self.prepare_runs()?;

        Ok(self.display.measure_width(max_width))
    }

    /// Measure required vertical height, wrapping as configured
    pub fn measure_height(&mut self) -> Result<f32, NoFontMatch> {
        if self.status >= Status::Wrapped {
            let (tl, br) = self.display.bounding_box();
            return Ok(br.1 - tl.1);
        }

        self.prepare_runs()?;
        Ok(self.display.measure_height(self.wrap_width))
    }

    /// Prepare text for display, as necessary
    ///
    /// [`Self::set_bounds`] must be called before this method.
    ///
    /// Does all preparation steps necessary in order to display or query the
    /// layout of this text. Text is aligned within the given `bounds`.
    ///
    /// Returns `Ok(true)` on success when some action is performed, `Ok(false)`
    /// when the text is already prepared.
    pub fn prepare(&mut self) -> Result<bool, NotReady> {
        if self.is_prepared() {
            return Ok(false);
        } else if !self.bounds.is_finite() {
            return Err(NotReady);
        }

        self.prepare_runs().unwrap();
        debug_assert!(self.status >= Status::LevelRuns);

        if self.status == Status::LevelRuns {
            self.display
                .prepare_lines(self.wrap_width, self.bounds.0, self.align.0);
        }

        if self.status <= Status::Wrapped {
            self.display.vertically_align(self.bounds.1, self.align.1);
        }

        self.status = Status::Ready;
        Ok(true)
    }

    /// Get the size of the required bounding box
    ///
    /// This is the position of the upper-left and lower-right corners of a
    /// bounding box on content.
    /// Alignment and input bounds do affect the result.
    #[inline]
    pub fn bounding_box(&self) -> Result<(Vec2, Vec2), NotReady> {
        Ok(self.wrapped_display()?.bounding_box())
    }
    /// Get the number of lines (after wrapping)
    ///
    /// See [`TextDisplay::num_lines`].
    #[inline]
    pub fn num_lines(&self) -> Result<usize, NotReady> {
        Ok(self.wrapped_display()?.num_lines())
    }

    /// Find the line containing text `index`
    ///
    /// See [`TextDisplay::find_line`].
    #[inline]
    pub fn find_line(
        &self,
        index: usize,
    ) -> Result<Option<(usize, std::ops::Range<usize>)>, NotReady> {
        Ok(self.wrapped_display()?.find_line(index))
    }

    /// Get the range of a line, by line number
    ///
    /// See [`TextDisplay::line_range`].
    #[inline]
    pub fn line_range(&self, line: usize) -> Result<Option<std::ops::Range<usize>>, NotReady> {
        Ok(self.wrapped_display()?.line_range(line))
    }

    /// Get the directionality of the current line
    ///
    /// See [`TextDisplay::line_is_rtl`].
    #[inline]
    pub fn line_is_rtl(&self, line: usize) -> Result<Option<bool>, NotReady> {
        Ok(self.wrapped_display()?.line_is_rtl(line))
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// See [`TextDisplay::text_index_nearest`].
    #[inline]
    pub fn text_index_nearest(&self, pos: Vec2) -> Result<usize, NotReady> {
        Ok(self.display()?.text_index_nearest(pos))
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// See [`TextDisplay::line_index_nearest`].
    #[inline]
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Result<Option<usize>, NotReady> {
        Ok(self.wrapped_display()?.line_index_nearest(line, x))
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// See [`TextDisplay::text_glyph_pos`].
    pub fn text_glyph_pos(&self, index: usize) -> Result<MarkerPosIter, NotReady> {
        Ok(self.display()?.text_glyph_pos(index))
    }

    /// Get the number of glyphs
    ///
    /// See [`TextDisplay::num_glyphs`].
    #[inline]
    #[cfg(feature = "num_glyphs")]
    pub fn num_glyphs(&self) -> Result<usize, NotReady> {
        Ok(self.wrapped_display()?.num_glyphs())
    }

    /// Iterate over runs of positioned glyphs
    ///
    /// Runs are yielded in undefined order. The total number of
    /// glyphs yielded will equal [`Self::num_glyphs`].
    pub fn runs(&self) -> Result<impl Iterator<Item = GlyphRun<'_>>, NotReady> {
        Ok(self.display()?.runs())
    }

    /// Like [`TextDisplay::glyphs`] but with added effects
    ///
    /// See [`TextDisplay::glyphs_with_effects`].
    pub fn glyphs_with_effects<X, F, G>(
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
    pub fn highlight_range<F>(
        &self,
        range: std::ops::Range<usize>,
        mut f: F,
    ) -> Result<(), NotReady>
    where
        F: FnMut(Vec2, Vec2),
    {
        Ok(self.display()?.highlight_range(range, &mut f))
    }
}

/// Text editing operations
impl<T: EditableText + ?Sized> Text<T> {
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
    #[inline]
    pub fn insert_char(&mut self, index: usize, c: char) {
        self.text.insert_char(index, c);
        self.set_max_status(Status::New);
    }

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
    #[inline]
    pub fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.text.replace_range(range, replace_with);
        self.set_max_status(Status::New);
    }

    /// Set text to a raw `String`
    ///
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    #[inline]
    pub fn set_string(&mut self, string: String) {
        self.text.set_string(string);
        self.set_max_status(Status::New);
    }

    /// Swap the raw text with a `String`
    ///
    /// This may be used to edit the raw text instead of replacing it.
    /// One must call [`Text::prepare`] afterwards.
    ///
    /// All existing text formatting is removed.
    ///
    /// Currently this is not significantly more efficient than
    /// [`Text::set_text`]. This may change in the future (TODO).
    #[inline]
    pub fn swap_string(&mut self, string: &mut String) {
        self.text.swap_string(string);
        self.set_max_status(Status::New);
    }
}
