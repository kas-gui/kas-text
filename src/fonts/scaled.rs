// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font face types

use crate::GlyphId;
use crate::conv::{DPU, LineMetrics};
use crate::fonts::Face;

/// Reference to a font face with scaling data
///
/// Several values are relative to the vertical baseline of the text. Due to
/// common axis conventions, it may be necessary to negate these; for example
/// `baseline - self.ascent()`.
#[derive(Copy, Clone)]
pub struct ScaledFace<'a> {
    face: &'a Face,
    dpu: DPU,
}

impl<'a> ScaledFace<'a> {
    /// Construct
    #[inline]
    pub(super) fn new(face: &'a Face, dpu: DPU) -> Self {
        ScaledFace { face, dpu }
    }

    /// Get the underlying (unscaled) font data
    #[inline]
    pub fn face(&self) -> &Face {
        self.face
    }

    /// Scale
    #[inline]
    pub fn dpu(&self) -> DPU {
        self.dpu
    }

    /// Horizontal advancement after this glyph, without shaping or kerning
    #[inline]
    pub fn h_advance(&self, id: GlyphId) -> f32 {
        let x = self.face.face().glyph_hor_advance(id.into()).unwrap();
        self.dpu.u16_to_px(x)
    }

    /// Horizontal side bearing
    ///
    /// If unspecified by the font this resolves to 0.
    #[inline]
    pub fn h_side_bearing(&self, id: GlyphId) -> f32 {
        let x = self
            .face
            .face()
            .glyph_hor_side_bearing(id.into())
            .unwrap_or(0);
        self.dpu.i16_to_px(x)
    }

    /// Ascender
    #[inline]
    pub fn ascent(&self) -> f32 {
        self.dpu.i16_to_px(self.face.face().ascender())
    }

    /// Descender
    #[inline]
    pub fn descent(&self) -> f32 {
        self.dpu.i16_to_px(self.face.face().descender())
    }

    /// Line gap
    #[inline]
    pub fn line_gap(&self) -> f32 {
        self.dpu.i16_to_px(self.face.face().line_gap())
    }

    /// Line height
    #[inline]
    pub fn height(&self) -> f32 {
        self.dpu.i16_to_px(self.face.face().height())
    }

    /// Metrics for underline
    #[inline]
    pub fn underline_metrics(&self) -> Option<LineMetrics> {
        self.face
            .face()
            .underline_metrics()
            .map(|m| self.dpu.to_line_metrics(m))
    }

    /// Metrics for strike-through
    #[inline]
    pub fn strikethrough_metrics(&self) -> Option<LineMetrics> {
        self.face
            .face()
            .strikeout_metrics()
            .map(|m| self.dpu.to_line_metrics(m))
    }
}
