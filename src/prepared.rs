// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — prepared text

use crate::layout::{GlyphPositioner, Layout, SectionGlyph, SectionText};
use ab_glyph::{Font, Glyph, ScaleFont};
use unicode_segmentation::GraphemeCursor;

use crate::{fonts, Align, FontId, FontScale, RichText, Size, TextPart};

/// Text, prepared for display in a given enviroment
///
/// Text is laid out for display in a box with the given size bounds.
///
/// The type can be default-constructed with no text.
#[derive(Clone, Debug, Default)]
pub struct PreparedText {
    align_horiz: Align,
    align_vert: Align,
    line_wrap: bool,
    // If !ready, then fonts must be selected and layout calculated
    ready: bool,
    bounds: Size,
    required: Size,
    offset: Size,
    base_scale: FontScale,
    parts: Vec<PreparedPart>,
    glyphs: Vec<SectionGlyph>,
}

#[derive(Clone, Debug)]
pub struct PreparedPart {
    text: String,
    // scale is relative to base_scale
    scale: f32,
    font_id: FontId,
}

impl PreparedText {
    /// Construct from a text model
    ///
    /// This method assumes default alignment. To adjust, use [`PreparedText::set_alignment`].
    ///
    /// This struct must be made ready for use before
    /// To do so, call [`PreparedText::set_environment`].
    pub fn new(text: RichText, line_wrap: bool) -> PreparedText {
        PreparedText {
            line_wrap,
            parts: text
                .parts
                .iter()
                .map(|part| PreparedPart {
                    text: part.text.clone(), // TODO: reference?
                    scale: 1.0,
                    font_id: Default::default(),
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Reconstruct the [`RichText`] defining this `PreparedText`
    pub fn clone_text(&self) -> RichText {
        let parts = self
            .parts
            .iter()
            .map(|part| TextPart {
                text: part.text.clone(),
            })
            .collect();
        RichText { parts }
    }

    /// Index at end of text
    pub fn total_len(&self) -> usize {
        self.parts.iter().map(|part| part.text.len()).sum()
    }

    /// Set the text
    ///
    /// Returns true when the contents have changed, in which case
    /// `prepared` must be called again and size-requirements may have changed.
    pub fn set_text(&mut self, text: RichText) -> bool {
        if self.parts.len() == text.parts.len() {
            if self
                .parts
                .iter()
                .zip(text.parts.iter())
                .all(|(p, t)| p.text == t.text && p.scale == 1.0 && p.font_id.get() == 0)
            {
                return false; // no change
            }
        }

        self.parts = text
            .parts
            .iter()
            .map(|part| PreparedPart {
                text: part.text.clone(),
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

        let sections: Vec<_> = self
            .parts
            .iter()
            .map(|part| SectionText {
                text: &part.text,
                scale: (self.base_scale * part.scale).into(),
                font_id: part.font_id.into(),
            })
            .collect();

        self.glyphs = layout.calculate_glyphs(&fonts, self.bounds, &sections);
        self.update_required();
        self.apply_alignment();
        self.ready = true;
    }

    /// Set size bounds
    pub fn set_bounds(&mut self, bounds: Size) {
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
    pub fn bounds(&self) -> Size {
        self.bounds
    }

    pub fn num_parts(&self) -> usize {
        self.parts.len()
    }

    /// Get the list of positioned glyphs
    ///
    /// One must call [`PreparedText::prepare`] before this method.
    ///
    /// The `pos` is used to adjust the glyph position: this is the top-left
    /// position of the rect within which glyphs appear (the size of the rect
    /// is that passed to [`PreparedText::set_bounds`]).
    pub fn positioned_glyphs(&self, pos: Size) -> Vec<SectionGlyph> {
        assert!(self.ready, "PreparedText: not ready");
        let mut glyphs = self.glyphs.clone();
        let offset = self.offset + pos;
        if offset != Size::default() {
            let offset = ab_glyph::Point::from(offset);
            for glyph in &mut glyphs {
                glyph.glyph.position += offset;
            }
        }
        glyphs
    }

    /// Calculate size requirements
    ///
    /// One must set initial size bounds and call [`PreparedText::prepare`]
    /// before this method. Note that initial size bounds may cause wrapping
    /// and may cause parts of the text outside the bounds to be cut off.
    pub fn required_size(&self) -> Size {
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
    pub fn text_glyph_pos(&self, index: usize) -> Size {
        if index == 0 {
            // Short-cut. We also cannot iterate since there may be no glyphs.
            return self.offset;
        }

        // Translate index to part number and part's byte-index.
        let mut section = 0;
        let mut section_index = index;
        let mut byte = None;
        while section < self.parts.len() {
            let section_len = self.parts[section].text.len();
            if section_index < section_len {
                byte = Some(self.parts[section].text.as_bytes()[section_index]);
                break;
            };
            section += 1;
            section_index -= section_len;
        }

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
    pub fn text_index_nearest(&self, pos: Size) -> usize {
        if self.parts.len() == 0 {
            return 0; // short-cut
        }
        let text_len = self.total_len();
        // NOTE: if self.parts.len() > 1 then base_to_mid may change, making the
        // row selection a little inaccurate. This method is best used with only
        // a single row of text anyway, so we consider this acceptable.
        // This also affects scale_font.h_advance at line-breaks. We consider
        // this a hack anyway and so tolerate some inaccuracy.
        let last_part = self.parts.last().unwrap();
        let scale = self.base_scale * last_part.scale;
        let scale_font = fonts().get(last_part.font_id).into_scaled(scale);
        let base_to_mid = -0.5 * (scale_font.ascent() + scale_font.descent());
        // Note: scale_font.line_gap() is 0.0 (why?). Use ascent() instead.
        let half_line_gap = (0.5 * scale_font.ascent()).abs();

        let mut iter = self.glyphs.iter();

        // Find the (horiz, vert) distance between pos and the glyph.
        let dist = |glyph: &Glyph| {
            let p = glyph.position;
            let glyph_pos = Size(p.x, p.y + base_to_mid);
            (pos - glyph_pos).abs()
        };
        let test_best = |best: Size, glyph: &Glyph| {
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
                        for part in &self.parts {
                            let len = part.text.len();
                            if index < cum_len + len {
                                break 'outer &part.text;
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

impl PreparedText {
    fn update_required(&mut self) {
        let fonts = fonts();
        let glyph_bounds = |g: &SectionGlyph| {
            let bounds = fonts.get(g.font_id).glyph_bounds(&g.glyph);
            (Size::from(bounds.min), Size::from(bounds.max))
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
            Size::ZERO
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

impl PreparedPart {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn font_id(&self) -> FontId {
        self.font_id
    }
}
