// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — simple data types

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent: for things that want to stretch it means
    /// stretch, for things which don't (text), it means align-to-start.
    Default,
    /// Align to top / left
    TL,
    /// Align to centre
    Centre,
    /// Align to bottom / right
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

/// 2D size over `f32`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size(pub f32, pub f32);

impl Size {
    /// Return the minimum, componentwise
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Size(self.0.min(other.0), self.1.min(other.1))
    }

    /// Return the maximum, componentwise
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Size(self.0.max(other.0), self.1.max(other.1))
    }
}

impl std::ops::Add for Size {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Size(self.0 + other.0, self.1 + other.1)
    }
}

impl std::ops::Sub for Size {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Size(self.0 - other.0, self.1 - other.1)
    }
}

impl From<Size> for (f32, f32) {
    fn from(size: Size) -> Self {
        (size.0, size.1)
    }
}

impl From<Size> for ab_glyph::Point {
    fn from(size: Size) -> ab_glyph::Point {
        ab_glyph::Point {
            x: size.0,
            y: size.1,
        }
    }
}

impl From<ab_glyph::Point> for Size {
    fn from(p: ab_glyph::Point) -> Size {
        Size(p.x, p.y)
    }
}

/// Font scale
///
/// This is approximately the pixel-height of a line of text or double the
/// "pt" size. Usually you want to use the same scale for both components,
/// e.g. `FontScale::from(18.0)`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontScale {
    pub x: f32,
    pub y: f32,
}

impl Default for FontScale {
    fn default() -> Self {
        FontScale::from(1.0)
    }
}

impl From<f32> for FontScale {
    fn from(scale: f32) -> Self {
        FontScale { x: scale, y: scale }
    }
}

impl From<FontScale> for ab_glyph::PxScale {
    fn from(FontScale { x, y }: FontScale) -> ab_glyph::PxScale {
        ab_glyph::PxScale { x, y }
    }
}

impl std::ops::Mul<f32> for FontScale {
    type Output = Self;

    #[inline]
    fn mul(self, f: f32) -> Self::Output {
        FontScale {
            x: self.x * f,
            y: self.y * f,
        }
    }
}
