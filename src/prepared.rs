// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use ab_glyph::{Font, Glyph, ScaleFont};
use glyph_brush_layout::{GlyphPositioner, Layout, SectionGeometry, SectionGlyph, SectionText};
use unicode_segmentation::GraphemeCursor;

use crate::{fonts, rich, Align, FontId, FontScale, Range, Vec2};

/// Text, prepared for display in a given enviroment
///
/// Text is laid out for display in a box with the given size bounds.
///
/// The type can be default-constructed with no text.
#[derive(Clone, Debug, Default)]
pub struct Text {
    text: String,
    runs: SmallVec<[Run; 1]>,
    align_horiz: Align,
    align_vert: Align,
    line_wrap: bool,
    // If !ready, then fonts must be selected and layout calculated
    ready: bool,
    bounds: Vec2,
    required: Vec2,
    offset: Vec2,
    base_scale: FontScale,
    glyphs: Vec<SectionGlyph>,
}

#[derive(Clone, Debug)]
struct Run {
    range: Range,
    // scale is relative to base_scale
    scale: f32,
    font_id: FontId,
}

impl Text {
    /// Construct from a text model
    ///
    /// This method assumes default alignment. To adjust, use [`Text::set_alignment`].
    ///
    /// This struct must be made ready for use before
    /// To do so, call [`Text::prepare`].
    pub fn new(text: rich::Text, line_wrap: bool) -> Text {
        Text {
            text: text.text,
            runs: text
                .runs
                .iter()
                .map(|run| Run {
                    range: run.range,
                    scale: 1.0,
                    font_id: Default::default(),
                })
                .collect(),
            line_wrap,
            ..Default::default()
        }
    }

    /// Reconstruct the [`rich::Text`] model defining this `Text`
    pub fn clone_text(&self) -> rich::Text {
        rich::Text {
            text: self.text.clone(),
            runs: self
                .runs
                .iter()
                .map(|run| rich::Run { range: run.range })
                .collect(),
        }
    }

    /// Length of raw text
    ///
    /// It is valid to reference text within the range `0..raw_text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    pub fn raw_text_len(&self) -> usize {
        self.text.len()
    }

    /// Set the text
    ///
    /// Returns true when the contents have changed, in which case
    /// `prepared` must be called again and size-requirements may have changed.
    pub fn set_text(&mut self, text: rich::Text) -> bool {
        if self.text == text.text && self.runs.len() == text.runs.len() {
            if self
                .runs
                .iter()
                .zip(text.runs.iter())
                .all(|(p, m)| p.range == m.range && p.scale == 1.0 && p.font_id.get() == 0)
            {
                return false; // no change
            }
        }

        self.text = text.text;
        self.runs = text
            .runs
            .iter()
            .map(|run| Run {
                range: run.range,
                scale: 1.0,
                font_id: Default::default(),
            })
            .collect();
        self.ready = false;
        true
    }

    /// Adjust alignment
    pub fn set_alignment(&mut self, horiz: Align, vert: Align) {
        if self.align_horiz != horiz || self.align_vert != vert {
            self.align_horiz = horiz;
            self.align_vert = vert;
            self.apply_alignment();
        }
    }

    /// Enable or disable line-wrapping
    pub fn set_line_wrap(&mut self, line_wrap: bool) {
        if self.line_wrap != line_wrap {
            self.line_wrap = line_wrap;
            self.ready = false;
        }
    }

    /// Prepare
    ///
    /// Calculates glyph layouts for use.
    pub fn prepare(&mut self) {
        if self.ready {
            return;
        }

        // NOTE: we can only use left-alignment here since bounds may be
        // infinite.

        let layout = match self.line_wrap {
            true => Layout::default_wrap(),
            false => Layout::default_single_line(),
        };

        let fonts = fonts().fonts_vec();

        let geometry = SectionGeometry {
            screen_position: (0.0, 0.0),
            bounds: self.bounds.into(),
        };

        let sections: Vec<_> = self
            .runs
            .iter()
            .map(|run| SectionText {
                text: &self.text[run.range],
                scale: (self.base_scale * run.scale).into(),
                font_id: run.font_id.into(),
            })
            .collect();

        self.glyphs = layout.calculate_glyphs(&fonts, &geometry, &sections);
        self.update_required();
        self.apply_alignment();
        self.ready = true;
    }

    /// Set size bounds
    pub fn set_bounds(&mut self, bounds: Vec2) {
        self.bounds = bounds;
        self.apply_alignment();
    }

    /// Set base font scale
    pub fn set_base_scale(&mut self, scale: FontScale) {
        self.base_scale = scale;
        self.ready = false;
    }

    pub fn align_horiz(&self) -> Align {
        self.align_horiz
    }
    pub fn align_vert(&self) -> Align {
        self.align_vert
    }
    pub fn line_wrap(&self) -> bool {
        self.line_wrap
    }
    pub fn bounds(&self) -> Vec2 {
        self.bounds
    }

    /// Get an iterator over positioned glyphs
    ///
    /// One must call [`Text::prepare`] before this method.
    ///
    /// The `pos` is used to adjust the glyph position: this is the top-left
    /// position of the rect within which glyphs appear (the size of the rect
    /// is that passed to [`Text::set_bounds`]).
    pub fn positioned_glyphs(&self, pos: Vec2) -> GlyphIter {
        assert!(self.ready, "kas-text::prepared::Text: not ready");
        GlyphIter {
            offset: ab_glyph::Point::from(self.offset + pos),
            glyphs: &self.glyphs,
        }
    }

    /// Calculate size requirements
    ///
    /// One must set initial size bounds and call [`Text::prepare`]
    /// before this method. Note that initial size bounds may cause wrapping
    /// and may cause parts of the text outside the bounds to be cut off.
    pub fn required_size(&self) -> Vec2 {
        self.required
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// May panic on invalid byte index.
    ///
    /// This method is only partially compatible with mult-line text.
    /// Ideally an external line-breaker should be used.
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to this result.
    pub fn text_glyph_pos(&self, index: usize) -> Vec2 {
        if index == 0 {
            // Short-cut. We also cannot iterate since there may be no glyphs.
            return self.offset;
        }

        let byte = if index < self.text.len() {
            Some(self.text.as_bytes()[index])
        } else {
            None
        };

        // Translate index to part number and part's byte-index.
        let (section, section_index) = 'outer: loop {
            for run in 0..self.runs.len() {
                if self.runs[run].range.contains(index) {
                    // NOTE: for now, a run corresponds directly to a section
                    break 'outer (run, index - self.runs[run].range.start());
                }
            }
            // No corresponding glyph -  TODO - how do we handle this?
            panic!("no glyph");
        };

        let mut iter = self.glyphs.iter();

        let mut advance = false;
        let mut glyph;
        if let Some(byte) = byte {
            // Tiny HACK: line-breaks don't have glyphs
            if byte == b'\r' || byte == b'\n' {
                advance = true;
            }

            glyph = iter.next().unwrap().clone();
            for next in iter {
                if next.section_index > section || next.byte_index > section_index {
                    // Use the previous glyph, e.g. if in the middle of a
                    // multi-byte sequence or index is a combining diacritic.
                    break;
                }
                glyph = next.clone();
            }
        } else {
            advance = true;
            glyph = iter.last().unwrap().clone();
        }

        let mut pos = self.offset + glyph.glyph.position.into();
        let scale_font = fonts().get(glyph.font_id).into_scaled(glyph.glyph.scale);
        if advance {
            pos.0 += scale_font.h_advance(glyph.glyph.id);
        }
        pos.1 -= scale_font.ascent();
        return pos;
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// Note: if the font's rect does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// This method is only partially compatible with mult-line text.
    /// Ideally an external line-breaker should be used.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        if self.runs.len() == 0 {
            return 0; // short-cut
        }
        let text_len = self.text.len();
        // NOTE: if self.runs.len() > 1 then base_to_mid may change, making the
        // row selection a little inaccurate. This method is best used with only
        // a single row of text anyway, so we consider this acceptable.
        // This also affects scale_font.h_advance at line-breaks. We consider
        // this a hack anyway and so tolerate some inaccuracy.
        let last_run = self.runs.last().unwrap();
        let scale = self.base_scale * last_run.scale;
        let scale_font = fonts().get(last_run.font_id).into_scaled(scale);
        let base_to_mid = -0.5 * (scale_font.ascent() + scale_font.descent());
        // Note: scale_font.line_gap() is 0.0 (why?). Use ascent() instead.
        let half_line_gap = (0.5 * scale_font.ascent()).abs();

        let mut iter = self.glyphs.iter();

        // Find the (horiz, vert) distance between pos and the glyph.
        let dist = |glyph: &Glyph| {
            let p = glyph.position;
            let glyph_pos = Vec2(p.x, p.y + base_to_mid);
            (pos - glyph_pos).abs()
        };
        let test_best = |best: Vec2, glyph: &Glyph| {
            let dist = dist(glyph);
            if dist.1 < best.1 {
                Some(dist)
            } else if dist.1 == best.1 && dist.0 < best.0 {
                Some(dist)
            } else {
                None
            }
        };

        let mut last: SectionGlyph = iter.next().unwrap().clone();
        let mut last_y = last.glyph.position.y;
        let mut best = (last.byte_index, dist(&last.glyph));
        for next in iter {
            // Heuristic to detect a new line. This is a HACK to handle
            // multi-line texts since line-end positions are not represented by
            // virtual glyphs (unlike spaces).
            if (next.glyph.position.y - last_y).abs() > half_line_gap {
                last.glyph.position.x += scale_font.h_advance(last.glyph.id);
                if let Some(new_best) = test_best(best.1, &last.glyph) {
                    let index = last.byte_index;
                    let mut cursor = GraphemeCursor::new(index, text_len, true);
                    let mut cum_len = 0;
                    let text = 'outer: loop {
                        for run in &self.runs {
                            let len = run.range.len();
                            if index < cum_len + len {
                                break 'outer &self.text[run.range];
                            }
                            cum_len += len;
                        }
                        unreachable!();
                    };
                    let byte = cursor
                        .next_boundary(text, cum_len)
                        .unwrap()
                        .unwrap_or(last.byte_index);
                    best = (byte, new_best);
                }
            }

            last = next.clone();
            last_y = last.glyph.position.y;
            if let Some(new_best) = test_best(best.1, &last.glyph) {
                best = (last.byte_index, new_best);
            }
        }

        // We must also consider the position after the last glyph
        last.glyph.position.x += scale_font.h_advance(last.glyph.id);
        if let Some(new_best) = test_best(best.1, &last.glyph) {
            best = (text_len, new_best);
        }

        assert!(
            best.0 <= text_len,
            "text_index_nearest: index beyond text length!"
        );
        best.0
    }
}

impl Text {
    fn update_required(&mut self) {
        let fonts = fonts();
        let glyph_bounds = |g: &SectionGlyph| {
            let bounds = fonts.get(g.font_id).glyph_bounds(&g.glyph);
            (Vec2::from(bounds.min), Vec2::from(bounds.max))
        };

        let mut iter = self.glyphs.iter();
        self.required = if let Some(first) = iter.next() {
            let mut bounds = glyph_bounds(first);
            for item in iter {
                let b = glyph_bounds(item);
                bounds.0 = bounds.0.min(b.0);
                bounds.1 = bounds.1.max(b.1);
            }
            bounds.1 - bounds.0
        } else {
            Vec2::ZERO
        };
    }

    fn apply_alignment(&mut self) {
        // TODO: this aligns the whole block of text; we should align each line
        self.offset.0 = match self.align_horiz {
            Align::Default | Align::TL | Align::Stretch => 0.0,
            Align::Centre => (self.bounds.0 - self.required.0) / 2.0,
            Align::BR => self.bounds.0 - self.required.0,
        };
        self.offset.1 = match self.align_vert {
            Align::Default | Align::TL | Align::Stretch => 0.0,
            Align::Centre => (self.bounds.1 - self.required.1) / 2.0,
            Align::BR => self.bounds.1 - self.required.1,
        };
    }
}

pub struct GlyphIter<'a> {
    offset: ab_glyph::Point,
    glyphs: &'a [SectionGlyph],
}

impl<'a> Iterator for GlyphIter<'a> {
    type Item = SectionGlyph;

    fn next(&mut self) -> Option<Self::Item> {
        if self.glyphs.len() > 0 {
            let mut glyph = self.glyphs[0].clone();
            self.glyphs = &self.glyphs[1..];
            glyph.glyph.position += self.offset;
            Some(glyph)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.glyphs.len();
        (len, Some(len))
    }
}
impl<'a> ExactSizeIterator for GlyphIter<'a> {}
impl<'a> std::iter::FusedIterator for GlyphIter<'a> {}
