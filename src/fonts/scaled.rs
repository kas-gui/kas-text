// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font face types

use crate::GlyphId;
use crate::conv::{DPU, LineMetrics};
use crate::fonts::Face;
use read_fonts::tables::hmtx::Hmtx;

/// Reference to a font face with scaling data
///
/// Several values are relative to the vertical baseline of the text. Due to
/// common axis conventions, it may be necessary to negate these; for example
/// `baseline - self.ascent()`.
#[derive(Clone)]
pub struct ScaledFace<'a> {
    face: &'a Face,
    hmtx: Hmtx<'a>,
    dpu: DPU,
}

impl<'a> ScaledFace<'a> {
    /// Construct
    #[inline]
    pub(super) fn new(face: &'a Face, hmtx: Hmtx<'a>, dpu: DPU) -> Self {
        ScaledFace { face, hmtx, dpu }
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
        // TODO: support font variations
        let x = self.hmtx.advance(id.into()).unwrap_or_default();
        self.dpu.u16_to_px(x)
    }

    /// Horizontal side bearing
    ///
    /// If unspecified by the font this resolves to 0.
    #[inline]
    pub fn h_side_bearing(&self, id: GlyphId) -> f32 {
        // TODO: support font variations
        let x = self.hmtx.side_bearing(id.into()).unwrap_or_default();
        self.dpu.i16_to_px(x)
    }

    /// Ascender
    #[inline]
    pub fn ascent(&self) -> f32 {
        // TODO: support font variations
        self.dpu.i16_to_px(self.face.ascender)
    }

    /// Descender
    #[inline]
    pub fn descent(&self) -> f32 {
        // TODO: support font variations
        self.dpu.i16_to_px(self.face.descender)
    }

    /// Line gap
    #[inline]
    pub fn line_gap(&self) -> f32 {
        // TODO: support font variations
        self.dpu.i16_to_px(self.face.line_gap)
    }

    /// Line height
    #[inline]
    pub fn height(&self) -> f32 {
        // TODO: support font variations
        self.dpu.i16_to_px(self.face.ascender - self.face.descender)
    }

    /// Metrics for underline
    #[inline]
    pub fn underline_metrics(&self) -> Option<LineMetrics> {
        self.face.post().map(|post| {
            // TODO: support font variations
            let top = self.dpu.i16_to_px(post.underline_position().to_i16());
            let thickness = self.dpu.i16_to_px(post.underline_thickness().to_i16());
            LineMetrics { top, thickness }
        })
    }

    /// Metrics for strike-through
    #[inline]
    pub fn strikethrough_metrics(&self) -> Option<LineMetrics> {
        self.face.os2().map(|os2| {
            // TODO: support font variations
            let top = self.dpu.i16_to_px(os2.y_strikeout_position());
            let thickness = self.dpu.i16_to_px(os2.y_strikeout_size());
            LineMetrics { top, thickness }
        })
    }
}
