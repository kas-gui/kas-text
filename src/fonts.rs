// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — fonts

use ab_glyph::{FontRef, InvalidFont, PxScale, PxScaleFont};
use font_kit::error::SelectionError;
use font_kit::source::SystemSource;
use font_kit::{family_name::FamilyName, handle::Handle, properties::Properties};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use thiserror::Error;

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
    #[error("invalid font data")]
    Invalid,
    #[error("FontLibrary::load_default is not first font load")]
    NotDefault,
}

impl From<InvalidFont> for FontError {
    fn from(_: InvalidFont) -> Self {
        FontError::Invalid
    }
}

/// Font identifier
///
/// This type may be default-constructed to use the default font (whichever is
/// loaded to the [`FontLibrary`] first). If no font is loaded, attempting to
/// access a font with a (default-constructed) `FontId` will cause a panic in
/// the [`FontLibrary`] method used.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(u32);

impl FontId {
    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// Handle to a loaded font
#[derive(Copy, Clone, Debug)]
pub struct Font {
    // Note: FontRef itself is too large to clone cheaply, so use a reference to it
    font: &'static FontRef<'static>,
}

impl Font {
    /// Get a scale
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
    pub(crate) fn font_scale(self, dpem: f32) -> f32 {
        use ab_glyph::Font;
        let upem = self.font.units_per_em().unwrap();
        dpem / upem * self.font.height_unscaled()
    }

    /// Get a scaled font
    ///
    /// Units: the same as [`PxScale] — pixels per line height.
    #[inline]
    pub(crate) fn scaled(self, scale: f32) -> PxScaleFont<&'static FontRef<'static>> {
        use ab_glyph::Font;
        self.font.into_scaled(PxScale::from(scale))
    }

    /// Get the height of a line of text in pixels
    ///
    /// Input: `dpem` is pixels/em (see [`Font::font_scale`]).
    #[inline]
    pub fn line_height(self, dpem: f32) -> f32 {
        // Due to the way ab-glyph works, this is font_scale
        // (We reserve the right to change this later, hence a separate method.)
        self.font_scale(dpem)
    }
}

struct FontStore<'a> {
    ab_glyph: FontRef<'a>,
    #[cfg(feature = "harfbuzz_rs")]
    harfbuzz: harfbuzz_rs::Shared<harfbuzz_rs::Face<'a>>,
}

impl<'a> FontStore<'a> {
    fn new(data: &'a [u8], index: u32) -> Result<Self, FontError> {
        Ok(FontStore {
            ab_glyph: FontRef::try_from_slice_and_index(data, index)?,
            #[cfg(feature = "harfbuzz_rs")]
            harfbuzz: harfbuzz_rs::Face::from_bytes(data, index).into(),
        })
    }
}

/// Library of loaded fonts
// Note: std::pin::Pin does not help us here: Unpin is implemented for both u8
// and FontRef, and we never give the user a `&mut FontLibrary` anyway.
pub struct FontLibrary {
    // Font files loaded into memory. Safety: we assume this is never freed
    // and that the `u8` slices are never moved or modified.
    data: RwLock<HashMap<PathBuf, Box<[u8]>>>,
    // Fonts defined over the above data (see safety note).
    // Additional safety: boxed so that instances do not move
    fonts: RwLock<(Vec<Box<FontStore<'static>>>, Vec<(u64, FontId)>)>,
}

// public API
impl FontLibrary {
    /// Get a font from its identifier
    pub fn get(&self, id: FontId) -> Font {
        let fonts = self.fonts.read().unwrap();
        assert!(id.get() < fonts.0.len(), "FontLibrary: invalid {:?}!", id);
        let font: &FontRef<'static> = &fonts.0[id.get()].ab_glyph;
        // Safety: elements of self.fonts are never dropped or modified
        let font = unsafe { extend_lifetime(font) };
        Font { font }
    }

    /// Get a HarfBuzz font face
    ///
    /// `font_scale` should be "point size × screen DPI / 72" (units: px/em).
    #[cfg(feature = "harfbuzz_rs")]
    pub fn get_harfbuzz(&self, id: FontId) -> harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>> {
        let fonts = self.fonts.read().unwrap();
        assert!(id.get() < fonts.0.len(), "FontLibrary: invalid {:?}!", id);
        harfbuzz_rs::Font::new(fonts.0[id.get()].harfbuzz.clone())
    }

    /// Get a list of all fonts
    ///
    /// E.g. `glyph_brush` needs this
    pub fn ab_glyph_fonts_vec(&self) -> Vec<&'static FontRef<'static>> {
        let fonts = self.fonts.read().unwrap();
        // Safety: each font is boxed so that its address never changes and
        // fonts are never modified or freed before program exit.
        fonts
            .0
            .iter()
            .map(|font| unsafe { extend_lifetime(&font.ab_glyph) })
            .collect()
    }

    /// Load a default font
    ///
    /// This should be at least once before attempting to query any font-derived
    /// properties (such as text dimensions).
    #[inline]
    pub fn load_default(&self) -> Result<FontId, Box<dyn std::error::Error>> {
        let id = self.load_font(FontSelector::default())?;
        if id != FontId::default() {
            return Err(Box::new(FontError::NotDefault));
        }
        Ok(id)
    }

    /// Load a font
    #[inline]
    pub fn load_font(&self, selector: FontSelector) -> Result<FontId, Box<dyn std::error::Error>> {
        let (path, index) = selector.select()?;
        self.load_pathbuf(path, index)
    }

    /// Load a font by path
    ///
    /// In case the `(path, index)` combination has already been loaded, the
    /// existing font object's [`FontId`] will be returned.
    ///
    /// The `index` is used to select fonts from a font-collection. If the font
    /// is not a collection, use `0`.
    pub fn load_pathbuf(
        &self,
        path: PathBuf,
        index: u32,
    ) -> Result<FontId, Box<dyn std::error::Error>> {
        let hash = self.hash_path(&path, index);

        // 1st lock: early exit if we already have this font
        let fonts = self.fonts.read().unwrap();
        for (h, id) in &fonts.1 {
            if *h == hash {
                return Ok(*id);
            }
        }
        drop(fonts);

        // 2nd lock: load and store file data / get reference
        let mut data = self.data.write().unwrap();
        let slice = if let Some(entry) = data.get(&path) {
            let slice: &[u8] = &entry[..];
            // Safety: slice is in self.data and will not be dropped or modified
            unsafe { extend_lifetime(slice) }
        } else {
            let v = std::fs::read(&path)?.into_boxed_slice();
            let slice = &data.entry(path).or_insert(v)[..];
            // Safety: as above
            unsafe { extend_lifetime(slice) }
        };
        drop(data);

        // 3rd lock: insert into font list
        let store = FontStore::new(slice, index)?;
        let mut fonts = self.fonts.write().unwrap();
        let id = FontId(fonts.0.len() as u32);
        fonts.0.push(Box::new(store));
        fonts.1.push((hash, id));

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontSelector;

impl FontSelector {
    fn select(&self) -> Result<(PathBuf, u32), SelectionError> {
        let families = [FamilyName::SansSerif];
        let properties = Properties::new();
        let handle = SOURCE.with(|source| source.select_best_match(&families, &properties))?;
        Ok(match handle {
            Handle::Path { path, font_index } => (path, font_index),
            // Note: handling the following would require changes to data
            // management and should not occur anyway:
            Handle::Memory { .. } => panic!("Unexpected: font in memory"),
        })
    }
}

unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
    std::mem::transmute::<&'b T, &'static T>(r)
}

// internals
impl FontLibrary {
    // Private because: safety depends on instance(s) never being destructed.
    fn new() -> Self {
        FontLibrary {
            data: Default::default(),
            fonts: Default::default(),
        }
    }
}

lazy_static::lazy_static! {
    static ref LIBRARY: FontLibrary = FontLibrary::new();
}
thread_local! {
    // This type is not Send, so we cannot store in a Mutex within lazy_static.
    // TODO: avoid multiple instances, since initialisation may be slow.
    static SOURCE: SystemSource = SystemSource::new();
}

/// Access the [`FontLibrary`] singleton
pub fn fonts() -> &'static FontLibrary {
    &*LIBRARY
}
