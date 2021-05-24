// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font face types

use crate::conv::{LineMetrics, DPU};
use crate::GlyphId;
use ttf_parser::Face;

/// Handle to a loaded font face
#[derive(Copy, Clone, Debug)]
pub struct FaceRef(pub(crate) &'static Face<'static>);

impl FaceRef {
    /// Convert `dpem` to `dpu`
    ///
    /// Output: a font-specific scale.
    ///
    /// Input: `dpem` is pixels/em
    ///
    /// ```none
    /// dpem
    ///   = pt_size × dpp
    ///   = pt_size × dpi / 72
    ///   = pt_size × scale_factor × (96 / 72)
    /// ```
    #[inline]
    pub(crate) fn dpu(self, dpem: f32) -> DPU {
        DPU(dpem / f32::from(self.0.units_per_em().unwrap()))
    }

    /// Get a scaled reference
    ///
    /// Units: `dpem` is dots (pixels) per Em (module documentation).
    #[inline]
    pub(crate) fn scale_by_dpem(self, dpem: f32) -> ScaledFaceRef {
        ScaledFaceRef(self.0, self.dpu(dpem))
    }

    /// Get a scaled reference
    ///
    /// Units: `dpu` is dots (pixels) per font-unit (see module documentation).
    #[inline]
    pub(crate) fn scale_by_dpu(self, dpu: DPU) -> ScaledFaceRef {
        ScaledFaceRef(self.0, dpu)
    }

    /// Get the height of horizontal text in pixels
    ///
    /// Units: `dpem` is dots (pixels) per Em (module documentation).
    #[inline]
    pub fn height(&self, dpem: f32) -> f32 {
        self.scale_by_dpem(dpem).height()
    }
}

/// Handle to a loaded font face
#[derive(Copy, Clone, Debug)]
pub(crate) struct ScaledFaceRef(&'static Face<'static>, DPU);
impl ScaledFaceRef {
    /// Unscaled face
    #[inline]
    #[allow(unused)] // built-in shaper only
    pub fn face(&self) -> &Face<'static> {
        self.0
    }

    #[inline]
    #[allow(unused)] // built-in shaper only
    pub fn dpu(&self) -> DPU {
        self.1
    }

    #[inline]
    #[allow(unused)] // built-in shaper only
    pub(crate) fn glyph_id(&self, c: char) -> GlyphId {
        // GlyphId 0 is required to be a special glyph representing a missing
        // character (see cmap table / TrueType specification).
        GlyphId(self.0.glyph_index(c).map(|id| id.0).unwrap_or(0))
    }

    #[inline]
    pub fn h_advance(&self, id: GlyphId) -> f32 {
        let x = self.0.glyph_hor_advance(id.into()).unwrap();
        self.1.u16_to_px(x)
    }

    #[inline]
    pub fn h_side_bearing(&self, id: GlyphId) -> f32 {
        let x = self.0.glyph_hor_side_bearing(id.into()).unwrap();
        self.1.i16_to_px(x)
    }

    #[inline]
    pub fn ascent(&self) -> f32 {
        self.1.i16_to_px(self.0.ascender())
    }

    #[inline]
    pub fn descent(&self) -> f32 {
        self.1.i16_to_px(self.0.descender())
    }

    #[inline]
    pub fn line_gap(&self) -> f32 {
        self.1.i16_to_px(self.0.line_gap())
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.1.i16_to_px(self.0.height())
    }

    #[inline]
    pub fn underline_metrics(&self) -> Option<LineMetrics> {
        self.0
            .underline_metrics()
            .map(|m| self.1.to_line_metrics(m))
    }

    #[inline]
    pub fn strikethrough_metrics(&self) -> Option<LineMetrics> {
        self.0
            .strikeout_metrics()
            .map(|m| self.1.to_line_metrics(m))
    }
}
