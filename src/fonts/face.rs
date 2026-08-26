// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

use crate::fonts::ScaledFace;
use crate::{DPU, GlyphId};
use easy_cast::Cast;
use fontique::{Blob, Charmap, QueryFont, Synthesis};
#[cfg(not(feature = "shaping"))]
use read_fonts::tables::kern;
use read_fonts::tables::os2::{Os2, SelectionFlags};
use read_fonts::tables::post::Post;
use read_fonts::types::NameId;
use read_fonts::{FontRef, ReadError, TableProvider};
use std::fmt::Display;
use thiserror::Error;

/// Font loading errors
#[derive(Error, Debug)]
pub(super) enum FontError {
    #[error("error reading font: no cmap table found")]
    NoCmap,
    #[error("error reading font")]
    ReadError(#[from] ReadError),
    #[cfg(feature = "ab_glyph")]
    #[error("font load error")]
    AbGlyph(#[from] ab_glyph::InvalidFont),
    #[cfg(feature = "swash")]
    #[error("font load error")]
    Swash,
}

fn opt_table<T>(r: Result<T, ReadError>) -> Result<Option<T>, ReadError> {
    match r {
        Ok(t) => Ok(Some(t)),
        Err(ReadError::TableIsMissing(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Font face data
///
/// This struct is a cache of font-face data. It is immutable once loaded.
/// Various backends are supported, depending on the enabled crate features.
pub struct Face {
    blob: Blob<u8>,
    index: u32,
    ems_per_unit: f32,
    pub(super) ascender: i16,
    pub(super) descender: i16,
    pub(super) line_gap: i16,
    #[cfg(not(feature = "shaping"))]
    pub(super) h_kern: Vec<kern::SubtableKind<'static>>,
    charmap: Charmap<'static>,
    font: FontRef<'static>,
    #[cfg(feature = "shaping")]
    shaper_data: harfrust::ShaperData,
    #[cfg(feature = "shaping")]
    shaper_instance: harfrust::ShaperInstance,
    #[cfg(feature = "ab_glyph")]
    ab_glyph: ab_glyph::FontRef<'static>,
    #[cfg(feature = "swash")]
    swash: (u32, swash::CacheKey), // (offset, key)
    synthesis: Synthesis,
}

impl Face {
    /// Test whether this `Face` matches a query result
    pub(super) fn is_query_font(&self, qf: &QueryFont) -> bool {
        self.blob.id() == qf.blob.id() && self.index == qf.index && self.synthesis == qf.synthesis
    }

    /// Construct, given a data blob, face index and synthesis settings
    pub(super) fn new(qf: &QueryFont) -> Result<Self, FontError> {
        let blob = qf.blob.clone();
        let index = qf.index;
        let synthesis = qf.synthesis;

        // Safety: this is a private fn used to construct a Face instance
        // to be stored in FontLibrary which is never deallocated. This
        // Face holds onto `blob`, so `data` is valid until program exit.
        let data = unsafe { super::extend_lifetime(blob.data()) };

        let font = FontRef::from_index(data, index)?;
        let _ = opt_table(font.name())?;
        let _ = font.hmtx()?; // accessed via unwrap() later
        let _ = opt_table(font.post())?;

        // Read ascent/descent/line_gap values (logic taken from ttf_parser)
        let os2 = font.os2();
        let use_typo_metrics = os2.as_ref().is_ok_and(|os2| {
            os2.fs_selection()
                .contains(SelectionFlags::USE_TYPO_METRICS)
        });
        let (ascender, descender, line_gap) = if !use_typo_metrics
            && let Some(hhea) = opt_table(font.hhea())?
            && hhea.ascender().to_i16() != 0
        {
            let _ = opt_table(os2)?;
            (
                hhea.ascender().to_i16(),
                hhea.descender().to_i16(),
                hhea.line_gap().to_i16(),
            )
        } else {
            let os2 = os2?;
            if use_typo_metrics || os2.s_typo_ascender() != 0 {
                (
                    os2.s_typo_ascender(),
                    os2.s_typo_descender(),
                    os2.s_typo_line_gap(),
                )
            } else {
                (
                    os2.us_win_ascent().cast(),
                    os2.us_win_descent().cast(),
                    os2.s_typo_line_gap(),
                )
            }
        };

        Ok(Face {
            blob,
            index,
            ems_per_unit: 1f32 / f32::from(font.head()?.units_per_em()),
            ascender,
            descender,
            line_gap,
            #[cfg(not(feature = "shaping"))]
            h_kern: {
                let mut subtables = vec![];
                if let Some(table) = opt_table(font.kern())? {
                    for sub in table.subtables() {
                        let sub = sub?;
                        if sub.is_horizontal() && !sub.is_variable() {
                            subtables.push(sub.kind()?);
                        }
                    }
                }
                subtables
            },
            charmap: qf.charmap_index.charmap(data).ok_or(FontError::NoCmap)?,
            #[cfg(feature = "shaping")]
            shaper_data: harfrust::ShaperData::new(&font),
            #[cfg(feature = "shaping")]
            shaper_instance: harfrust::ShaperInstance::from_variations(
                &font,
                synthesis.variation_settings(),
            ),
            font,
            #[cfg(feature = "ab_glyph")]
            ab_glyph: {
                let mut font = ab_glyph::FontRef::try_from_slice_and_index(data, index)?;
                for (tag, value) in synthesis.variation_settings() {
                    ab_glyph::VariableFont::set_variation(&mut font, &tag.to_be_bytes(), *value);
                }
                font
            },
            #[cfg(feature = "swash")]
            swash: {
                use easy_cast::Cast;
                let f = swash::FontRef::from_index(data, index.cast()).ok_or(FontError::Swash)?;
                (f.offset, f.key)
            },
            synthesis,
        })
    }

    /// Attempt to read a specific name
    ///
    /// Decoding is best effort and may fail. Lossy decoding is used (may
    /// replace data with [`std::char::REPLACEMENT_CHARACTER`]).
    ///
    /// See [Microsoft's documentation].
    ///
    /// [Microsoft's documentation]: https://learn.microsoft.com/en-us/typography/opentype/spec/name
    pub fn read_name(&self, id: NameId) -> Option<impl Display> {
        let Ok(table) = self.font.name() else {
            return None;
        };
        let record = table.name_record().get(id.to_u16() as usize)?;
        // Note: we do not report read errors here
        Some(record.string(table.string_data()).ok()?)
    }

    /// Get the font family name
    #[inline]
    pub fn name_family(&self) -> Option<impl Display> {
        self.read_name(NameId::FAMILY_NAME)
    }

    /// Get the font sub-family (i.e. style) name
    #[inline]
    pub fn name_subfamily(&self) -> Option<impl Display> {
        self.read_name(NameId::SUBFAMILY_NAME)
    }

    /// Get the full font name
    #[inline]
    pub fn name_full(&self) -> Option<impl Display> {
        self.read_name(NameId::FULL_NAME)
    }

    /// Get the face index within the font file
    #[inline]
    pub fn face_index(&self) -> u32 {
        self.index
    }

    /// Get a [`read_fonts::FontRef`]
    ///
    /// [`read_fonts::FontRef`]: https://docs.rs/read-fonts/latest/read_fonts/struct.FontRef.html
    pub fn font_ref(&self) -> &FontRef<'_> {
        &self.font
    }

    /// Access the OS/2 table
    pub(crate) fn os2(&self) -> Option<Os2<'_>> {
        self.font.os2().ok()
    }

    /// Access the post (PostScript) table
    pub(crate) fn post(&self) -> Option<Post<'_>> {
        self.font.post().ok()
    }

    /// Get a [`harfrust::Shaper`] for this font
    ///
    /// [`harfrust::Shaper`]: https://docs.rs/harfrust/latest/harfrust/struct.Shaper.html
    #[cfg(feature = "shaping")]
    pub(crate) fn shaper(&self) -> harfrust::Shaper<'_> {
        self.shaper_data
            .shaper(&self.font)
            .instance(Some(&self.shaper_instance))
            .build()
    }

    /// Get a [`ab_glyph::FontRef`]
    ///
    /// [`ab_glyph::FontRef`]: https://docs.rs/ab_glyph/latest/ab_glyph/struct.FontRef.html
    #[cfg(feature = "ab_glyph")]
    pub fn ab_glyph(&self) -> &ab_glyph::FontRef<'static> {
        &self.ab_glyph
    }

    /// Get a [`swash::FontRef`]
    ///
    /// [`swash::FontRef`]: https://docs.rs/swash/latest/swash/struct.FontRef.html
    #[cfg(feature = "swash")]
    pub fn swash(&self) -> swash::FontRef<'_> {
        swash::FontRef {
            data: self.blob.data(),
            offset: self.swash.0,
            key: self.swash.1,
        }
    }

    /// Get font variation settings aka [`Synthesis`]
    ///
    /// These settings are used for example to support variable weight fonts
    /// and synthesized italics.
    ///
    /// [`Synthesis`]: https://docs.rs/fontique/latest/fontique/struct.Synthesis.html
    pub fn synthesis(&self) -> &Synthesis {
        &self.synthesis
    }

    /// Find a glyph within the font face
    ///
    /// To use the "missing ideograph" (white square) fallback for missing
    /// glyphs use `store.glyph_index(c).unwrap_or_default()`.
    pub fn glyph_index(&self, code_point: char) -> Option<GlyphId> {
        self.charmap.map(code_point as u32).map(GlyphId::new)
    }

    /// Convert `dpem` to `dpu`
    ///
    /// Output: a font-specific scale.
    ///
    /// Input: `dpem` is pixels/em
    ///
    /// ```none
    /// dpem
    ///   = pt_size × dpp
    ///   = pt_size × dpi / 72
    ///   = pt_size × scale_factor × (96 / 72)
    /// ```
    #[inline]
    pub fn dpu(&self, dpem: f32) -> DPU {
        DPU(dpem * self.ems_per_unit)
    }

    /// Get a scaled reference
    ///
    /// Units: `dpem` is dots (pixels) per Em (module documentation).
    #[inline]
    pub fn scale_by_dpem(&self, dpem: f32) -> ScaledFace<'_> {
        self.scale_by_dpu(self.dpu(dpem))
    }

    /// Get a scaled reference
    ///
    /// Units: `dpu` is dots (pixels) per font-unit (see module documentation).
    #[inline]
    pub fn scale_by_dpu(&self, dpu: DPU) -> ScaledFace<'_> {
        let hmtx = self.font.hmtx().unwrap();
        ScaledFace::new(self, hmtx, dpu)
    }
}
