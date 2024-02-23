// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Support for rastering glyphs
//!
//! This module is only available if `ab_glyph`, `fontdue` or both features are
//! enabled.
//!
//! # Example
//!
//! [`TextDisplay::glyphs`] and [`TextDisplay::glyphs_with_effects`] yield a sequence of glyphs
//! which may be drawn roughly as follows:
//!
//! ```
//! use std::collections::HashMap;
//! use kas_text::{Glyph, Vec2, TextDisplay};
//! use kas_text::fonts::{FaceId};
//! use kas_text::raster::{Config, raster, SpriteDescriptor, Sprite};
//!
//! #[derive(Default)]
//! struct DrawText {
//!     config: Config,
//!     cache: HashMap::<SpriteDescriptor, Option<Sprite>>,
//! }
//!
//! impl DrawText {
//!     fn text(&mut self, pos: Vec2, text: &TextDisplay) {
//!         // Ensure input position is not fractional:
//!         let pos = Vec2(pos.0.round(), pos.1.round());
//!
//!         let config = &self.config;
//!         let cache = &mut self.cache;
//!
//!         let for_glyph = |face: FaceId, dpem: f32, glyph: Glyph| {
//!             let desc = SpriteDescriptor::new(config, face, glyph, dpem);
//!             let opt_sprite = cache.entry(desc).or_insert_with(|| {
//!                 let opt_sprite = raster(config, desc);
//!                 if let Some(ref sprite) = opt_sprite {
//!                     // upload sprite to GPU here ...
//!                 }
//!                 opt_sprite
//!             });
//!             if let Some(sprite) = opt_sprite {
//!                 let offset = Vec2(sprite.offset.0 as f32, sprite.offset.1 as f32);
//!                 // Here we must discard sub-pixel position with floor:
//!                 let glyph_pos = Vec2(glyph.position.0.floor(), glyph.position.1.floor());
//!                 let a = pos + glyph_pos + offset;
//!                 let b = a + Vec2(sprite.size.0 as f32, sprite.size.1 as f32);
//!                 // draw rect from a to b
//!             }
//!         };
//!
//!         text.glyphs(for_glyph);
//!     }
//! }
//! ```

use crate::fonts::{fonts, FaceId};
use crate::{Glyph, GlyphId};
use easy_cast::*;

#[allow(unused)]
use crate::TextDisplay;

/// Raster configuration
#[derive(Debug, PartialEq)]
pub struct Config {
    #[allow(unused)]
    sb_align: bool,
    #[allow(unused)]
    fontdue: bool,
    scale_steps: f32,
    subpixel_threshold: f32,
    subpixel_steps: u8,
}

impl Config {
    /// Construct configuration
    ///
    /// For large glyphs the effects of configuration will be mostly unnoticeable
    /// but for small glyphs effects are more significant. The defaults will
    /// usually be a good choice. Results may depend on the font used.
    ///
    /// The `mode` parameter selects rendering mode (though depending on crate
    /// features, not all options will be available):
    ///
    /// -   `mode == 0` (default): use `ab_glyph` for rastering
    /// -   `mode == 1`: use `ab_glyph` and align glyphs to side bearings
    /// -   `mode == 2`: use `fontdue` for rastering
    ///
    /// Fonts sizes, in pixels per Em, are rounded to a multiple of `1 / scale_steps`.
    /// The default is `scale_steps == 4`.
    ///
    /// For font sizes (in pixels per Em) less than `subpixel_threshold`, sub-pixel positioning is
    /// enabled with `subpixel_steps` (supporting between 1 and 16 steps). Sub-pixel positioning
    /// allows better glyph spacing for small fonts potentially at the cost of minor blurring
    /// (though blurring may be present in any case), and makes words drawn with very small font
    /// sizes more faithfully represent scaled versions drawn with larger fonts.
    ///
    /// Subjectively, readability and apparent quality is better up to around `dpem=18` (~13.5pt),
    /// thus we recommend `subpixel_threshold == 18` and `subpixel_steps == 5`.
    pub fn new(mode: u8, scale_steps: u8, subpixel_threshold: u8, subpixel_steps: u8) -> Self {
        Config {
            sb_align: mode == 1,
            fontdue: mode == 2,
            scale_steps: scale_steps.cast(),
            subpixel_threshold: subpixel_threshold.cast(),
            subpixel_steps: subpixel_steps.clamp(1, 16),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::new(0, 4, 18, 5)
    }
}

/// A rastered sprite
#[derive(Debug, PartialEq, Eq)]
pub struct Sprite {
    /// Offset to be added to the glyph position
    pub offset: (i32, i32),
    /// Size of the sprite in pixels
    pub size: (u32, u32),
    /// Grayscale image, row major order, length `size.0 * size.1`
    pub data: Vec<u8>,
}

/// A Sprite descriptor
///
/// This descriptor includes all important properties of a rastered glyph in a
/// small, easily hashable value. It is thus ideal for caching rastered glyphs
/// in a `HashMap`.
#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct SpriteDescriptor(u64);

impl std::fmt::Debug for SpriteDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let dpem_steps = ((self.0 & 0x00FF_FFFF_0000_0000) >> 32) as u32;
        let x_steps = ((self.0 & 0x0F00_0000_0000_0000) >> 56) as u8;
        let y_steps = ((self.0 & 0xF000_0000_0000_0000) >> 60) as u8;
        f.debug_struct("SpriteDescriptor")
            .field("face", &self.face())
            .field("glyph", &self.glyph())
            .field("dpem_steps", &dpem_steps)
            .field("offset_steps", &(x_steps, y_steps))
            .finish()
    }
}

impl SpriteDescriptor {
    /// Choose a sub-pixel precision multiplier based on scale (pixels per Em)
    ///
    /// Must return an integer between 1 and 16.
    fn sub_pixel_from_dpem(config: &Config, dpem: f32) -> u8 {
        if dpem < config.subpixel_threshold {
            config.subpixel_steps
        } else {
            1
        }
    }

    /// Construct
    ///
    /// Most parameters come from [`TextDisplay::glyphs`] output. See also [`raster`].
    pub fn new(config: &Config, face: FaceId, glyph: Glyph, dpem: f32) -> Self {
        let face: u16 = face.get().cast();
        let glyph_id: u16 = glyph.id.0;
        let steps = Self::sub_pixel_from_dpem(config, dpem);
        let mult = f32::conv(steps);
        let dpem = u32::conv_trunc(dpem * config.scale_steps + 0.5);
        let x_off = u8::conv_trunc(glyph.position.0.fract() * mult) % steps;
        let y_off = u8::conv_trunc(glyph.position.1.fract() * mult) % steps;
        assert!(dpem & 0xFF00_0000 == 0 && x_off & 0xF0 == 0 && y_off & 0xF0 == 0);
        let packed = face as u64
            | ((glyph_id as u64) << 16)
            | ((dpem as u64) << 32)
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

    /// Get scale (pixels per Em)
    pub fn dpem(self, config: &Config) -> f32 {
        let dpem_steps = ((self.0 & 0x00FF_FFFF_0000_0000) >> 32) as u32;
        f32::conv(dpem_steps) / config.scale_steps
    }

    /// Get fractional position
    ///
    /// This may optionally be used (depending on [`Config`]) to improve letter
    /// spacing at small font sizes. Returns the `(x, y)` offsets in the range
    /// `0.0 ≤ x < 1.0` (and the same for `y`).
    pub fn fractional_position(self, config: &Config) -> (f32, f32) {
        let mult = 1.0 / f32::conv(Self::sub_pixel_from_dpem(config, self.dpem(config)));
        let x_steps = ((self.0 & 0x0F00_0000_0000_0000) >> 56) as u8;
        let y_steps = ((self.0 & 0xF000_0000_0000_0000) >> 60) as u8;
        let x = f32::conv(x_steps) * mult;
        let y = f32::conv(y_steps) * mult;
        (x, y)
    }
}

#[cfg(feature = "ab_glyph")]
fn raster_ab(config: &Config, desc: SpriteDescriptor) -> Option<Sprite> {
    use crate::fonts::FaceRef;
    use ab_glyph::Font;

    let id = desc.glyph();
    let face = desc.face();
    let face_store = fonts().get_face_store(face);
    let dpem = desc.dpem(config);

    let (mut x, y) = desc.fractional_position(config);
    if config.sb_align && desc.dpem(config) >= config.subpixel_threshold {
        let sf = FaceRef(&face_store.face).scale_by_dpem(dpem);
        x -= sf.h_side_bearing(id);
    }

    let font = &face_store.ab_glyph;
    let scale = dpem * font.height_unscaled() / font.units_per_em().unwrap();
    let glyph = ab_glyph::Glyph {
        id: ab_glyph::GlyphId(id.0),
        scale: scale.into(),
        position: ab_glyph::point(x, y),
    };
    let outline = font.outline_glyph(glyph)?;

    let bounds = outline.px_bounds();
    let offset = (bounds.min.x.cast_trunc(), bounds.min.y.cast_trunc());
    let size = bounds.max - bounds.min;
    let size = (u32::conv_trunc(size.x), u32::conv_trunc(size.y));
    if size.0 == 0 || size.1 == 0 {
        log::warn!("Zero-sized glyph: {:?}", desc.glyph());
        return None; // nothing to draw
    }

    let mut data = vec![0; usize::conv(size.0 * size.1)];
    outline.draw(|x, y, c| {
        // Convert to u8 with saturating conversion, rounding down:
        data[usize::conv((y * size.0) + x)] = (c * 256.0) as u8;
    });

    Some(Sprite { offset, size, data })
}

#[cfg(feature = "fontdue")]
fn raster_fontdue(config: &Config, desc: SpriteDescriptor) -> Option<Sprite> {
    let face = desc.face();
    let face = &fonts().get_face_store(face).fontdue;

    let (metrics, data) = face.rasterize_indexed(desc.glyph().0, desc.dpem(config));

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
/// Attempts to raster a glyph. Can fail (if the glyph in the given font face
/// cannot be rastered), in which case `None` is returned.
pub fn raster(config: &Config, desc: SpriteDescriptor) -> Option<Sprite> {
    cfg_if::cfg_if! {
        if #[cfg(all(feature = "fontdue", feature = "ab_glyph"))] {
            if config.fontdue {
                raster_fontdue(config, desc)
            } else {
                raster_ab(config, desc)
            }
        } else if #[cfg(feature = "ab_glyph")] {
            raster_ab(config, desc)
        } else {
            raster_fontdue(config, desc)
        }
    }
}
