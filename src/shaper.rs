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

use crate::fonts::{fonts, Font, FontId};
use crate::{prepared, Range, Vec2};
use ab_glyph::{GlyphId, ScaleFont};
use smallvec::SmallVec;
use unicode_bidi::Level;

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
    pub no_space_end: f32,
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
pub struct GlyphRun {
    /// Sequence of all glyphs, in left-to-right order
    pub glyphs: Vec<Glyph>,
    /// Sequence of all break points, in left-to-right order
    pub breaks: SmallVec<[GlyphBreak; 2]>,

    pub font_id: FontId,
    pub font_scale: f32,

    /// End position, excluding whitespace
    ///
    /// Use [`GlyphRun::start_no_space`] or [`GlyphRun::end_no_space`].
    pub no_space_end: f32,
    /// Position of next glyph, if this run is followed by another
    pub caret: f32,

    // TODO: maybe we shouldn't duplicate this information?
    /// Range of text represented
    pub range: Range,
    /// BIDI level (odd levels are right-to-left)
    pub level: Level,
    /// If true, the logical-start of this Run is not a valid break point
    pub no_break: bool,
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
                    if (b.pos as usize) < self.glyphs.len() {
                        len = self.glyphs[b.pos as usize].position.0
                    }
                }
            }

            if range.start > 0 {
                let glyph = self.breaks[range.start - 1].pos as usize;
                offset = self.glyphs[glyph].position.0;
                len_no_space -= offset;
                len -= offset;
            }
        } else {
            if range.start <= self.breaks.len() {
                len = self.caret;
                if range.start > 0 {
                    let b = self.breaks.len() - range.start;
                    let pos = self.breaks[b].pos as usize;
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
                    if (b.pos as usize) < self.glyphs.len() {
                        offset = self.glyphs[b.pos as usize].position.0;
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
                self.breaks[part - 1].pos as usize
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

/// Shape a `run` of text
///
/// A "run" is expected to be the maximal sequence of code points of the same
/// embedding level (as defined by Unicode TR9 aka BIDI algorithm) *and*
/// excluding all hard line breaks (e.g. `\n`).
pub(crate) fn shape(text: &str, run: &prepared::Run) -> GlyphRun {
    let mut glyphs = vec![];
    let mut breaks = Default::default();
    let mut no_space_end = 0.0;
    let mut caret = 0.0;

    let font = fonts().get(run.font_id);
    let font_scale = font.font_scale(run.dpem);

    if run.dpem >= 0.0 {
        #[cfg(feature = "harfbuzz_rs")]
        let r = shape_harfbuzz(text, run);

        #[cfg(not(feature = "harfbuzz_rs"))]
        let r = shape_simple(font, font_scale, text, run);

        glyphs = r.0;
        breaks = r.1;
        no_space_end = r.2;
        caret = r.3;
    }

    if run.level.is_rtl() {
        // With RTL text, no_space_end means start_no_space; recalculate
        let mut break_i = breaks.len().wrapping_sub(1);
        let mut start_no_space = caret;
        let mut last_id = None;
        let scale_font = font.scaled(font_scale);
        let side_bearing =
            |id: Option<GlyphId>| id.map(|id| scale_font.h_side_bearing(id)).unwrap_or(0.0);
        for (pos, glyph) in glyphs.iter().enumerate().rev() {
            if break_i < breaks.len() && breaks[break_i].pos as usize == pos {
                assert!(pos < glyphs.len());
                breaks[break_i].pos = pos as u32 + 1;
                breaks[break_i].no_space_end = start_no_space - side_bearing(last_id);
                break_i = break_i.wrapping_sub(1);
            }
            if !text[(glyph.index as usize)..]
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
        glyphs,
        breaks,
        font_id: run.font_id,
        font_scale,
        no_space_end,
        caret,
        range: run.range,
        level: run.level,
        no_break: run.no_break,
    }
}

// Use HarfBuzz lib
#[cfg(feature = "harfbuzz_rs")]
fn shape_harfbuzz(
    text: &str,
    run: &prepared::Run,
) -> (Vec<Glyph>, SmallVec<[GlyphBreak; 2]>, f32, f32) {
    let dpem = run.dpem;
    let mut font = fonts().get_harfbuzz(run.font_id);

    // ppem affects hinting but does not scale layout, so this has little effect:
    font.set_ppem(dpem as u32, dpem as u32);

    // Note: we could alternatively set scale to dpem*x and let unit_factor=1/x,
    // resulting in sub-pixel precision of x.
    let upem = font.face().upem() as i32;
    // This is the default: font.set_scale(upem, upem);
    let unit_factor = dpem / (upem as f32);

    let slice = &text[run.range];
    let idx_offset = run.range.start;
    let rtl = run.level.is_rtl();

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

    let mut breaks_iter = run.breaks.iter();
    let mut get_next_break = || match rtl {
        false => breaks_iter.next().cloned().unwrap_or(u32::MAX),
        true => breaks_iter.next_back().cloned().unwrap_or(u32::MAX),
    };
    let mut next_break = get_next_break();
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
            breaks.push(GlyphBreak { pos, no_space_end });
            next_break = get_next_break();
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
            no_space_end = caret;
        }
    }

    (glyphs, breaks, no_space_end, caret)
}

// Simple implementation (kerning but no shaping)
#[cfg(not(feature = "harfbuzz_rs"))]
fn shape_simple(
    font: Font,
    font_scale: f32,
    text: &str,
    run: &prepared::Run,
) -> (Vec<Glyph>, SmallVec<[GlyphBreak; 2]>, f32, f32) {
    use ab_glyph::Font;
    use unicode_bidi_mirroring::get_mirrored;

    let scale_font = font.scaled(font_scale);

    let slice = &text[run.range];
    let idx_offset = run.range.start;
    let rtl = run.level.is_rtl();

    let mut caret = 0.0;
    let mut no_space_end = caret;
    let mut prev_glyph_id = None;

    let mut breaks_iter = run.breaks.iter();
    let mut get_next_break = || match rtl {
        false => breaks_iter.next().cloned().unwrap_or(u32::MAX),
        true => breaks_iter.next_back().cloned().unwrap_or(u32::MAX),
    };
    let mut next_break = get_next_break();
    assert!(next_break >= idx_offset);
    let mut breaks = SmallVec::<[GlyphBreak; 2]>::with_capacity(run.breaks.len());

    // Allocate with an over-estimate and shrink later:
    let mut glyphs = Vec::with_capacity(slice.len());
    let mut iter = slice.char_indices();
    let mut next_char_index = || match rtl {
        false => iter.next(),
        true => iter.next_back(),
    };
    while let Some((index, mut c)) = next_char_index() {
        let index = idx_offset + index as u32;
        if rtl {
            if let Some(m) = get_mirrored(c) {
                c = m;
            }
        }
        let id = scale_font.font().glyph_id(c);

        if index == next_break {
            let pos = glyphs.len() as u32;
            breaks.push(GlyphBreak { pos, no_space_end });
            next_break = get_next_break();
            no_space_end = caret;
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
            no_space_end = caret;
        }
    }

    glyphs.shrink_to_fit();

    (glyphs, breaks, no_space_end, caret)
}
