// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Simple layout engine — copied from glyph_brush_layout

use crate::FontId;
use ab_glyph::*;

/// Text to layout together using a font & scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionText<'a> {
    /// Text to render
    pub text: &'a str,
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub scale: PxScale,
    /// Font id to use for this section.
    ///
    /// It must be a valid id in the `FontMap` used for layout calls.
    /// The default `FontId(0)` should always be valid.
    pub font_id: FontId,
}

impl Default for SectionText<'static> {
    #[inline]
    fn default() -> Self {
        Self {
            text: "",
            scale: PxScale::from(16.0),
            font_id: FontId::default(),
        }
    }
}

pub trait ToSectionText {
    fn to_section_text(&self) -> SectionText<'_>;
}

impl ToSectionText for SectionText<'_> {
    #[inline]
    fn to_section_text(&self) -> SectionText<'_> {
        *self
    }
}

impl ToSectionText for &SectionText<'_> {
    #[inline]
    fn to_section_text(&self) -> SectionText<'_> {
        **self
    }
}

/// A positioned glyph with info relating to the `SectionText` from which it was derived.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SectionGlyph {
    /// The `SectionText` index.
    pub section_index: usize,
    /// The character byte index from the `SectionText` text.
    pub byte_index: usize,
    /// A positioned glyph.
    pub glyph: Glyph,
    /// Font id.
    pub font_id: FontId,
}
