// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

#![allow(clippy::len_without_is_empty)]

use super::{FaceRef, FontSelector, Resolver};
use crate::conv::{to_u32, to_usize};
use fontique::{Blob, QueryStatus, Script, Synthesis};
use std::collections::hash_map::{Entry, HashMap};
use std::sync::{LazyLock, Mutex, MutexGuard, RwLock};
use thiserror::Error;
pub(crate) use ttf_parser::Face;

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
    #[error("font load error")]
    TtfParser(#[from] ttf_parser::FaceParsingError),
    #[cfg(feature = "ab_glyph")]
    #[error("font load error")]
    AbGlyph(#[from] ab_glyph::InvalidFont),
    #[error("font load error")]
    Swash,
}

/// Bad [`FontId`] or no font loaded
///
/// This error should be impossible to observe, but exists to avoid panic in
/// lower level methods.
#[derive(Error, Debug)]
#[error("invalid FontId")]
pub struct InvalidFontId;

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
pub struct FaceId(pub(crate) u32);
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

/// Font face identifier
///
/// Identifies a font list within the [`FontLibrary`] by index.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontId(u32);
impl FontId {
    /// Get as `usize`
    pub fn get(self) -> usize {
        to_usize(self.0)
    }
}

/// A store of data for a font face, supporting various backends
pub struct FaceStore {
    blob: Blob<u8>,
    index: u32,
    face: Face<'static>,
    #[cfg(feature = "rustybuzz")]
    rustybuzz: rustybuzz::Face<'static>,
    #[cfg(feature = "ab_glyph")]
    ab_glyph: ab_glyph::FontRef<'static>,
    swash: (u32, swash::CacheKey), // (offset, key)
    synthesis: Synthesis,
}

impl FaceStore {
    /// Construct, given a data blob, face index and synthesis settings
    fn new(blob: Blob<u8>, index: u32, synthesis: Synthesis) -> Result<Self, FontError> {
        // Safety: this is a private fn used to construct a FaceStore instance
        // to be stored in FontLibrary which is never deallocated. This
        // FaceStore holds onto `blob`, so `data` is valid until program exit.
        let data = unsafe { extend_lifetime(blob.data()) };

        let face = Face::parse(data, index)?;

        Ok(FaceStore {
            blob,
            index,
            #[cfg(feature = "rustybuzz")]
            rustybuzz: {
                use {rustybuzz::Variation, ttf_parser::Tag};

                let len = synthesis.variation_settings().len();
                debug_assert!(len <= 3);
                let mut vars = [Variation {
                    tag: Tag(0),
                    value: 0.0,
                }; 3];
                for (r, (tag, value)) in vars.iter_mut().zip(synthesis.variation_settings()) {
                    r.tag = Tag::from_bytes(&tag.to_be_bytes());
                    r.value = *value;
                }

                let mut rustybuzz = rustybuzz::Face::from_face(face.clone());
                rustybuzz.set_variations(&vars[0..len]);
                rustybuzz
            },
            face,
            #[cfg(feature = "ab_glyph")]
            ab_glyph: {
                let mut font = ab_glyph::FontRef::try_from_slice_and_index(data, index)?;
                for (tag, value) in synthesis.variation_settings() {
                    ab_glyph::VariableFont::set_variation(&mut font, &tag.to_be_bytes(), *value);
                }
                font
            },
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
    pub fn read_name(&self, id: u16) -> Option<String> {
        use ttf_parser::PlatformId;
        let name = self.face.names().get(id)?;

        // NOTE: we ignore name.encoding_id which should be used to select a
        // Unicode / ASCII code page encoding.
        match name.platform_id {
            PlatformId::Macintosh => Some(String::from_utf8_lossy(name.name).to_string()),
            // TODO(std lib) use String::from_utf16be_lossy:
            PlatformId::Unicode | PlatformId::Windows => {
                let name: Vec<u16> = name
                    .name
                    .as_chunks()
                    .0
                    .iter()
                    .map(|chunk| u16::from_be_bytes(*chunk))
                    .collect();
                Some(String::from_utf16_lossy(&name))
            }
            _ => None,
        }
    }

    /// Get the font family name
    #[inline]
    pub fn name_family(&self) -> Option<String> {
        self.read_name(1)
    }

    /// Get the font sub-family (i.e. style) name
    #[inline]
    pub fn name_subfamily(&self) -> Option<String> {
        self.read_name(2)
    }

    /// Get the full font name
    #[inline]
    pub fn name_full(&self) -> Option<String> {
        self.read_name(4)
    }

    /// Access the [`Face`] object
    pub fn face(&self) -> &Face<'static> {
        &self.face
    }

    /// Access a [`FaceRef`] object
    pub fn face_ref(&self) -> FaceRef<'_> {
        FaceRef(&self.face)
    }

    /// Access the [`rustybuzz`] object
    #[cfg(feature = "rustybuzz")]
    pub fn rustybuzz(&self) -> &rustybuzz::Face<'static> {
        &self.rustybuzz
    }

    /// Access the [`ab_glyph`] object
    #[cfg(feature = "ab_glyph")]
    pub fn ab_glyph(&self) -> &ab_glyph::FontRef<'static> {
        &self.ab_glyph
    }

    /// Get a swash `FontRef`
    pub fn swash(&self) -> swash::FontRef<'_> {
        swash::FontRef {
            data: self.face.raw_face().data,
            offset: self.swash.0,
            key: self.swash.1,
        }
    }

    /// Get font variation settings
    pub fn synthesis(&self) -> &Synthesis {
        &self.synthesis
    }
}

#[derive(Default)]
struct FaceList {
    // Safety: unsafe code depends on entries never moving (hence the otherwise
    // redundant use of Box). See e.g. FontLibrary::get_face().
    #[allow(clippy::vec_box)]
    faces: Vec<Box<FaceStore>>,
    // These are vec-maps. Why? Because length should be short.
    source_hash: Vec<(u64, FaceId)>,
}

impl FaceList {
    fn push(&mut self, face: Box<FaceStore>, source_hash: u64) -> FaceId {
        let id = FaceId(to_u32(self.faces.len()));
        self.faces.push(face);
        self.source_hash.push((source_hash, id));
        id
    }
}

#[derive(Default)]
struct FontList {
    // A "font" is a list of faces (primary + fallbacks); we cache glyph-lookups per char
    fonts: Vec<(FontId, Vec<FaceId>, HashMap<char, Option<FaceId>>)>,
    sel_hash: Vec<(u64, FontId)>,
}

impl FontList {
    fn push(&mut self, list: Vec<FaceId>, sel_hash: u64) -> FontId {
        let id = FontId(to_u32(self.fonts.len()));
        self.fonts.push((id, list, HashMap::new()));
        self.sel_hash.push((sel_hash, id));
        id
    }
}

/// Library of loaded fonts
///
/// This is the type of the global singleton accessible via the [`library()`]
/// function. Thread-safety is handled via internal locks.
pub struct FontLibrary {
    resolver: Mutex<Resolver>,
    faces: RwLock<FaceList>,
    fonts: RwLock<FontList>,
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
    pub fn first_face_for(&self, font_id: FontId) -> Result<FaceId, InvalidFontId> {
        let fonts = self.fonts.read().unwrap();
        for (id, list, _) in &fonts.fonts {
            if *id == font_id {
                return Ok(*list.first().unwrap());
            }
        }
        Err(InvalidFontId)
    }

    /// Get the first face for a font
    ///
    /// This is a wrapper around [`FontLibrary::first_face_for`] and [`FontLibrary::get_face`].
    #[inline]
    pub fn get_first_face(&self, font_id: FontId) -> Result<FaceRef<'_>, InvalidFontId> {
        let face_id = self.first_face_for(font_id)?;
        Ok(self.get_face(face_id))
    }

    /// Check whether a [`FaceId`] is part of a [`FontId`]
    pub fn contains_face(&self, font_id: FontId, face_id: FaceId) -> Result<bool, InvalidFontId> {
        let fonts = self.fonts.read().unwrap();
        for (id, list, _) in &fonts.fonts {
            if *id == font_id {
                return Ok(list.contains(&face_id));
            }
        }
        Err(InvalidFontId)
    }

    /// Resolve the font face for a character
    ///
    /// If `last_face_id` is a face used by `font_id` and this face covers `c`,
    /// then return `last_face_id`. (This is to avoid changing the font face
    /// unnecessarily, such as when encountering a space amid Arabic text.)
    ///
    /// Otherwise, return the first face of `font_id` which covers `c`.
    pub fn face_for_char(
        &self,
        font_id: FontId,
        last_face_id: Option<FaceId>,
        c: char,
    ) -> Result<Option<FaceId>, InvalidFontId> {
        // TODO: `face.glyph_index` is a bit slow to use like this where several
        // faces may return no result before we find a match. Caching results
        // in a HashMap helps. Perhaps better would be to (somehow) determine
        // the script/language in use and check whether the font face supports
        // that, perhaps also checking it has shaping support.
        let mut fonts = self.fonts.write().unwrap();
        let font = fonts
            .fonts
            .iter_mut()
            .find(|item| item.0 == font_id)
            .ok_or(InvalidFontId)?;

        let faces = self.faces.read().unwrap();

        if let Some(face_id) = last_face_id
            && font.1.contains(&face_id)
        {
            let face = &faces.faces[face_id.get()];
            // TODO(opt): should we cache this lookup?
            if face.face.glyph_index(c).is_some() {
                return Ok(Some(face_id));
            }
        }

        Ok(match font.2.entry(c) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let mut id: Option<FaceId> = None;
                for face_id in font.1.iter() {
                    let face = &faces.faces[face_id.get()];
                    if face.face.glyph_index(c).is_some() {
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

    /// Select a font
    ///
    /// This method uses internal caching to enable fast look-ups of existing
    /// (loaded) fonts. Resolving new fonts may be slower.
    pub fn select_font(
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

        let fonts = self.fonts.read().unwrap();
        for (h, id) in &fonts.sel_hash {
            if *h == sel_hash {
                return Ok(*id);
            }
        }
        drop(fonts);

        let mut faces = Vec::new();
        let mut families = Vec::new();
        let mut resolver = self.resolver.lock().unwrap();
        let mut face_list = self.faces.write().unwrap();

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

            for (h, id) in face_list.source_hash.iter().cloned() {
                if h == source_hash {
                    let face = &face_list.faces[id.get()];
                    if face.blob.id() == qf.blob.id()
                        && face.index == qf.index
                        && face.synthesis == qf.synthesis
                    {
                        faces.push(id);
                        return QueryStatus::Continue;
                    }
                }
            }

            match FaceStore::new(qf.blob.clone(), qf.index, qf.synthesis) {
                Ok(store) => {
                    let id = face_list.push(Box::new(store), source_hash);
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
                log::debug!("match: {name}");
            }
        }

        if faces.is_empty() {
            return Err(NoFontMatch);
        }
        let font = self.fonts.write().unwrap().push(faces, sel_hash);
        Ok(font)
    }
}

/// Face management
impl FontLibrary {
    /// Get a font face from its identifier
    ///
    /// Panics if `id` is not valid (required: `id.get() < self.num_faces()`).
    pub fn get_face(&self, id: FaceId) -> FaceRef<'static> {
        self.get_face_store(id).face_ref()
    }

    /// Get access to the [`FaceStore`]
    ///
    /// Panics if `id` is not valid (required: `id.get() < self.num_faces()`).
    pub fn get_face_store(&self, id: FaceId) -> &'static FaceStore {
        let faces = self.faces.read().unwrap();
        assert!(id.get() < faces.faces.len(), "FontLibrary: invalid {id:?}!",);
        let faces: &FaceStore = &faces.faces[id.get()];
        // Safety: elements of self.faces are never dropped or modified
        unsafe { extend_lifetime(faces) }
    }
}

pub(crate) unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
    unsafe { std::mem::transmute::<&'b T, &'static T>(r) }
}

static LIBRARY: LazyLock<FontLibrary> = LazyLock::new(|| FontLibrary {
    resolver: Mutex::new(Resolver::new()),
    faces: Default::default(),
    fonts: Default::default(),
});

/// Access the [`FontLibrary`] singleton
pub fn library() -> &'static FontLibrary {
    &LIBRARY
}
