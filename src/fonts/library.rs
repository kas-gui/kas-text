// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font library

use super::{families, FaceRef, FontSelector};
use crate::conv::{to_u32, to_usize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard};
use thiserror::Error;
pub(crate) use ttf_parser::Face;

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
    #[error("no matching font found")]
    NotFound,
    #[error("font load error")]
    TtfParser(#[from] ttf_parser::FaceParsingError),
    #[error("FontLibrary::load_default is not first font load")]
    NotDefault,
}

/// Font face identifier
///
/// Identifies a loaded font face within the [`FontLibrary`] by index.
///
/// This type may be default-constructed to use the default font (whichever is
/// loaded to the [`FontLibrary`] first). If no font is loaded, attempting to
/// access a font with a (default-constructed) `FontId` will cause a panic in
/// the [`FontLibrary`] method used.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);
impl FontId {
    pub fn get(self) -> usize {
        to_usize(self.0)
    }
}

struct FaceStore<'a> {
    path: PathBuf,
    index: u32,
    face: Face<'a>,
    #[cfg(feature = "harfbuzz_rs")]
    harfbuzz: harfbuzz_rs::Shared<harfbuzz_rs::Face<'a>>,
}

impl<'a> FaceStore<'a> {
    /// Construct, given a file path, a reference to the loaded data and the face index
    ///
    /// The `path` is to be stored; its contents are already loaded in `data`.
    fn new(path: PathBuf, data: &'a [u8], index: u32) -> Result<Self, FontError> {
        Ok(FaceStore {
            path,
            index,
            face: Face::from_slice(data, index)?,
            #[cfg(feature = "harfbuzz_rs")]
            harfbuzz: harfbuzz_rs::Face::from_bytes(data, index).into(),
        })
    }
}

#[derive(Default)]
struct FontsData {
    fonts: Vec<Box<FaceStore<'static>>>,
    // These are vec-maps. Why? Because length should be short.
    sel_hash: Vec<(u64, FontId)>,
    path_hash: Vec<(u64, FontId)>,
}

impl FontsData {
    fn push(&mut self, font: Box<FaceStore<'static>>, sel_hash: u64, path_hash: u64) -> FontId {
        let id = FontId(to_u32(self.fonts.len()));
        self.fonts.push(font);
        self.sel_hash.push((sel_hash, id));
        self.path_hash.push((path_hash, id));
        id
    }
}

/// Library of loaded fonts
///
/// This is the type of the global singleton accessible via the [`fonts`]
/// function. Thread-safety is handled via internal locks.
pub struct FontLibrary {
    db: fontdb::Database,
    // Font files loaded into memory. Safety: we assume that existing entries
    // are never modified or removed (though the Vec is allowed to reallocate).
    // Note: using std::pin::Pin does not help since u8 impls Unpin.
    data: RwLock<HashMap<PathBuf, Box<[u8]>>>,
    // Fonts defined over the above data (see safety note).
    // Additional safety: fonts are boxed so that instances do not move
    fonts: RwLock<FontsData>,
}

// public API
impl FontLibrary {
    /// Get a font from its identifier
    ///
    /// Panics if `id` is not valid (required: `id.get() < self.num_fonts()`).
    pub fn get(&self, id: FontId) -> FaceRef {
        let fonts = self.fonts.read().unwrap();
        assert!(
            id.get() < fonts.fonts.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        let face: &Face<'static> = &fonts.fonts[id.get()].face;
        // Safety: elements of self.fonts are never dropped or modified
        let face = unsafe { extend_lifetime(face) };
        FaceRef(face)
    }

    /// Get a HarfBuzz font face
    #[cfg(feature = "harfbuzz_rs")]
    pub(crate) fn get_harfbuzz(
        &self,
        id: FontId,
    ) -> harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>> {
        let fonts = self.fonts.read().unwrap();
        assert!(
            id.get() < fonts.fonts.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        harfbuzz_rs::Font::new(fonts.fonts[id.get()].harfbuzz.clone())
    }

    /// Get the number of loaded font faces
    ///
    /// [`FontId`] values are indices assigned consecutively and are permanent.
    /// For any `x < self.num_fonts()`, `FontId(x)` is a valid font identifier.
    ///
    /// Font faces may be loaded on demand (by [`crate::Text::prepare`] but are
    /// never unloaded or adjusted, hence this value may increase but not decrease.
    pub fn num_fonts(&self) -> usize {
        let fonts = self.fonts.read().unwrap();
        fonts.fonts.len()
    }

    /// Access loaded font data
    pub fn font_data<'a>(&'a self) -> FontData<'a> {
        FontData {
            fonts: self.fonts.read().unwrap(),
            data: self.data.read().unwrap(),
        }
    }

    /// Load a default font
    ///
    /// This *must* be called before any other font-loading method.
    ///
    /// This should be at least once before attempting to query any font-derived
    /// properties (such as text dimensions).
    #[inline]
    pub fn load_default(&self) -> Result<FontId, Box<dyn std::error::Error>> {
        let id = self.load_font(&FontSelector::default())?;
        if id != FontId::default() {
            return Err(Box::new(FontError::NotDefault));
        }
        Ok(id)
    }

    /// Load a font
    ///
    /// This method uses two levels of caching to resolve existing
    /// fonts, thus is suitable for repeated usage.
    pub fn load_font(&self, selector: &FontSelector) -> Result<FontId, Box<dyn std::error::Error>> {
        let sel_hash = selector.hash();
        let fonts = self.fonts.read().unwrap();
        for (h, id) in &fonts.sel_hash {
            if *h == sel_hash {
                return Ok(*id);
            }
        }
        drop(fonts);

        let (source, index) = selector.select(&self.db).ok_or(FontError::NotFound)?;
        match source {
            fontdb::Source::Binary(_) => unimplemented!(),
            fontdb::Source::File(path) => self.load_path(path, index, sel_hash),
        }
    }

    /// Load a font by path
    ///
    /// In case the `(path, index)` combination has already been loaded, the
    /// existing font object's [`FontId`] will be returned.
    ///
    /// The `index` is used to select fonts from a font-collection. If the font
    /// is not a collection, use `0`.
    ///
    /// `sel_hash` is the hash of the [`FontSelector`] used; if this is not
    /// used, pass 0.
    pub fn load_path(
        &self,
        path: &Path,
        index: u32,
        sel_hash: u64,
    ) -> Result<FontId, Box<dyn std::error::Error>> {
        let path_hash = self.hash_path(path, index);

        // 1st lock: early exit if we already have this font
        let fonts = self.fonts.read().unwrap();
        for (h, id) in &fonts.path_hash {
            if *h == path_hash {
                return Ok(*id);
            }
        }
        drop(fonts);

        // 2nd lock: load and store file data / get reference
        let mut data = self.data.write().unwrap();
        let slice = if let Some(entry) = data.get(path) {
            let slice: &[u8] = &entry[..];
            // Safety: slice is in self.data and will not be dropped or modified
            unsafe { extend_lifetime(slice) }
        } else {
            let v = std::fs::read(path)?.into_boxed_slice();
            let slice = &data.entry(path.to_owned()).or_insert(v)[..];
            // Safety: as above
            unsafe { extend_lifetime(slice) }
        };
        drop(data);

        // 3rd lock: insert into font list
        let store = FaceStore::new(path.to_owned(), slice, index)?;
        let mut fonts = self.fonts.write().unwrap();
        let id = fonts.push(Box::new(store), sel_hash, path_hash);

        log::debug!("Loaded: {:?} = {},{}", id, path.display(), index);
        Ok(id)
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

/// Provides access to font data
///
/// Each valid [`FontId`] is an index to a loaded font face. Since fonts are
/// never unloaded or replaced, [`FontId::get`] is a valid index into these
/// arrays for any valid [`FontId`].
pub struct FontData<'a> {
    fonts: RwLockReadGuard<'a, FontsData>,
    data: RwLockReadGuard<'a, HashMap<PathBuf, Box<[u8]>>>,
}
impl<'a> FontData<'a> {
    /// Number of available fonts
    pub fn len(&self) -> usize {
        self.fonts.fonts.len()
    }

    /// Access font path and face index
    ///
    /// Note: use [`FontData::get_data`] to access the font file data, already
    /// loaded into memory.
    pub fn get_path(&self, index: usize) -> (&Path, u32) {
        let f = &self.fonts.fonts[index];
        (&f.path, f.index)
    }

    /// Access font data and face index
    pub fn get_data(&self, index: usize) -> (&'static [u8], u32) {
        let f = &self.fonts.fonts[index];
        let data = self.data.get(&f.path).unwrap();
        // Safety: data is in FontLibrary::data and will not be dropped or modified
        let data = unsafe { extend_lifetime(data) };
        (data, f.index)
    }
}

pub(crate) unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
    std::mem::transmute::<&'b T, &'static T>(r)
}

// internals
impl FontLibrary {
    // Private because: safety depends on instance(s) never being destructed.
    fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        families::set_defaults(&mut db);

        FontLibrary {
            db,
            data: Default::default(),
            fonts: Default::default(),
        }
    }
}

lazy_static::lazy_static! {
    static ref LIBRARY: FontLibrary = FontLibrary::new();
}

/// Access the [`FontLibrary`] singleton
pub fn fonts() -> &'static FontLibrary {
    &*LIBRARY
}