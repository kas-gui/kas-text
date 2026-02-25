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

use crate::conv::{DPU, to_u32, to_usize};
use crate::display::RunSpecial;
use crate::fonts::{self, FaceId};
use crate::{Range, Vec2};
use icu_properties::props::Script;
use tinyvec::TinyVec;
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GlyphBreak {
    /// Index of char in source text
    pub index: u32,
    /// Position in sequence of glyphs
    pub gi: u32,
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
            gi: u32::MAX,
            no_space_end: f32::NAN,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PartMetrics {
    /// The distance from the origin to the start of the left-most part
    pub offset: f32,
    /// Length (excluding whitespace)
    pub len_no_space: f32,
    /// Length (including trailing whitespace)
    pub len: f32,
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
    /// Script
    pub script: Script,

    /// Sequence of all glyphs, in left-to-right order
    pub glyphs: Vec<Glyph>,
    /// All soft-breaks within this run, in logical order
    ///
    /// Note: it would be equivalent to use a separate `Run` for each sub-range
    /// in the text instead of tracking breaks via this field.
    pub breaks: TinyVec<[GlyphBreak; 4]>,

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
    pub fn part_lengths(&self, range: std::ops::Range<usize>) -> PartMetrics {
        assert!(range.start <= range.end);

        let mut part = PartMetrics::default();
        if range.is_empty() {
            return part;
        }

        if self.level.is_ltr() {
            part.len_no_space = self.no_space_end;
            part.len = self.caret;
            if range.end <= self.breaks.len() {
                let b = self.breaks[range.end - 1];
                part.len_no_space = b.no_space_end;
                if to_usize(b.gi) < self.glyphs.len() {
                    part.len = self.glyphs[to_usize(b.gi)].position.0
                } else {
                    debug_assert!(false);
                }
            }

            if range.start > 0 {
                let glyph = to_usize(self.breaks[range.start - 1].gi);
                part.offset = self.glyphs[glyph].position.0;
                part.len_no_space -= part.offset;
                part.len -= part.offset;
            }
        } else {
            part.len = self.caret;
            if range.start > 0 {
                let gi = to_usize(self.breaks[range.start - 1].gi);
                if gi > 0 {
                    part.len = self.glyphs[gi - 1].position.0;
                } else {
                    debug_assert!(false);
                }
            }
            part.len_no_space = part.len;

            if range.end <= self.breaks.len() {
                part.offset = self.caret;
                debug_assert!(range.end > 0);
                let b = self.breaks[range.end - 1];
                part.len_no_space -= b.no_space_end;
                if b.gi > 0 {
                    part.offset = self.glyphs[to_usize(b.gi) - 1].position.0;
                } else {
                    debug_assert!(false);
                }
                part.len -= part.offset;
            }
        }

        debug_assert!(
            part.len_no_space <= part.len,
            "{part:?} for {range:?} in {self:?}"
        );
        part
    }

    /// Get glyph index from part index
    pub fn to_glyph_range(&self, range: std::ops::Range<usize>) -> Range {
        if self.level.is_ltr() {
            let map = |part: usize| {
                if part == 0 {
                    0
                } else if part <= self.breaks.len() {
                    to_usize(self.breaks[part - 1].gi)
                } else {
                    debug_assert_eq!(part, self.breaks.len() + 1);
                    self.glyphs.len()
                }
            };

            let start = map(range.start);
            let end = map(range.end);
            Range::from(start..end)
        } else {
            let map = |part: usize| {
                if part == 0 {
                    0
                } else if part <= self.breaks.len() {
                    to_usize(self.breaks[part - 1].gi)
                } else {
                    debug_assert_eq!(part, self.breaks.len() + 1);
                    self.glyphs.len()
                }
            };

            let start = map(range.start);
            let end = map(range.end);
            Range::from(start..end)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Input<'a> {
    /// Contiguous text
    pub text: &'a str,
    pub dpem: f32,
    pub level: Level,
    pub script: Script,
}

/// Shape a `run` of text
///
/// A "run" is expected to be the maximal sequence of code points of the same
/// embedding level (as defined by Unicode TR9 aka BIDI algorithm) *and*
/// excluding all hard line breaks (e.g. `\n`).
pub(crate) fn shape(
    input: Input,
    range: Range, // range in text
    face_id: FaceId,
    // All soft-break locations within this run, excluding the end
    mut breaks: TinyVec<[GlyphBreak; 4]>,
    special: RunSpecial,
) -> GlyphRun {
    /*
    eprint!("shape[{} breaks, {:?}]:\t", breaks.len(), special);
    let mut start = range.start();
    for b in &breaks {
        eprint!("\"{}\" ", &input.text[start..(b.index as usize)]);
        start = b.index as usize;
    }
    eprintln!("\"{}\"", &input.text[start..range.end()]);
    */

    debug_assert!(breaks.iter().all(|b| b.index > 0));

    if input.level.is_rtl() {
        // Breaks must be reversed for shaping; they are reversed again after
        breaks.reverse();
    }

    let mut glyphs = vec![];
    let mut no_space_end = 0.0;
    let mut caret = 0.0;

    let face = fonts::library().get_face(face_id);
    let dpu = face.dpu(input.dpem);
    let sf = face.scale_by_dpu(dpu);

    if input.dpem >= 0.0 {
        #[cfg(feature = "rustybuzz")]
        let r = shape_rustybuzz(input, range, face_id, &mut breaks);

        #[cfg(not(feature = "rustybuzz"))]
        let r = shape_simple(sf, input, range, &mut breaks);

        glyphs = r.0;
        no_space_end = r.1;
        caret = r.2;
    }

    if input.level.is_rtl() {
        // With RTL text, no_space_end means start_no_space; recalculate
        breaks.reverse();
        let mut break_i = 0;
        let mut start_no_space = caret;
        let mut last_id = None;
        let side_bearing = |id: Option<GlyphId>| id.map(|id| sf.h_side_bearing(id)).unwrap_or(0.0);
        glyphs.reverse();
        for (gi, glyph) in glyphs.iter().enumerate() {
            if let Some(b) = breaks.get_mut(break_i)
                && to_usize(b.gi) == glyphs.len() - gi - 1
            {
                b.gi = to_u32(gi);
                b.no_space_end = start_no_space - side_bearing(last_id);
                break_i += 1;
            }
            if !input.text[to_usize(glyph.index)..]
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

    debug_assert!(glyphs.iter().is_sorted_by_key(|g| g.index));
    debug_assert!(breaks.iter().all(|b| b.gi > 0));

    GlyphRun {
        range,
        dpem: input.dpem,
        dpu,
        face_id,
        special,
        level: input.level,
        script: input.script,

        glyphs,
        breaks,
        no_space_end,
        caret,
    }
}

// Use Rustybuzz lib
#[cfg(feature = "rustybuzz")]
fn shape_rustybuzz(
    input: Input<'_>,
    range: Range,
    face_id: FaceId,
    breaks: &mut [GlyphBreak],
) -> (Vec<Glyph>, f32, f32) {
    let Input {
        text,
        dpem,
        level,
        script,
    } = input;

    let fonts = fonts::library();
    let store = fonts.get_face_store(face_id);
    let dpu = store.face_ref().dpu(dpem);
    let face = store.rustybuzz();

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
    let script: fontique::Script = script.into();
    let tag = ttf_parser::Tag(u32::from_be_bytes(script.0));
    if let Some(script) = rustybuzz::Script::from_iso15924_tag(tag) {
        buffer.set_script(script);
    }
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
            breaks[break_i].gi = to_u32(glyphs.len());
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
#[cfg(not(feature = "rustybuzz"))]
fn shape_simple(
    sf: crate::fonts::ScaledFaceRef,
    input: Input<'_>,
    range: Range,
    breaks: &mut [GlyphBreak],
) -> (Vec<Glyph>, f32, f32) {
    let Input { text, level, .. } = input;

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
        if rtl && let Some(m) = get_mirrored(c) {
            c = m;
        }
        let id = sf.face().glyph_index(c);

        if breaks
            .get(break_i)
            .map(|b| b.index == index)
            .unwrap_or(false)
        {
            breaks[break_i].gi = to_u32(glyphs.len());
            breaks[break_i].no_space_end = no_space_end;
            break_i += 1;
            no_space_end = caret;
        }

        if let Some(prev) = prev_glyph_id
            && let Some(kern) = sf.face().0.tables().kern
            && let Some(adv) = kern
                .subtables
                .into_iter()
                .filter(|st| st.horizontal && !st.variable)
                .find_map(|st| st.glyphs_kerning(prev.into(), id.into()))
        {
            caret += sf.dpu().i16_to_px(adv);
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

/// Warning: test results may depend on system fonts
///
/// Tests are extensions of those in `display/text_runs.rs`.
#[cfg(test)]
mod test {
    use crate::{Direction, TextDisplay, format::FontToken};
    use std::iter;
    use std::ops::Range;

    type Expected<'a> = &'a [(Range<usize>, &'a [u32], &'a [u32])];

    fn test_shaping(text: &str, dir: Direction, expected: Expected) {
        let fonts = iter::once(FontToken {
            start: 0,
            dpem: 16.0,
            font: Default::default(),
        });

        let mut display = TextDisplay::default();
        assert!(display.prepare_runs(text, dir, fonts).is_ok());

        for (i, (run, expected)) in display.raw_runs().iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                run.range.to_std(),
                expected.0,
                "text range for text \"{text}\", run {i}"
            );
            assert_eq!(
                run.glyphs.iter().map(|g| g.index).collect::<Vec<_>>(),
                expected.1,
                "glyph indices for text \"{text}\", run {i}"
            );
            assert_eq!(
                run.breaks.iter().map(|b| b.gi).collect::<Vec<_>>(),
                expected.2,
                "glyph break indices for text \"{text}\", run {i}"
            );
        }
        assert_eq!(display.raw_runs().len(), expected.len(), "number of runs");
    }

    #[test]
    fn test_shaping_simple() {
        let sample = "Layout demo 123";
        test_shaping(
            sample,
            Direction::Auto,
            &[(
                0..sample.len(),
                &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
                &[7, 12],
            )],
        );
    }

    #[test]
    fn test_shaping_bidi() {
        let sample = "one אחת שתיים two";
        test_shaping(
            sample,
            Direction::Auto,
            &[
                (0..4, &[0, 1, 2, 3], &[]),
                (4..21, &[4, 6, 8, 10, 11, 13, 15, 17, 19], &[4]),
                (21..25, &[21, 22, 23, 24], &[1]),
            ],
        );

        let sample = "אחת one two שתיים";
        test_shaping(
            sample,
            Direction::Auto,
            &[
                (0..7, &[0, 2, 4, 6], &[]),
                (7..14, &[7, 8, 9, 10, 11, 12, 13], &[4]),
                (14..25, &[14, 15, 17, 19, 21, 23], &[1]),
            ],
        );
    }

    #[test]
    fn test_shaping_weak_bidi() {
        let sample = "123 (1-2)";

        let expected_ltr: Expected = &[(0..9, &[0, 1, 2, 3, 4, 5, 6, 7, 8], &[4])];
        test_shaping(sample, Direction::Auto, &expected_ltr[..]);
        test_shaping(sample, Direction::Ltr, &expected_ltr[..]);

        let expected_rtl: Expected = &[
            (0..3, &[0, 1, 2], &[]),
            (3..5, &[3, 4], &[1]),
            (5..8, &[5, 6, 7], &[]),
            (8..9, &[8], &[]),
        ];
        test_shaping(sample, Direction::AutoRtl, &expected_rtl[..]);
        test_shaping(sample, Direction::Rtl, &expected_rtl[..]);
    }

    #[test]
    fn test_shaping_rtl() {
        // Additional tests for right-to-left languages: Hebrew, Arabic.
        // Samples are translations of the first article of the UDHR from https://r12a.github.io/

        let sample = "סעיף א. כל בני אדם נולדו בני חורין ושווים בערכם ובזכויותיהם. כולם חוננו בתבונה ובמצפון, לפיכך חובה עליהם לנהוג איש ברעהו ברוח של אחוה.";
        let glyphs = [
            0, 2, 4, 6, 8, 9, 11, 12, 13, 15, 17, 18, 20, 22, 24, 25, 27, 29, 31, 32, 34, 36, 38,
            40, 42, 43, 45, 47, 49, 50, 52, 54, 56, 58, 60, 61, 63, 65, 67, 69, 71, 73, 74, 76, 78,
            80, 82, 84, 85, 87, 89, 91, 93, 95, 97, 99, 101, 103, 105, 107, 108, 109, 111, 113,
            115, 117, 118, 120, 122, 124, 126, 128, 129, 131, 133, 135, 137, 139, 141, 142, 144,
            146, 148, 150, 152, 154, 156, 157, 158, 160, 162, 164, 166, 168, 169, 171, 173, 175,
            177, 178, 180, 182, 184, 186, 188, 189, 191, 193, 195, 197, 199, 200, 202, 204, 206,
            207, 209, 211, 213, 215, 217, 218, 220, 222, 224, 226, 227, 229, 231, 232, 234, 236,
            238, 240,
        ];
        // Checked: using break_gi indices in glyphs yields the break indices from the text_runs test
        let break_gi = [
            5, 8, 11, 15, 19, 25, 29, 35, 42, 48, 61, 66, 72, 79, 88, 94, 99, 105, 111, 115, 121,
            126, 129,
        ];
        test_shaping(
            sample,
            Direction::Auto,
            &[(0..sample.len(), &glyphs, &break_gi)],
        );

        let sample = "المادة 1 يولد جميع الناس أحرارًا متساوين في الكرامة والحقوق. وقد وهبوا عقلاً وضميرًا وعليهم أن يعامل بعضهم بعضًا بروح الإخاء.";
        #[cfg(not(feature = "shaping"))]
        let glyphs_0 = [0, 2, 4, 6, 8, 10, 12];
        #[cfg(feature = "shaping")]
        let glyphs_0 = [0, 2, 4, 6, 8, 10, 10, 12];
        #[cfg(not(feature = "shaping"))]
        let glyphs_2 = [
            14, 15, 17, 19, 21, 23, 24, 26, 28, 30, 32, 33, 35, 37, 39, 41, 43, 44, 46, 48, 50, 52,
            54, 56, 58, 59, 61, 63, 65, 67, 69, 71, 73, 74, 76, 78, 79, 81, 83, 85, 87, 89, 91, 93,
            94, 96, 98, 100, 102, 104, 106, 108, 109, 110, 112, 114, 116, 117, 119, 121, 123, 125,
            127, 128, 130, 132, 134, 136, 138, 139, 141, 143, 145, 147, 149, 151, 153, 154, 156,
            158, 160, 162, 164, 166, 167, 169, 171, 172, 174, 176, 178, 180, 182, 183, 185, 187,
            189, 191, 193, 194, 196, 198, 200, 202, 204, 205, 207, 209, 211, 213, 214, 216, 218,
            220, 222, 224, 226,
        ];
        #[cfg(feature = "shaping")]
        let glyphs_2 = [
            14, 15, 15, 17, 19, 21, 23, 24, 24, 26, 28, 28, 30, 32, 33, 35, 37, 37, 39, 41, 43, 44,
            44, 46, 48, 50, 52, 52, 56, 58, 59, 61, 61, 63, 65, 67, 69, 69, 71, 71, 73, 74, 74, 76,
            76, 78, 79, 81, 83, 85, 87, 89, 91, 91, 93, 94, 96, 98, 100, 102, 102, 104, 106, 106,
            108, 109, 110, 112, 112, 114, 116, 117, 119, 121, 121, 123, 125, 127, 128, 130, 130,
            132, 134, 134, 138, 139, 141, 141, 143, 145, 145, 147, 147, 151, 153, 154, 156, 158,
            160, 160, 162, 164, 166, 167, 167, 169, 169, 171, 172, 172, 174, 176, 178, 180, 182,
            183, 183, 185, 187, 187, 189, 191, 193, 194, 194, 196, 198, 198, 198, 202, 204, 205,
            205, 207, 209, 211, 213, 214, 216, 218, 218, 220, 220, 222, 224, 226,
        ];
        #[cfg(not(feature = "shaping"))]
        let break_gi_2 = [
            1, 6, 11, 17, 25, 33, 36, 44, 53, 57, 63, 69, 77, 84, 87, 93, 99, 105, 110,
        ];
        #[cfg(feature = "shaping")]
        let break_gi_2 = [
            2, 8, 14, 22, 30, 42, 46, 55, 66, 71, 78, 85, 95, 104, 109, 116, 124, 132, 137,
        ];
        test_shaping(
            sample,
            Direction::Auto,
            &[
                (0..13, &glyphs_0, &[]),
                (13..14, &[13], &[]),
                (14..sample.len(), &glyphs_2, &break_gi_2),
            ],
        );
    }
}
