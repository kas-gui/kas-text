// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — prepared text

use ab_glyph::Font;
use glyph_brush_layout::{GlyphPositioner, Layout, SectionGeometry, SectionGlyph, SectionText};

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

        let geometry = SectionGeometry {
            screen_position: (0.0, 0.0),
            bounds: self.bounds.into(),
        };

        let sections: Vec<_> = self
            .parts
            .iter()
            .map(|part| SectionText {
                text: &part.text,
                scale: (self.base_scale * part.scale).into(),
                font_id: part.font_id.into(),
            })
            .collect();

        self.glyphs = layout.calculate_glyphs(&fonts, &geometry, &sections);
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
    /// The offset is used to adjust the glyph position.
    pub fn positioned_glyphs(&self, offset: Size) -> Vec<SectionGlyph> {
        assert!(self.ready, "PreparedText: not ready");
        let mut glyphs = self.glyphs.clone();
        let offset = self.offset + offset;
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
            Size(0.0, 0.0)
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
