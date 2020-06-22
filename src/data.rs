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

#[derive(Clone, Copy, Default)]
pub struct Size(pub f32, pub f32);

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
        FontScale::from(18.0)
    }
}

impl From<f32> for FontScale {
    fn from(scale: f32) -> Self {
        FontScale { x: scale, y: scale }
    }
}

/// Font identifier
///
/// A default font may be obtained with `FontId(0)`, which refers to the
/// first font loaded by the (first) theme.
///
/// Other than this, users should treat this type as an opaque handle.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub usize);
