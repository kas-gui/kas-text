// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display environment

use crate::fonts::{fonts, FontId};
use crate::Vec2;

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Text direction
    ///
    /// By default, text direction (LTR or RTL) is automatically detected with
    /// full bi-directional text support (Unicode Technical Report #9).
    pub direction: Direction,
    /// Line wrapping
    ///
    /// By default, this is true and long text lines are wrapped based on the
    /// width bounds. If set to false, lines are not wrapped at the width
    /// boundary, but explicit line-breaks such as `\n` still result in new
    /// lines.
    pub wrap: bool,
    /// Alignment (`horiz`, `vert`)
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Self::direction`]), and vertical alignment is
    /// to the top.
    pub align: (Align, Align),
    /// Default font
    ///
    /// This font is used unless a formatting token (see [`crate::format`])
    /// is used.
    pub font_id: FontId,
    /// Font size: pixels per Em
    ///
    /// This is a scaling factor used to convert font sizes, with units
    /// `pixels/Em`. Equivalently, this is the line-height in pixels.
    /// See [`crate::fonts`] documentation.
    ///
    /// To calculate this from text size in Points, use `dpem = dpp * pt_size`
    /// where the dots-per-point is usually `dpp = scale_factor * 96.0 / 72.0`
    /// on PC platforms, or `dpp = 1` on MacOS (or 2 for retina displays).
    pub dpem: f32,
    /// The available (horizontal and vertical) space
    ///
    /// This defaults to infinity (implying no bounds). To enable line-wrapping
    /// set at least a horizontal bound. The vertical bound is required for
    /// alignment (when aligning to the center or bottom).
    /// Glyphs outside of these bounds may not be drawn.
    pub bounds: Vec2,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            direction: Direction::default(),
            wrap: true,
            font_id: Default::default(),
            dpem: 11.0 * 96.0 / 72.0,
            bounds: Vec2::INFINITY,
            align: Default::default(),
        }
    }
}

impl Environment {
    /// Set font size
    ///
    /// This is an alternative to setting [`Self::dpem`] directly. It is assumed
    /// that 72 Points = 1 Inch and the base screen resolution is 96 DPI.
    /// (Note: MacOS uses a different definition where 1 Point = 1 Pixel.)
    pub fn set_font_size(&mut self, pt_size: f32, scale_factor: f32) {
        self.dpem = pt_size * scale_factor * (96.0 / 72.0);
    }

    /// Returns the height of horizontal text
    ///
    /// This should be similar to the value of [`Self::dpem`], but depends on
    /// the font.
    ///
    /// To use "the standard font", use `font_id = Default::default()`.
    pub fn line_height(&self, font_id: FontId) -> f32 {
        fonts().get_first_face(font_id).height(self.dpem)
    }
}

impl Environment {
    /// Construct, with explicit font size (pixels per Em)
    pub fn new(dpem: f32) -> Self {
        Environment {
            dpem,
            ..Default::default()
        }
    }
}

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent. For example, for Left-To-Right text it means
    /// `TL`; for things which want to stretch it may mean `Stretch`.
    Default,
    /// Align to top or left
    TL,
    /// Align to center
    Center,
    /// Align to bottom or right
    BR,
    /// Stretch to fill space
    ///
    /// For text, this is known as "justified alignment".
    Stretch,
}

impl Default for Align {
    fn default() -> Self {
        Align::Default
    }
}

/// Directionality of text
///
/// This can be used to force the text direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    /// Auto-detect with bi-directional support (default)
    Bidi,
    /// Auto-detect with bi-directional support, defaulting to right-to-left
    BidiRtl,
    /// Auto-detect, single line direction only
    Single,
    /// Force left-to-right text direction
    Ltr,
    /// Force right-to-left text direction
    Rtl,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Bidi
    }
}

#[test]
fn size() {
    assert_eq!(std::mem::size_of::<Environment>(), 20);
}
