// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Support for rastering glyphs
//!
//! This module is only available if `ab_glyph`, `fontdue` or both features are
//! enabled.

use crate::conv::DPU;
use crate::fonts::{fonts, FaceId};
use crate::{Glyph, GlyphId};
use easy_cast::*;

/// Raster configuration
pub struct Config {
    #[allow(unused)]
    sb_align: bool,
    #[allow(unused)]
    fontdue: bool,
    scale_steps: f32,
    subpixel_threshold: f32,
    subpixel_steps: f32,
}

impl Config {
    pub fn new(mode: u8, scale_steps: u8, subpixel_threshold: u8, subpixel_steps: u8) -> Self {
        Config {
            sb_align: mode == 1,
            fontdue: mode == 2,
            scale_steps: scale_steps.cast(),
            subpixel_threshold: subpixel_threshold.cast(),
            subpixel_steps: subpixel_steps.cast(),
        }
    }
}

/// A rastered sprite
pub struct Sprite {
    pub offset: (i32, i32),
    pub size: (u32, u32),
    pub data: Vec<u8>,
}

/// A Sprite descriptor
///
/// This descriptor includes all important properties of a rastered glyph in a
/// small, easily hashable value. It is thus ideal for caching rastered glyphs
/// in a `HashMap`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct SpriteDescriptor(u64);

impl SpriteDescriptor {
    /// Choose a sub-pixel precision multiplier based on the height
    ///
    /// Must return an integer between 1 and 16.
    fn sub_pixel_from_height(config: &Config, height: f32) -> f32 {
        if height < config.subpixel_threshold {
            config.subpixel_steps
        } else {
            1.0
        }
    }

    /// Construct
    pub fn new(config: &Config, face: FaceId, glyph: Glyph, height: f32) -> Self {
        let face: u16 = face.get().cast();
        let glyph_id: u16 = glyph.id.0;
        let mult = Self::sub_pixel_from_height(config, height);
        let mult2 = 0.5 * mult;
        let steps = u8::conv_nearest(mult);
        let height: u32 = (height * config.scale_steps).cast_nearest();
        let x_off = u8::conv_floor(glyph.position.0.fract() * mult + mult2) % steps;
        let y_off = u8::conv_floor(glyph.position.1.fract() * mult + mult2) % steps;
        assert!(height & 0xFF00_0000 == 0 && x_off & 0xF0 == 0 && y_off & 0xF0 == 0);
        let packed = face as u64
            | ((glyph_id as u64) << 16)
            | ((height as u64) << 32)
            | ((x_off as u64) << 56)
            | ((y_off as u64) << 60);
        SpriteDescriptor(packed)
    }

    /// Get `FaceId` descriptor
    pub fn face(self) -> FaceId {
        FaceId((self.0 & 0x0000_0000_0000_FFFF).cast())
    }

    /// Get `GlyphId` descriptor
    pub fn glyph(self) -> GlyphId {
        GlyphId(((self.0 & 0x0000_0000_FFFF_0000) >> 16).cast())
    }

    /// Get scale (height in pixels)
    pub fn height(self, config: &Config) -> f32 {
        let height = ((self.0 & 0x00FF_FFFF_0000_0000) >> 32) as u32;
        f32::conv(height) / config.scale_steps
    }

    /// Get fractional position
    ///
    /// This may optionally be used (depending on [`Config`]) to improve letter
    /// spacing at small font sizes. Returns the `(x, y)` offsets in the range
    /// `0.0 ≤ x < 1.0` (and the same for `y`).
    pub fn fractional_position(self, config: &Config) -> (f32, f32) {
        let mult = 1.0 / Self::sub_pixel_from_height(config, self.height(config));
        let x = ((self.0 & 0x0F00_0000_0000_0000) >> 56) as u8;
        let y = ((self.0 & 0xF000_0000_0000_0000) >> 60) as u8;
        let x = f32::conv(x) * mult;
        let y = f32::conv(y) * mult;
        (x, y)
    }
}

#[cfg(feature = "ab_glyph")]
fn raster_ab(config: &Config, dpu: DPU, desc: SpriteDescriptor) -> Option<Sprite> {
    use crate::fonts::FaceRef;
    use ab_glyph::Font;

    let id = desc.glyph();
    let face = desc.face();
    let face_store = fonts().get_face_store(face);

    let (mut x, y) = desc.fractional_position(config);
    let glyph_off = (x.round(), y.round());
    if config.sb_align && desc.height(config) >= config.subpixel_threshold {
        let sf = FaceRef(&face_store.face).scale_by_dpu(dpu);
        x -= sf.h_side_bearing(id);
    }

    let glyph = ab_glyph::Glyph {
        id: ab_glyph::GlyphId(id.0),
        scale: desc.height(config).into(),
        position: ab_glyph::point(x, y),
    };
    let outline = face_store.ab_glyph.outline_glyph(glyph)?;

    let bounds = outline.px_bounds();
    let offset = (bounds.min.x - glyph_off.0, bounds.min.y - glyph_off.1);
    let offset = (offset.0.cast_trunc(), offset.1.cast_trunc());
    let size = bounds.max - bounds.min;
    let size = (u32::conv_trunc(size.x), u32::conv_trunc(size.y));
    if size.0 == 0 || size.1 == 0 {
        log::warn!("Zero-sized glyph: {:?}", desc.glyph());
        return None; // nothing to draw
    }

    let mut data = Vec::new();
    data.resize(usize::conv(size.0 * size.1), 0u8);
    outline.draw(|x, y, c| {
        // Convert to u8 with saturating conversion, rounding down:
        data[usize::conv((y * size.0) + x)] = (c * 256.0) as u8;
    });

    Some(Sprite { offset, size, data })
}

#[cfg(feature = "fontdue")]
fn raster_fontdue(dpu: DPU, desc: SpriteDescriptor) -> Option<Sprite> {
    let face = desc.face();
    let face = &fonts().get_face_store(face).fontdue;

    // Ironically fontdue uses DPU internally, but doesn't let us input that.
    let px_per_em = dpu.0 * face.units_per_em();
    let (metrics, data) = face.rasterize_indexed(desc.glyph().0.cast(), px_per_em);

    let size = (u32::conv(metrics.width), u32::conv(metrics.height));
    let h_off = -metrics.ymin - i32::conv(metrics.height);
    let offset = (metrics.xmin, h_off);
    if size.0 == 0 || size.1 == 0 {
        log::warn!("Zero-sized glyph: {:?}", desc.glyph());
        return None; // nothing to draw
    }

    Some(Sprite { offset, size, data })
}

/// Raster a glyph
///
/// Attempts to raster a glyph. Can fail, in which case `None` is returned.
///
/// On success, returns the glyph offset (to be added to the sprite position
/// when rendering), the glyph size, and the rastered glyph (row-major order).
pub fn raster(dpu: DPU, config: &Config, desc: SpriteDescriptor) -> Option<Sprite> {
    cfg_if::cfg_if! {
        if #[cfg(all(feature = "fontdue", feature = "ab_glyph"))] {
            if config.fontdue {
                raster_fontdue(dpu, desc)
            } else {
                raster_ab(config, dpu, desc)
            }
        } else if #[cfg(feature = "ab_glyph")] {
            raster_ab(config, dpu, desc)
        } else {
            let _ = config;
            raster_fontdue(dpu, desc)
        }
    }
}