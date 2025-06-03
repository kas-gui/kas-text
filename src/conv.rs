// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Type conversion utilities
//!
//! Many indices are represented as `u32` instead of `usize` by this library in
//! order to save space (note that we do not expect `usize` smaller than `u32`
//! and our text representations are not intended to scale anywhere close to
//! `u32::MAX` bytes of text, so `u32` is always an appropriate index type).

use easy_cast::Cast;

/// Convert `usize` → `u32`
///
/// This is a "safer" wrapper around `as` ensuring (on debug builds) that the
/// input value may be represented correctly by `u32`.
#[inline]
pub fn to_u32(x: usize) -> u32 {
    x.cast()
}

/// Convert `u32` → `usize`
///
/// This is a "safer" wrapper around `as` ensuring that the operation is
/// zero-extension.
#[inline]
pub fn to_usize(x: u32) -> usize {
    x.cast()
}

/// Scale factor: pixels per font unit
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DPU(pub f32);

impl DPU {
    #[cfg(feature = "rustybuzz")]
    pub(crate) fn i32_to_px(self, x: i32) -> f32 {
        x as f32 * self.0
    }
    pub(crate) fn i16_to_px(self, x: i16) -> f32 {
        f32::from(x) * self.0
    }
    pub(crate) fn u16_to_px(self, x: u16) -> f32 {
        f32::from(x) * self.0
    }
    pub(crate) fn to_line_metrics(self, metrics: ttf_parser::LineMetrics) -> LineMetrics {
        LineMetrics {
            position: self.i16_to_px(metrics.position),
            thickness: self.i16_to_px(metrics.thickness),
        }
    }
}

/// Metrics for line marks
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub position: f32,
    pub thickness: f32,
}
