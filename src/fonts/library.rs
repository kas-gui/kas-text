// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

use super::{Face, FontSelector, Resolver};
use crate::conv::{to_u32, to_usize};
use fontique::{QueryStatus, Script, Synthesis};
use std::collections::hash_map::{Entry, HashMap};
use std::sync::{LazyLock, Mutex, MutexGuard};
use thiserror::Error;

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
impl FontId {
    /// Get as `usize`
    pub(crate) fn get(self) -> usize {
        to_usize(self.0)
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
    ) -> Option<FaceId> {
        // TODO: `face.glyph_index` is a bit slow to use like this where several
        // faces may return no result before we find a match. Caching results
        // in a HashMap helps. Perhaps better would be to (somehow) determine
        // the script/language in use and check whether the font face supports
        // that, perhaps also checking it has shaping support.
        let font = &mut self.fonts[font_id.get()];
        debug_assert_eq!(font.id, font_id);

        if let Some(face_id) = preferred_face
            && font.faces.contains(&face_id)
        {
            let face = &self.faces[face_id.get()];
            // TODO(opt): should we cache this lookup?
            if face.glyph_index(c).is_some() {
                return Some(face_id);
            }
        }

        match font.glyph_map.entry(c) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let mut id: Option<FaceId> = None;
                for face_id in font.faces.iter() {
                    let face = &self.faces[face_id.get()];
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
        }
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
    pub(crate) fn first_face_for(&self, font_id: FontId) -> FaceId {
        let fonts = self.fonts.lock().unwrap();
        let font = &fonts.fonts[font_id.get()];
        debug_assert_eq!(font.id, font_id);
        *font.faces.first().unwrap()
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
    ) -> Option<FaceId> {
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
        text: &str,
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

        let mut unmatched_text = String::new();
        let mut filter_chars = |face: &Face| -> QueryStatus {
            let mut unmatched = String::new();
            let source = if unmatched_text.is_empty() {
                text
            } else {
                &unmatched_text
            };
            for c in source.chars() {
                if face.glyph_index(c).is_none() && !unmatched.contains(c) {
                    unmatched.push(c);
                }
            }

            if unmatched.is_empty() {
                QueryStatus::Stop
            } else {
                unmatched_text = unmatched;
                QueryStatus::Continue
            }
        };

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
                    if face.is_query_font(qf) {
                        faces.push(id);
                        return filter_chars(face);
                    }
                }
            }

            match Face::new(qf) {
                Ok(face) => {
                    if let Some(name) = face.name_full() {
                        if *face.synthesis() == Synthesis::default() {
                            log::debug!("Loaded font: {name}");
                        } else {
                            log::debug!("Loaded font: {name} with {:?}", face.synthesis());
                        }
                    }
                    let status = filter_chars(&face);
                    let id = fonts.push_face(Box::new(face), source_hash);
                    faces.push(id);
                    status
                }
                Err(err) => {
                    log::error!("Failed to load font: {err}");
                    QueryStatus::Continue
                }
            }
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
        unsafe { super::extend_lifetime(faces) }
    }
}

static LIBRARY: LazyLock<FontLibrary> = LazyLock::new(|| FontLibrary {
    resolver: Mutex::new(Resolver::new()),
    fonts: Default::default(),
});

/// Access the [`FontLibrary`] singleton
pub fn library() -> &'static FontLibrary {
    &LIBRARY
}
