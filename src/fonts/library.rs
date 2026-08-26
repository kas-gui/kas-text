// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

use super::{FontSelector, Resolver};
use crate::conv::{to_u32, to_usize};
use crate::fonts::ScaledFace;
use crate::{DPU, GlyphId};
use easy_cast::Cast;
use fontique::{Blob, Charmap, QueryFont, QueryStatus, Script, Synthesis};
#[cfg(not(feature = "shaping"))]
use read_fonts::tables::kern;
use read_fonts::tables::os2::{Os2, SelectionFlags};
use read_fonts::tables::post::Post;
use read_fonts::types::NameId;
use read_fonts::{FontRef, ReadError, TableProvider};
use std::collections::hash_map::{Entry, HashMap};
use std::fmt::Display;
use std::sync::{LazyLock, Mutex, MutexGuard};
use thiserror::Error;

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
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

/// Bad [`FontId`] or no font loaded
///
/// This error should be impossible to observe, but exists to avoid panic in
/// lower level methods.
#[derive(Error, Debug)]
#[error("invalid FontId")]
pub(crate) struct InvalidFontId;

/// No matching font found
///
/// Text layout failed.
#[derive(Clone, Copy, Error, Debug)]
#[error("no font match")]
pub struct NoFontMatch;

/// Font face identifier
///
/// Identifies a loaded font face within the [`FontLibrary`] by index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceId(u32);
impl FaceId {
    /// Get as `usize`
    pub fn get(self) -> usize {
        to_usize(self.0)
    }
}

impl From<u32> for FaceId {
    fn from(id: u32) -> Self {
        FaceId(id)
    }
}

/// Font list identifier
///
/// A "font" is a list of faces selected for a given [`FontSelector`] and
/// [`Script`].
///
/// Identifies a font list within the [`FontLibrary`] by index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FontId(u32);

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
    /// Construct, given a data blob, face index and synthesis settings
    fn new(qf: &QueryFont) -> Result<Self, FontError> {
        let blob = qf.blob.clone();
        let index = qf.index;
        let synthesis = qf.synthesis;

        // Safety: this is a private fn used to construct a Face instance
        // to be stored in FontLibrary which is never deallocated. This
        // Face holds onto `blob`, so `data` is valid until program exit.
        let data = unsafe { extend_lifetime(blob.data()) };

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

/// A "font" is a list of faces (primary + fallbacks)
struct Font {
    id: FontId,
    faces: Vec<FaceId>,
    /// Cached `char -> FaceId` lookups
    glyph_map: HashMap<char, Option<FaceId>>,
}

#[derive(Default)]
struct FontList {
    // Safety: unsafe code depends on entries never moving (hence the otherwise
    // redundant use of Box). See e.g. FontLibrary::get_face().
    #[allow(clippy::vec_box)]
    faces: Vec<Box<Face>>,
    // These are vec-maps. Why? Because length should be short.
    source_hash: Vec<(u64, FaceId)>,
    fonts: Vec<Font>,
    sel_hash: Vec<(u64, FontId)>,
}

impl FontList {
    fn push_face(&mut self, face: Box<Face>, source_hash: u64) -> FaceId {
        let id = FaceId(to_u32(self.faces.len()));
        self.faces.push(face);
        self.source_hash.push((source_hash, id));
        id
    }

    fn push_font(&mut self, faces: Vec<FaceId>, sel_hash: u64) -> FontId {
        let id = FontId(to_u32(self.fonts.len()));
        self.fonts.push(Font {
            id,
            faces,
            glyph_map: HashMap::new(),
        });
        self.sel_hash.push((sel_hash, id));
        id
    }

    fn face_for_char(
        &mut self,
        font_id: FontId,
        preferred_face: Option<FaceId>,
        c: char,
    ) -> Result<Option<FaceId>, InvalidFontId> {
        // TODO: `face.glyph_index` is a bit slow to use like this where several
        // faces may return no result before we find a match. Caching results
        // in a HashMap helps. Perhaps better would be to (somehow) determine
        // the script/language in use and check whether the font face supports
        // that, perhaps also checking it has shaping support.
        let faces = &self.faces;
        let fonts = &mut self.fonts;
        let font = fonts
            .iter_mut()
            .find(|item| item.id == font_id)
            .ok_or(InvalidFontId)?;

        if let Some(face_id) = preferred_face
            && font.faces.contains(&face_id)
        {
            let face = &faces[face_id.get()];
            // TODO(opt): should we cache this lookup?
            if face.glyph_index(c).is_some() {
                return Ok(Some(face_id));
            }
        }

        Ok(match font.glyph_map.entry(c) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let mut id: Option<FaceId> = None;
                for face_id in font.faces.iter() {
                    let face = &faces[face_id.get()];
                    if face.glyph_index(c).is_some() {
                        id = Some(*face_id);
                        break;
                    }
                }

                // TODO: we need some mechanism to widen the search when this
                // fails (certain chars might only be found in a special font).

                entry.insert(id);
                id
            }
        })
    }
}

/// Library of loaded fonts
///
/// This is the type of the global singleton accessible via the [`library()`]
/// function. Thread-safety is handled via internal locks.
pub struct FontLibrary {
    resolver: Mutex<Resolver>,
    fonts: Mutex<FontList>,
}

/// Font management
impl FontLibrary {
    /// Get a reference to the font resolver
    pub fn resolver(&self) -> MutexGuard<'_, Resolver> {
        self.resolver.lock().unwrap()
    }

    /// Get the first face for a font
    ///
    /// Each font identifier has at least one font face. This resolves the first
    /// (default) one.
    pub(crate) fn first_face_for(&self, font_id: FontId) -> Result<FaceId, InvalidFontId> {
        let fonts = self.fonts.lock().unwrap();
        for font in &fonts.fonts {
            if font.id == font_id {
                return Ok(*font.faces.first().unwrap());
            }
        }
        Err(InvalidFontId)
    }

    /// Resolve the font face for a character
    ///
    /// If `preferred_face` is a face used by `font_id` and this face covers
    /// `c`, then return `preferred_face`.
    /// Otherwise, return the first face of `font_id` which covers `c`, if any.
    pub(crate) fn face_for_char(
        &self,
        font_id: FontId,
        preferred_face: Option<FaceId>,
        c: char,
    ) -> Result<Option<FaceId>, InvalidFontId> {
        self.fonts
            .lock()
            .unwrap()
            .face_for_char(font_id, preferred_face, c)
    }

    /// Select a font
    ///
    /// This method uses internal caching to enable fast look-ups of existing
    /// (loaded) fonts. Resolving new fonts may be slower.
    pub(crate) fn select_font(
        &self,
        selector: &FontSelector,
        script: Script,
    ) -> Result<FontId, NoFontMatch> {
        let sel_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut s = DefaultHasher::new();
            selector.hash(&mut s);
            script.hash(&mut s);
            s.finish()
        };

        let mut resolver = self.resolver.lock().unwrap();
        let mut fonts = self.fonts.lock().unwrap();

        for (h, id) in &fonts.sel_hash {
            if *h == sel_hash {
                return Ok(*id);
            }
        }

        let mut faces = Vec::new();
        let mut families = Vec::new();

        selector.select(&mut resolver, script, |qf| {
            if log::log_enabled!(log::Level::Debug) {
                families.push(qf.family);
            }

            let source_hash = {
                use std::hash::{DefaultHasher, Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                qf.blob.id().hash(&mut hasher);
                hasher.write_u32(qf.index);
                // Hashing of qf.synthesis is incomplete, but we use an equality test later anyway
                for var in qf.synthesis.variation_settings() {
                    var.0.hash(&mut hasher);
                }
                qf.synthesis.embolden().hash(&mut hasher);
                qf.synthesis.skew().is_some().hash(&mut hasher);
                hasher.finish()
            };

            for (h, id) in fonts.source_hash.iter().cloned() {
                if h == source_hash {
                    let face = &fonts.faces[id.get()];
                    if face.blob.id() == qf.blob.id()
                        && face.index == qf.index
                        && face.synthesis == qf.synthesis
                    {
                        faces.push(id);
                        return QueryStatus::Continue;
                    }
                }
            }

            match Face::new(qf) {
                Ok(store) => {
                    if let Some(name) = store.name_full() {
                        if store.synthesis == Synthesis::default() {
                            log::debug!("Loaded font: {name}");
                        } else {
                            log::debug!("Loaded font: {name} with {:?}", &store.synthesis);
                        }
                    }
                    let id = fonts.push_face(Box::new(store), source_hash);
                    faces.push(id);
                }
                Err(err) => {
                    log::error!("Failed to load font: {err}");
                }
            }

            QueryStatus::Continue
        });

        for family in families {
            if let Some(name) = resolver.font_family(family.0) {
                log::trace!("match: {name}");
            }
        }

        if faces.is_empty() {
            return Err(NoFontMatch);
        }
        let font = fonts.push_font(faces, sel_hash);
        Ok(font)
    }
}

/// Face management
impl FontLibrary {
    /// Get the [`Face`] for a given `id`
    ///
    /// Panics if `id` is not valid (required: `id.get() < self.num_faces()`).
    /// This shouldn't be the case for any [`FaceId`] returned by this library.
    ///
    /// This method returns a `'static` reference: font face data is immutable
    /// once loaded and is never freed.
    pub fn get_face(&self, id: FaceId) -> &'static Face {
        let fonts = self.fonts.lock().unwrap();
        assert!(id.get() < fonts.faces.len(), "FontLibrary: invalid {id:?}!",);
        let faces: &Face = &fonts.faces[id.get()];
        // Safety: elements of self.faces are never dropped or modified
        unsafe { extend_lifetime(faces) }
    }
}

unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
    unsafe { std::mem::transmute::<&'b T, &'static T>(r) }
}

static LIBRARY: LazyLock<FontLibrary> = LazyLock::new(|| FontLibrary {
    resolver: Mutex::new(Resolver::new()),
    fonts: Default::default(),
});

/// Access the [`FontLibrary`] singleton
pub fn library() -> &'static FontLibrary {
    &LIBRARY
}
