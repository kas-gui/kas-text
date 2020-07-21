// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text shaping
//!
//! To quote the HarfBuzz manual:
//!
//! > Text shaping is the process of translating a string of character codes
//! > (such as Unicode codepoints) into a properly arranged sequence of glyphs
//! > that can be rendered onto a screen or into final output form for
//! > inclusion in a document.
//!
//! This module provides the [`shape`] function, which produces a sequence of
//! [`PositionedGlyph`]s based on the given text.
//!
//! This module *does not* perform line-breaking, wrapping or text reversal.

use crate::{fonts, prepared, FontId, Range, Vec2};
use ab_glyph::GlyphId;
use smallvec::SmallVec;

/// A positioned glyph
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    /// Index of char in source text
    pub index: u32,
    /// Glyph identifier in font
    pub id: GlyphId,
    /// Position of glyph
    pub position: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphBreak {
    /// Position in sequence of glyphs
    pub pos: u32,
    /// End position of previous "word" excluding space
    pub end_no_space: f32,
}

#[derive(Clone, Debug)]
pub struct GlyphRun {
    /// Sequence of all glyphs, with index in text
    pub glyphs: Vec<Glyph>,
    /// Sequence of all break points
    pub breaks: SmallVec<[GlyphBreak; 2]>,

    pub font_id: FontId,
    pub font_scale: f32,

    /// End position, excluding whitespace
    pub end_no_space: f32,
    /// Position of next glyph, if this run is followed by another
    pub caret: f32,

    /// Range of slice represented
    pub range: Range,
    /// If true, append to the prior line (if any)
    pub append_to_prev: bool,
}

/// Shape a `run` of text
///
/// A "run" is expected to be the maximal sequence of code points of the same
/// embedding level (as defined by Unicode TR9 aka BIDI algorithm) *and*
/// excluding all hard line breaks (e.g. `\n`).
///
/// Param `dpem` is the font size in pixels/em.
pub(crate) fn shape(font_id: FontId, dpem: f32, text: &str, run: &prepared::Run) -> GlyphRun {
    let mut glyphs = vec![];
    let mut breaks = Default::default();
    let mut end_no_space = 0.0;
    let mut caret = 0.0;

    let font = fonts().get(font_id);
    let font_scale = font.font_scale(dpem);

    if dpem >= 0.0 {
        #[cfg(feature = "harfbuzz_rs")]
        let r = shape_harfbuzz(font_id, dpem, text, run);

        #[cfg(not(feature = "harfbuzz_rs"))]
        let r = shape_simple(font, font_scale, text, run);

        glyphs = r.0;
        breaks = r.1;
        end_no_space = r.2;
        caret = r.3;
    }

    GlyphRun {
        glyphs,
        breaks,
        font_id,
        font_scale,
        end_no_space,
        caret,
        range: run.range,
        append_to_prev: run.append_to_prev(),
    }
}

// Use HarfBuzz lib
#[cfg(feature = "harfbuzz_rs")]
fn shape_harfbuzz(
    font_id: FontId,
    dpem: f32,
    text: &str,
    run: &prepared::Run,
) -> (Vec<Glyph>, SmallVec<[GlyphBreak; 2]>, f32, f32) {
    let mut font = fonts().get_harfbuzz(font_id);

    // ppem affects hinting but does not scale layout, so this has little effect:
    font.set_ppem(dpem as u32, dpem as u32);

    // Note: we could alternatively set scale to dpem*x and let unit_factor=1/x,
    // resulting in sub-pixel precision of x.
    let upem = font.face().upem() as i32;
    // This is the default: font.set_scale(upem, upem);
    let unit_factor = dpem / (upem as f32);

    let slice = &text[run.range];
    let idx_offset = run.range.start;

    // TODO: cache the buffer for reuse later?
    let buffer = harfbuzz_rs::UnicodeBuffer::new().add_str(slice);
    let features = [];

    let output = harfbuzz_rs::shape(&font, buffer, &features);

    let unit = |x: harfbuzz_rs::Position| x as f32 * unit_factor;

    let mut caret = 0.0;
    let mut end_no_space = caret;

    let mut breaks_iter = run.breaks.iter();
    let mut next_break = breaks_iter.next().cloned().unwrap_or(u32::MAX);
    assert!(next_break >= idx_offset);
    let mut breaks = SmallVec::<[GlyphBreak; 2]>::with_capacity(run.breaks.len());

    let mut glyphs = Vec::with_capacity(output.len());

    for (info, pos) in output
        .get_glyph_infos()
        .iter()
        .zip(output.get_glyph_positions().iter())
    {
        let index = idx_offset + info.cluster;
        assert!(info.codepoint <= u16::MAX as u32, "failed to map glyph id");
        let id = GlyphId(info.codepoint as u16);

        if index == next_break {
            let pos = glyphs.len() as u32;
            breaks.push(GlyphBreak { pos, end_no_space });
            next_break = breaks_iter.next().cloned().unwrap_or(u32::MAX);
        }

        let position = Vec2(caret + unit(pos.x_offset), unit(pos.y_offset));
        glyphs.push(Glyph {
            index,
            id,
            position,
        });

        // IIRC this is only applicable to vertical text, which we don't
        // currently support:
        debug_assert_eq!(pos.y_advance, 0);
        caret += unit(pos.x_advance);
        if text[(index as usize)..]
            .chars()
            .next()
            .map(|c| !c.is_whitespace())
            .unwrap()
        {
            end_no_space = caret;
        }
    }

    (glyphs, breaks, end_no_space, caret)
}

// Simple implementation (kerning but no shaping)
#[cfg(not(feature = "harfbuzz_rs"))]
fn shape_simple(
    font: crate::Font,
    font_scale: f32,
    text: &str,
    run: &prepared::Run,
) -> (Vec<Glyph>, SmallVec<[GlyphBreak; 2]>, f32, f32) {
    use ab_glyph::{Font, ScaleFont};
    let scale_font = font.scaled(font_scale);

    let slice = &text[run.range];
    let idx_offset = run.range.start;

    let mut caret = 0.0;
    let mut end_no_space = caret;
    let mut prev_glyph_id = None;

    let mut breaks_iter = run.breaks.iter();
    let mut next_break = breaks_iter.next().cloned().unwrap_or(u32::MAX);
    assert!(next_break >= idx_offset);
    let mut breaks = SmallVec::<[GlyphBreak; 2]>::with_capacity(run.breaks.len());

    // Allocate with an over-estimate and shrink later:
    let mut glyphs = Vec::with_capacity(slice.len());

    for (index, c) in slice.char_indices() {
        let index = idx_offset + index as u32;
        let id = scale_font.font().glyph_id(c);

        if index == next_break {
            let pos = glyphs.len() as u32;
            breaks.push(GlyphBreak { pos, end_no_space });
            next_break = breaks_iter.next().cloned().unwrap_or(u32::MAX);
        }

        if let Some(prev) = prev_glyph_id {
            caret += scale_font.kern(prev, id);
        }
        prev_glyph_id = Some(id);

        let position = Vec2(caret, 0.0);
        let glyph = Glyph {
            index,
            id,
            position,
        };
        glyphs.push(glyph);

        caret += scale_font.h_advance(id);
        if !c.is_whitespace() {
            end_no_space = caret;
        }
    }

    glyphs.shrink_to_fit();

    (glyphs, breaks, end_no_space, caret)
}
