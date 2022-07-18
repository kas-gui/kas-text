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
//! [`Glyph`]s based on the given text.
//!
//! This module *does not* perform line-breaking, wrapping or text reversal.

use crate::conv::{to_u32, to_usize, DPU};
use crate::display::RunSpecial;
use crate::fonts::{fonts, FaceId};
use crate::{Range, Vec2};
use smallvec::SmallVec;
use unicode_bidi::Level;

/// A type-safe wrapper for glyph ID.
#[repr(transparent)]
#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Default, Debug)]
pub struct GlyphId(pub u16);

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
pub(crate) struct GlyphBreak {
    /// Index of char in source text
    pub index: u32,
    /// Position in sequence of glyphs
    pub pos: u32,
    /// End position of previous "word" excluding space
    pub no_space_end: f32,
}
impl GlyphBreak {
    /// Constructs with first field only
    ///
    /// Other fields are set later by shaper.
    pub(crate) fn new(index: u32) -> Self {
        GlyphBreak {
            index,
            pos: u32::MAX,
            no_space_end: f32::NAN,
        }
    }
}

/// A glyph run
///
/// A glyph run is a sequence of glyphs, starting from the origin: 0.0.
/// Whether the run is left-to-right text or right-to-left, glyphs are
/// positioned between 0.0 and `run.caret` (usually with some internal
/// margin due to side bearings — though this could even be negative).
/// The first glyph in the run should not be invisible (space) except where the
/// run occurs at the start of a line with explicit initial spacing, however
/// the run may end with white-space. `no_space_end` gives the "caret" position
/// of the *logical* end of the run, excluding white-space (for right-to-left
/// text, this is the end nearer the origin than `caret`).
#[derive(Clone, Debug)]
pub(crate) struct GlyphRun {
    /// Range in source text
    pub range: Range,
    /// Font size (pixels/em)
    pub dpem: f32,
    pub dpu: DPU,
    /// Font face identifier
    pub face_id: FaceId,
    /// Tab or no-break property
    pub special: RunSpecial,
    /// BIDI level
    pub level: Level,

    /// Sequence of all glyphs, in left-to-right order
    pub glyphs: Vec<Glyph>,
    /// All soft-breaks within this run, in left-to-right order
    ///
    /// Note: it would be equivalent to use a separate `Run` for each sub-range
    /// in the text instead of tracking breaks via this field.
    pub breaks: SmallVec<[GlyphBreak; 5]>,

    /// End position, excluding whitespace
    ///
    /// Use [`GlyphRun::start_no_space`] or [`GlyphRun::end_no_space`].
    pub no_space_end: f32,
    /// Position of next glyph, if this run is followed by another
    pub caret: f32,
}

impl GlyphRun {
    /// Number of parts
    ///
    /// Parts are in logical order
    pub fn num_parts(&self) -> usize {
        self.breaks.len() + 1
    }

    /// Calculate lengths for a part range
    ///
    /// Parts are identified in logical order with end index up to
    /// `self.num_parts()`.
    ///
    /// Returns `(offset, len_no_space, len)` where `offset` is the distance to
    /// from the origin to the start of the left-most part, `len` is the
    /// horizontal length of the given parts, and `len_no_space` is `len` but
    /// excluding whitespace at the logical end.
    pub fn part_lengths(&self, range: std::ops::Range<usize>) -> (f32, f32, f32) {
        // TODO: maybe we should adjust self.breaks to clean this up?
        assert!(range.start <= range.end);

        let (mut offset, mut len_no_space, mut len) = (0.0, 0.0, 0.0);
        if self.level.is_ltr() {
            if range.end > 0 {
                len_no_space = self.no_space_end;
                len = self.caret;
                if range.end <= self.breaks.len() {
                    let b = self.breaks[range.end - 1];
                    len_no_space = b.no_space_end;
                    if to_usize(b.pos) < self.glyphs.len() {
                        len = self.glyphs[to_usize(b.pos)].position.0
                    }
                }
            }

            if range.start > 0 {
                let glyph = to_usize(self.breaks[range.start - 1].pos);
                offset = self.glyphs[glyph].position.0;
                len_no_space -= offset;
                len -= offset;
            }
        } else {
            if range.start <= self.breaks.len() {
                len = self.caret;
                if range.start > 0 {
                    let b = self.breaks.len() - range.start;
                    let pos = to_usize(self.breaks[b].pos);
                    if pos < self.glyphs.len() {
                        len = self.glyphs[pos].position.0;
                    }
                }
                len_no_space = len;
            }
            if range.end <= self.breaks.len() {
                offset = self.caret;
                if range.end == 0 {
                    len_no_space = 0.0;
                } else {
                    let b = self.breaks.len() - range.end;
                    let b = self.breaks[b];
                    len_no_space -= b.no_space_end;
                    if to_usize(b.pos) < self.glyphs.len() {
                        offset = self.glyphs[to_usize(b.pos)].position.0;
                    }
                }
                len -= offset;
            }
        }
        (offset, len_no_space, len)
    }

    /// Get glyph index from part index
    pub fn to_glyph_range(&self, range: std::ops::Range<usize>) -> Range {
        let mut start = range.start;
        let mut end = range.end;

        let rtl = self.level.is_rtl();
        if rtl {
            let num_parts = self.num_parts();
            start = num_parts - start;
            end = num_parts - end;
        }

        let map = |part: usize| {
            if part == 0 {
                0
            } else if part <= self.breaks.len() {
                to_usize(self.breaks[part - 1].pos)
            } else {
                debug_assert_eq!(part, self.breaks.len() + 1);
                self.glyphs.len()
            }
        };

        let mut start = map(start);
        let mut end = map(end);

        if rtl {
            std::mem::swap(&mut start, &mut end);
        }

        Range::from(start..end)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Input<'a> {
    /// Contiguous text
    pub text: &'a str,
    pub dpem: f32,
    pub face_id: FaceId,
    pub level: Level,
}

/// Shape a `run` of text
///
/// A "run" is expected to be the maximal sequence of code points of the same
/// embedding level (as defined by Unicode TR9 aka BIDI algorithm) *and*
/// excluding all hard line breaks (e.g. `\n`).
pub(crate) fn shape(
    input: Input,
    range: Range, // range in text
    // All soft-break locations within this run, excluding the end
    mut breaks: SmallVec<[GlyphBreak; 5]>,
    special: RunSpecial,
) -> GlyphRun {
    let Input {
        text,
        dpem,
        face_id,
        level,
    } = input;

    /*
    print!("shape[{:?}]:\t", special);
    let mut start = range.start();
    for b in &breaks {
        print!("\"{}\" ", &text[start..(b.index as usize)]);
        start = b.index as usize;
    }
    println!("\"{}\"", &text[start..range.end()]);
    */

    if level.is_rtl() {
        breaks.reverse();
    }

    let mut glyphs = vec![];
    let mut no_space_end = 0.0;
    let mut caret = 0.0;

    let face = fonts().get_face(face_id);
    let dpu = face.dpu(dpem);
    let sf = face.scale_by_dpu(dpu);

    if dpem >= 0.0 {
        #[cfg(feature = "harfbuzz_rs")]
        let r = shape_harfbuzz(text, range, dpem, face_id, level, &mut breaks);

        #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
        let r = shape_rustybuzz(text, range, dpem, face_id, level, &mut breaks);

        #[cfg(all(not(feature = "harfbuzz_rs"), not(feature = "rustybuzz")))]
        let r = shape_simple(sf, text, range, level, &mut breaks);

        glyphs = r.0;
        no_space_end = r.1;
        caret = r.2;
    }

    if level.is_rtl() {
        // With RTL text, no_space_end means start_no_space; recalculate
        let mut break_i = breaks.len().wrapping_sub(1);
        let mut start_no_space = caret;
        let mut last_id = None;
        let side_bearing = |id: Option<GlyphId>| id.map(|id| sf.h_side_bearing(id)).unwrap_or(0.0);
        for (pos, glyph) in glyphs.iter().enumerate().rev() {
            if break_i < breaks.len() && to_usize(breaks[break_i].pos) == pos {
                assert!(pos < glyphs.len());
                breaks[break_i].pos = to_u32(pos) + 1;
                breaks[break_i].no_space_end = start_no_space - side_bearing(last_id);
                break_i = break_i.wrapping_sub(1);
            }
            if !text[to_usize(glyph.index)..]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(true)
            {
                last_id = Some(glyph.id);
                start_no_space = glyph.position.0;
            }
        }
        no_space_end = start_no_space - side_bearing(last_id);
    }

    GlyphRun {
        range,
        dpem,
        dpu,
        face_id,
        special,
        level,

        glyphs,
        breaks,
        no_space_end,
        caret,
    }
}

// Use HarfBuzz lib
#[cfg(feature = "harfbuzz_rs")]
fn shape_harfbuzz(
    text: &str,
    range: Range,
    dpem: f32,
    face_id: FaceId,
    level: Level,
    breaks: &mut [GlyphBreak],
) -> (Vec<Glyph>, f32, f32) {
    let dpem = dpem;
    let mut font = fonts().get_harfbuzz(face_id);

    // ppem affects hinting but does not scale layout, so this has little effect:
    font.set_ppem(dpem as u32, dpem as u32);

    // Note: we could alternatively set scale to dpem*x and let unit_factor=1/x,
    // resulting in sub-pixel precision of x.
    let upem = font.face().upem();
    // This is the default: font.set_scale(upem, upem);
    let unit_factor = dpem / (upem as f32);

    let slice = &text[range];
    let idx_offset = range.start;
    let rtl = level.is_rtl();

    // TODO: cache the buffer for reuse later?
    let buffer = harfbuzz_rs::UnicodeBuffer::new()
        .set_direction(match rtl {
            false => harfbuzz_rs::Direction::Ltr,
            true => harfbuzz_rs::Direction::Rtl,
        })
        .add_str(slice);
    let features = [];

    let output = harfbuzz_rs::shape(&font, buffer, &features);

    let unit = |x: harfbuzz_rs::Position| x as f32 * unit_factor;

    let mut caret = 0.0;
    let mut no_space_end = caret;
    let mut break_i = 0;

    let mut glyphs = Vec::with_capacity(output.len());

    for (info, pos) in output
        .get_glyph_infos()
        .iter()
        .zip(output.get_glyph_positions().iter())
    {
        let index = idx_offset + info.cluster;
        assert!(info.codepoint <= u16::MAX as u32, "failed to map glyph id");
        let id = GlyphId(info.codepoint as u16);

        if breaks
            .get(break_i)
            .map(|b| b.index == index)
            .unwrap_or(false)
        {
            breaks[break_i].pos = to_u32(glyphs.len());
            breaks[break_i].no_space_end = no_space_end;
            break_i += 1;
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
        if text[to_usize(index)..]
            .chars()
            .next()
            .map(|c| !c.is_whitespace())
            .unwrap()
        {
            no_space_end = caret;
        }
    }

    (glyphs, no_space_end, caret)
}

// Use Rustybuzz lib
#[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
fn shape_rustybuzz(
    text: &str,
    range: Range,
    dpem: f32,
    face_id: FaceId,
    level: Level,
    breaks: &mut [GlyphBreak],
) -> (Vec<Glyph>, f32, f32) {
    let dpem = dpem;
    let fonts = fonts();
    let dpu = fonts.get_face(face_id).dpu(dpem);
    let face = fonts.get_rustybuzz(face_id);

    // ppem affects hinting but does not scale layout, so this has little effect:
    // face.set_pixels_per_em(Some((dpem as u16, dpem as u16)));

    let slice = &text[range];
    let idx_offset = range.start;
    let rtl = level.is_rtl();

    // TODO: cache the buffer for reuse later?
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.set_direction(match rtl {
        false => rustybuzz::Direction::LeftToRight,
        true => rustybuzz::Direction::RightToLeft,
    });
    buffer.push_str(slice);
    let features = [];

    let output = rustybuzz::shape(face, &features, buffer);

    let mut caret = 0.0;
    let mut no_space_end = caret;
    let mut break_i = 0;

    let mut glyphs = Vec::with_capacity(output.len());

    for (info, pos) in output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions().iter())
    {
        let index = idx_offset + info.cluster;
        assert!(info.glyph_id <= u16::MAX as u32, "failed to map glyph id");
        let id = GlyphId(info.glyph_id as u16);

        if breaks
            .get(break_i)
            .map(|b| b.index == index)
            .unwrap_or(false)
        {
            breaks[break_i].pos = to_u32(glyphs.len());
            breaks[break_i].no_space_end = no_space_end;
            break_i += 1;
        }

        let position = Vec2(
            caret + dpu.i32_to_px(pos.x_offset),
            dpu.i32_to_px(pos.y_offset),
        );
        glyphs.push(Glyph {
            index,
            id,
            position,
        });

        // IIRC this is only applicable to vertical text, which we don't
        // currently support:
        debug_assert_eq!(pos.y_advance, 0);
        caret += dpu.i32_to_px(pos.x_advance);
        if text[to_usize(index)..]
            .chars()
            .next()
            .map(|c| !c.is_whitespace())
            .unwrap()
        {
            no_space_end = caret;
        }
    }

    (glyphs, no_space_end, caret)
}

// Simple implementation (kerning but no shaping)
#[cfg(all(not(feature = "harfbuzz_rs"), not(feature = "rustybuzz")))]
fn shape_simple(
    sf: crate::fonts::ScaledFaceRef,
    text: &str,
    range: Range,
    level: Level,
    breaks: &mut [GlyphBreak],
) -> (Vec<Glyph>, f32, f32) {
    use unicode_bidi_mirroring::get_mirrored;

    let slice = &text[range];
    let idx_offset = range.start;
    let rtl = level.is_rtl();

    let mut caret = 0.0;
    let mut no_space_end = caret;
    let mut prev_glyph_id: Option<GlyphId> = None;
    let mut break_i = 0;

    // Allocate with an over-estimate and shrink later:
    let mut glyphs = Vec::with_capacity(slice.len());
    let mut iter = slice.char_indices();
    let mut next_char_index = || match rtl {
        false => iter.next(),
        true => iter.next_back(),
    };
    while let Some((index, mut c)) = next_char_index() {
        let index = idx_offset + to_u32(index);
        if rtl {
            if let Some(m) = get_mirrored(c) {
                c = m;
            }
        }
        let id = sf.face().glyph_index(c);

        if breaks
            .get(break_i)
            .map(|b| b.index == index)
            .unwrap_or(false)
        {
            breaks[break_i].pos = to_u32(glyphs.len());
            breaks[break_i].no_space_end = no_space_end;
            break_i += 1;
            no_space_end = caret;
        }

        if let Some(prev) = prev_glyph_id {
            if let Some(kern) = sf.face().0.tables().kern {
                if let Some(adv) = kern
                    .subtables
                    .into_iter()
                    .filter(|st| st.horizontal && !st.variable)
                    .find_map(|st| st.glyphs_kerning(prev.into(), id.into()))
                {
                    caret += sf.dpu().i16_to_px(adv);
                }
            }
        }
        prev_glyph_id = Some(id);

        let position = Vec2(caret, 0.0);
        let glyph = Glyph {
            index,
            id,
            position,
        };
        glyphs.push(glyph);

        caret += sf.h_advance(id);
        if !c.is_whitespace() {
            no_space_end = caret;
        }
    }

    glyphs.shrink_to_fit();

    (glyphs, no_space_end, caret)
}
