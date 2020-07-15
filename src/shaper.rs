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

use crate::{fonts, prepared, FontId, Vec2};
use ab_glyph::{Font, GlyphId, PxScale, ScaleFont};
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
    pub font_scale: PxScale,

    /// End position, excluding whitespace
    pub end_no_space: f32,
    /// Position of next glyph, if this run is followed by another
    pub caret: f32,

    /// End of slice represented
    pub end_index: u32,
    /// If true, append to the prior line (if any)
    pub append_to_prev: bool,
}

/// Shape a `run` of text
///
/// A "run" is expected to be the maximal sequence of code points of the same
/// embedding level (as defined by Unicode TR9 aka BIDI algorithm) *and*
/// excluding all hard line breaks (e.g. `\n`).
pub(crate) fn shape(
    font_id: FontId,
    font_scale: PxScale,
    text: &str,
    run: &prepared::Run,
) -> GlyphRun {
    let scale_font = fonts().get(font_id).into_scaled(font_scale);

    if !(scale_font.scale().x >= 0.0 || scale_font.scale().y >= 0.0) {
        return GlyphRun {
            glyphs: vec![],
            breaks: Default::default(),
            font_id,
            font_scale,
            end_no_space: 0.0,
            caret: 0.0,
            end_index: run.range.end,
            append_to_prev: run.append_to_prev(),
        };
    }

    let slice = &text[run.range];
    let idx_offset = run.range.start;

    let mut caret = 0.0;
    let mut end_no_space = caret;
    let mut prev_glyph_id = None;

    let mut breaks_iter = run.breaks.iter();
    let mut next_break = breaks_iter.next().cloned().unwrap_or(u32::MAX);
    assert!(next_break >= idx_offset);
    let mut breaks = SmallVec::<[GlyphBreak; 2]>::new();

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

    GlyphRun {
        glyphs,
        breaks,
        font_id,
        font_scale,
        end_no_space,
        caret,
        end_index: run.range.end,
        append_to_prev: run.append_to_prev(),
    }
}
