// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

#![allow(clippy::len_without_is_empty)]

use super::{FaceRef, FontSelector, Resolver};
use crate::conv::{to_u32, to_usize};
use fontdb::Database;
use log::warn;
use std::collections::hash_map::{Entry, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, OnceLock, RwLock, RwLockReadGuard};
use thiserror::Error;
pub(crate) use ttf_parser::Face;

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
    #[error("font DB not yet initialized")]
    NotReady,
    #[error("no matching font found")]
    NotFound,
    #[error("unsupported: loading fonts from data source")]
    UnsupportedFontDataSource,
    #[error("font load error")]
    TtfParser(#[from] ttf_parser::FaceParsingError),
    #[cfg(feature = "ab_glyph")]
    #[error("font load error")]
    AbGlyph(#[from] ab_glyph::InvalidFont),
    #[cfg(feature = "fontdue")]
    #[error("font load error")]
    StrError(&'static str),
}

#[cfg(feature = "fontdue")]
impl From<&'static str> for FontError {
    fn from(msg: &'static str) -> Self {
        FontError::StrError(msg)
    }
}

/// Bad [`FontId`] or no font loaded
///
/// Since [`FontId`] supports default construction, this error can occur when
/// text preparation is run before a default font is loaded.
///
/// It is safe to ignore this error, though (successful) text preparation will
/// still be required before display.
#[derive(Error, Debug)]
#[error("invalid FontId")]
pub struct InvalidFontId;

/// Font face identifier
///
/// Identifies a loaded font face within the [`FontLibrary`] by index.
///
/// Internally this uses a numeric identifier, which is always less than
/// [`FontLibrary::num_faces`], assuming that at least one font has been loaded.
/// [`FontLibrary::face_data`] may be used to retrieve the matching font.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FaceId(pub u32);
impl FaceId {
    /// Get as `usize`
    pub fn get(self) -> usize {
        to_usize(self.0)
    }
}

/// Font face identifier
///
/// Identifies a font list within the [`FontLibrary`] by index.
///
/// This type may be default-constructed to use the default font (whichever is
/// loaded to the [`FontLibrary`] first). If no font is loaded, attempting to
/// access a font with a (default-constructed) `FontId` will cause a panic in
/// the [`FontLibrary`] method used.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(u32);
impl FontId {
    /// Get as `usize`
    pub fn get(self) -> usize {
        to_usize(self.0)
    }
}

pub(crate) struct FaceStore<'a> {
    path: PathBuf,
    index: u32,
    pub(crate) face: Face<'a>,
    #[cfg(feature = "harfbuzz_rs")]
    pub(crate) harfbuzz: harfbuzz_rs::Shared<harfbuzz_rs::Face<'a>>,
    #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
    pub(crate) rustybuzz: rustybuzz::Face<'a>,
    #[cfg(feature = "ab_glyph")]
    pub(crate) ab_glyph: ab_glyph::FontRef<'a>,
    #[cfg(feature = "fontdue")]
    pub(crate) fontdue: fontdue::Font,
}

impl<'a> FaceStore<'a> {
    /// Construct, given a file path, a reference to the loaded data and the face index
    ///
    /// The `path` is to be stored; its contents are already loaded in `data`.
    fn new(path: PathBuf, data: &'a [u8], index: u32) -> Result<Self, FontError> {
        let face = Face::parse(data, index)?;
        #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
        let rustybuzz = rustybuzz::Face::from_face(face.clone());
        Ok(FaceStore {
            path,
            index,
            face,
            #[cfg(feature = "harfbuzz_rs")]
            harfbuzz: harfbuzz_rs::Face::from_bytes(data, index).into(),
            #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
            rustybuzz,
            #[cfg(feature = "ab_glyph")]
            ab_glyph: ab_glyph::FontRef::try_from_slice_and_index(data, index)?,
            #[cfg(feature = "fontdue")]
            fontdue: {
                let settings = fontdue::FontSettings {
                    collection_index: index,
                    scale: 40.0, // TODO: max expected font size in dpem
                    load_substitutions: true,
                };
                fontdue::Font::from_bytes(data, settings)?
            },
        })
    }
}

#[derive(Default)]
struct FaceList {
    // Safety: unsafe code depends on entries never moving (hence the otherwise
    // redundant use of Box). See e.g. FontLibrary::get_face().
    #[allow(clippy::vec_box)]
    faces: Vec<Box<FaceStore<'static>>>,
    // These are vec-maps. Why? Because length should be short.
    path_hash: Vec<(u64, FaceId)>,
}

impl FaceList {
    fn push(&mut self, face: Box<FaceStore<'static>>, path_hash: u64) -> FaceId {
        let id = FaceId(to_u32(self.faces.len()));
        self.faces.push(face);
        self.path_hash.push((path_hash, id));
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
    resolver: RwLock<Resolver>,
    // Font files loaded into memory. Safety: we assume that existing entries
    // are never modified or removed.
    // Note: using std::pin::Pin does not help since u8 impls Unpin.
    data: RwLock<HashMap<PathBuf, Box<[u8]>>>,
    // Font faces defined over the above data (see safety note).
    // Additional safety: `FaceStore` instances are boxed and so cannot move
    faces: RwLock<FaceList>,
    fonts: RwLock<FontList>,
}

/// Font management
impl FontLibrary {
    /// Adjust the font resolver
    ///
    /// This method may only be called before [`FontLibrary::init`].
    /// If called afterwards this will just return `None`.
    pub fn adjust_resolver<F: FnOnce(&mut Resolver) -> T, T>(&self, f: F) -> Option<T> {
        if DB.get().is_some() {
            warn!("unable to update resolver after kas_text::fonts::library().init()");
            return None;
        }

        Some(f(&mut self.resolver.write().unwrap()))
    }

    /// Initialize
    ///
    /// This method constructs the [`fontdb::Database`], loads fonts
    /// and resolves the default font (i.e. `FontId(0)`).
    ///
    /// Either this method or [`FontLibrary::init_custom`] *must* be called
    /// before any other font selection method, and before
    /// querying any font-derived properties (such as text dimensions).
    /// It is safe (but ineffective) to call multiple times.
    #[inline]
    pub fn init(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.init_custom(|db| db.load_system_fonts())
    }

    /// Initialize with custom fonts
    ///
    /// This method may be called instead of [`FontLibrary::init`] to customize
    /// initial font loading: the `loader` method must load all required fonts
    /// (system and/or custom fonts).
    ///
    /// Calling this method repeatedly is safe but ineffective.
    /// Loading fonts after initialization is not currently supported.
    pub fn init_custom(
        &self,
        loader: impl FnOnce(&mut Database),
    ) -> Result<(), Box<dyn std::error::Error>> {
        if DB.get().is_some() {
            return Ok(());
        }

        let mut db = Arc::new(Database::new());
        let dbm = Arc::make_mut(&mut db);
        loader(dbm);

        self.resolver.write().unwrap().init(dbm);

        if let Ok(()) = DB.set(db) {
            let id = self.select_font(&FontSelector::default())?;
            debug_assert!(id == FontId::default());
        }

        Ok(())
    }

    /// Get a reference to the font resolver
    pub fn resolver(&self) -> RwLockReadGuard<Resolver> {
        self.resolver.read().unwrap()
    }

    /// Get the first face for a font
    ///
    /// Assumes that `font_id` is valid; if not the method will panic.
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
    pub fn get_first_face(&self, font_id: FontId) -> Result<FaceRef, InvalidFontId> {
        let face_id = self.first_face_for(font_id)?;
        Ok(self.get_face(face_id))
    }

    /// Resolve the font face for a character
    ///
    /// If `last_face_id` is a face used by `font_id` and this face covers `c`,
    /// then return `last_face_id`. (This is to avoid changing the font face
    /// unnecessarily, such as when encountering a space amid Arabic text.)
    ///
    /// Otherwise, return the first face of `font_id` which covers `c`.
    ///
    /// Otherwise (if no face covers `c`) return `None`.
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

        if let Some(face_id) = last_face_id {
            if font.1.contains(&face_id) {
                let face = &faces.faces[face_id.get()];
                // TODO(opt): should we cache this lookup?
                if face.face.glyph_index(c).is_some() {
                    return Ok(Some(face_id));
                }
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
                entry.insert(id);
                id
            }
        })
    }

    /// Resolve the font face for a character
    ///
    /// If `last_face_id` is a face used by `font_id` and this face covers `c`,
    /// then return `last_face_id`. (This is to avoid changing the font face
    /// unnecessarily, such as when encountering a space amid Arabic text.)
    ///
    /// Otherwise, return the first face of `font_id` which covers `c`.
    ///
    /// Otherwise (if no face covers `c`) return the first face for `font_id`.
    #[inline]
    pub fn face_for_char_or_first(
        &self,
        font_id: FontId,
        last_face_id: Option<FaceId>,
        c: char,
    ) -> Result<FaceId, InvalidFontId> {
        match self.face_for_char(font_id, last_face_id, c) {
            Ok(Some(face_id)) => Ok(face_id),
            Ok(None) => self.first_face_for(font_id),
            Err(e) => Err(e),
        }
    }

    /// Select a font
    ///
    /// This method uses internal caching to enable fast look-ups of existing
    /// (loaded) fonts. Resolving new fonts may be slower.
    pub fn select_font(
        &self,
        selector: &FontSelector,
    ) -> Result<FontId, Box<dyn std::error::Error>> {
        let sel_hash = calculate_hash(&selector);
        let fonts = self.fonts.read().unwrap();
        for (h, id) in &fonts.sel_hash {
            if *h == sel_hash {
                return Ok(*id);
            }
        }
        drop(fonts);

        let mut faces = Vec::new();
        let resolver = self.resolver.read().unwrap();
        let Some(db) = DB.get() else {
            return Err(Box::new(FontError::NotReady));
        };

        selector.select(&resolver, db, |face_info| {
            match &face_info.source {
                fontdb::Source::File(path) => {
                    let path_hash = self.hash_path(path, face_info.index);

                    // 1st lock: early exit if we already have this font
                    let lock = self.faces.read().unwrap();
                    for (h, id) in lock.path_hash.iter().cloned() {
                        if h == path_hash {
                            faces.push(id);
                            return Ok(());
                        }
                    }
                    drop(lock);

                    // 2nd lock: load and store file data / get reference
                    let mut lock = self.data.write().unwrap();
                    let slice = if let Some(entry) = lock.get(path) {
                        &entry[..]
                    } else {
                        let v = std::fs::read(path)?.into_boxed_slice();
                        &lock.entry(path.to_owned()).or_insert(v)[..]
                    };
                    // Safety: slice is in self.data and will not be dropped or modified
                    let slice = unsafe { extend_lifetime(slice) };
                    drop(lock);

                    // 3rd lock: insert into font list
                    let store = FaceStore::new(path.to_owned(), slice, face_info.index)?;
                    let mut lock = self.faces.write().unwrap();
                    let id = lock.push(Box::new(store), path_hash);

                    log::debug!("Loaded: {:?} = {},{}", id, path.display(), face_info.index);
                    faces.push(id);
                    Ok(())
                }
                _ => Err(Box::new(FontError::UnsupportedFontDataSource)),
            }
        })?;

        if faces.is_empty() {
            return Err(Box::new(FontError::NotFound));
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
    pub fn get_face(&self, id: FaceId) -> FaceRef {
        let faces = self.faces.read().unwrap();
        assert!(
            id.get() < faces.faces.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        let face: &Face<'static> = &faces.faces[id.get()].face;
        // Safety: elements of self.faces are never dropped or modified
        let face = unsafe { extend_lifetime(face) };
        FaceRef(face)
    }

    /// Get access to the [`FaceStore`]
    #[cfg(any(feature = "ab_glyph", feature = "fontdue"))]
    pub(crate) fn get_face_store(&self, id: FaceId) -> &'static FaceStore {
        let faces = self.faces.read().unwrap();
        assert!(
            id.get() < faces.faces.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        let faces: &FaceStore = &faces.faces[id.get()];
        // Safety: elements of self.faces are never dropped or modified
        unsafe { extend_lifetime(faces) }
    }

    /// Get a HarfBuzz font face
    #[cfg(feature = "harfbuzz_rs")]
    pub(crate) fn get_harfbuzz(
        &self,
        id: FaceId,
    ) -> harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>> {
        let faces = self.faces.read().unwrap();
        assert!(
            id.get() < faces.faces.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        harfbuzz_rs::Font::new(faces.faces[id.get()].harfbuzz.clone())
    }

    /// Get a Rustybuzz font face
    #[cfg(all(not(feature = "harfbuzz_rs"), feature = "rustybuzz"))]
    pub(crate) fn get_rustybuzz(&self, id: FaceId) -> &rustybuzz::Face<'static> {
        let faces = self.faces.read().unwrap();
        assert!(
            id.get() < faces.faces.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        let face: &rustybuzz::Face<'static> = &faces.faces[id.get()].rustybuzz;
        // Safety: elements of self.fonts are never dropped or modified
        unsafe { extend_lifetime(face) }
    }

    /// Get the number of loaded font faces
    ///
    /// [`FaceId`] values are indices assigned consecutively and are permanent.
    /// For any `x < self.num_faces()`, `FaceId(x)` is a valid font face identifier.
    ///
    /// This value may increase as fonts may be loaded on demand. It will not
    /// decrease since fonts are never unloaded during program execution.
    pub fn num_faces(&self) -> usize {
        let faces = self.faces.read().unwrap();
        faces.faces.len()
    }

    fn hash_path(&self, path: &Path, index: u32) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.write_u32(index);
        hasher.finish()
    }
}

pub(crate) unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
    std::mem::transmute::<&'b T, &'static T>(r)
}

static LIBRARY: LazyLock<FontLibrary> = LazyLock::new(|| FontLibrary {
    resolver: RwLock::new(Resolver::new()),
    data: Default::default(),
    faces: Default::default(),
    fonts: Default::default(),
});

/// Access the [`FontLibrary`] singleton
pub fn library() -> &'static FontLibrary {
    &LIBRARY
}

static DB: OnceLock<Arc<Database>> = OnceLock::new();

/// Access the font database
///
/// Returns `None` when called before [`FontLibrary::init`].
pub fn db() -> Option<&'static Database> {
    DB.get().map(|arc| &**arc)
}

/// Get owning access the font database
///
/// Returns `None` when called before [`FontLibrary::init`].
pub fn clone_db() -> Option<Arc<Database>> {
    DB.get().cloned()
}

fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
