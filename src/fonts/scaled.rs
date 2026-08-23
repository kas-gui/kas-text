// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font face types

use crate::GlyphId;
use crate::conv::{DPU, LineMetrics};
use crate::fonts::Face;
use read_fonts::{TableProvider, types::GlyphId as ReadGlyphId};

/// Reference to a font face with scaling data
///
/// Several values are relative to the vertical baseline of the text. Due to
/// common axis conventions, it may be necessary to negate these; for example
/// `baseline - self.ascent()`.
#[derive(Copy, Clone)]
pub struct ScaledFace<'a>(pub(super) &'a Face, pub(super) DPU);
impl<'a> ScaledFace<'a> {
    /// Get the underlying (unscaled) font data
    #[inline]
    pub fn face(&self) -> &Face {
        self.0
    }

    /// Scale
    #[inline]
    pub fn dpu(&self) -> DPU {
        self.1
    }

    /// Horizontal advancement after this glyph, without shaping or kerning
    #[inline]
    pub fn h_advance(&self, id: GlyphId) -> f32 {
        let hmtx = self.0.font().hmtx().unwrap();
        let x = hmtx.advance(ReadGlyphId::new(u32::from(id.0))).unwrap();
        self.1.u16_to_px(x)
    }

    /// Horizontal side bearing
    ///
    /// If unspecified by the font this resolves to 0.
    #[inline]
    pub fn h_side_bearing(&self, id: GlyphId) -> f32 {
        let hmtx = self.0.font().hmtx().unwrap();
        let x = hmtx
            .side_bearing(ReadGlyphId::new(u32::from(id.0)))
            .unwrap_or(0);
        self.1.i16_to_px(x)
    }

    /// Ascender
    #[inline]
    pub fn ascent(&self) -> f32 {
        self.1.i16_to_px(self.0.ascender())
    }

    /// Descender
    #[inline]
    pub fn descent(&self) -> f32 {
        self.1.i16_to_px(self.0.descender())
    }

    /// Line gap
    #[inline]
    pub fn line_gap(&self) -> f32 {
        self.1.i16_to_px(self.0.line_gap())
    }

    /// Line height
    #[inline]
    pub fn height(&self) -> f32 {
        self.1.i16_to_px(self.0.ascender() - self.0.descender())
    }

    /// Metrics for underline
    #[inline]
    pub fn underline_metrics(&self) -> Option<LineMetrics> {
        self.0
            .face()
            .underline_metrics()
            .map(|m| self.1.to_line_metrics(m))
    }

    /// Metrics for strike-through
    #[inline]
    pub fn strikethrough_metrics(&self) -> Option<LineMetrics> {
        self.0
            .face()
            .strikeout_metrics()
            .map(|m| self.1.to_line_metrics(m))
    }
}
