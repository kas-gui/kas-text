// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — prepared text

use crate::{Align, FontId, FontScale, RichText, Size, TextPart};

/// Text, prepared for display in a given enviroment
///
/// Text is laid out for display in a box with the given size.
///
/// The type can be default-constructed with no text.
#[derive(Clone, Debug, Default)]
pub struct PreparedText {
    align_horiz: Align,
    align_vert: Align,
    line_wrap: bool,
    // If !ready, then fonts must be selected and layout calculated
    ready: bool,
    size: Size,
    parts: Vec<PreparedPart>,
}

#[derive(Clone, Debug, Default)]
pub struct PreparedPart {
    text: String,
    font_id: FontId,
    scale: FontScale,
}

impl PreparedText {
    /// Construct from a text model
    ///
    /// This method assumes default alignment. To adjust, use [`PreparedText::set_alignment`].
    ///
    /// This method does not calculate text layout or size requirements.
    /// To do so, call [`PreparedText::set_environment`].
    pub fn new(text: RichText, line_wrap: bool) -> PreparedText {
        PreparedText {
            line_wrap,
            parts: text
                .parts
                .iter()
                .map(|part| PreparedPart {
                    text: part.text.clone(), // TODO: reference?
                    ..Default::default()
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
    pub fn set_text(&mut self, text: RichText) {
        self.parts = text
            .parts
            .iter()
            .map(|part| PreparedPart {
                text: part.text.clone(),
                ..Default::default()
            })
            .collect();
        self.ready = false;
    }

    /// Adjust alignment
    pub fn set_alignment(&mut self, horiz: Align, vert: Align) {
        self.align_horiz = horiz;
        self.align_vert = vert;
    }

    /// Enable or disable line-wrapping
    pub fn set_line_wrap(&mut self, line_wrap: bool) {
        self.line_wrap = line_wrap;
    }

    /// True if the user must call `set_font` before further use
    #[inline]
    pub fn require_font(&self) -> bool {
        !self.ready
    }

    /// Set fonts
    pub fn set_font(&mut self, font_id: FontId, scale: FontScale) {
        for part in &mut self.parts {
            part.font_id = font_id;
            part.scale = scale;
        }
        self.ready = true;
    }

    /// Set size bounds
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
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
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn parts<'a>(&'a self) -> impl Iterator<Item = &'a PreparedPart> {
        assert!(self.ready, "PreparedText::parts(): not ready");
        (&self.parts).iter()
    }
}

impl PreparedPart {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn scale(&self) -> FontScale {
        self.scale
    }
    pub fn font_id(&self) -> FontId {
        self.font_id
    }
}
