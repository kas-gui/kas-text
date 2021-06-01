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

use std::mem::size_of;

/// Convert `usize` → `u32`
///
/// This is a "safer" wrapper around `as` ensuring (on debug builds) that the
/// input value may be represented correctly by `u32`.
#[inline]
pub fn to_u32(x: usize) -> u32 {
    debug_assert!(x <= to_usize(u32::MAX));
    x as u32
}

// Borrowed from static_assertions:
macro_rules! const_assert {
    ($x:expr $(,)?) => {
        #[allow(unknown_lints, eq_op)]
        const _: [(); 0 - !{
            const ASSERT: bool = $x;
            ASSERT
        } as usize] = [];
    };
}

/// Convert `u32` → `usize`
///
/// This is a "safer" wrapper around `as` ensuring that the operation is
/// zero-extension.
#[inline]
pub fn to_usize(x: u32) -> usize {
    const_assert!(size_of::<usize>() >= size_of::<u32>());
    x as usize
}

/// Scale factor: pixels per font unit
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DPU(pub(crate) f32);

impl DPU {
    #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LineMetrics {
    pub position: f32,
    pub thickness: f32,
}
