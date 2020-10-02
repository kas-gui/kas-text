// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font selection and loading
//!
//! Fonts are managed by the [`FontLibrary`], of which a static singleton
//! exists and can be accessed via [`fonts`].
//!
//! The first font loaded is known as the "default font" and used for all texts
//! which do not explicitly select another font. As such, the user of this
//! library *must* load the default font before all other fonts and before
//! any operation requiring font metrics:
//!
//! ```
//! if let Err(e) = kas_text::fonts::fonts().load_default() {
//!     panic!("Error loading font: {}", e);
//! }
//! // from now on, kas_text::fonts::FontId::default() identifies the default font
//! ```

pub use ab_glyph::PxScale;
use ab_glyph::{FontRef, InvalidFont, PxScaleFont};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use thiserror::Error;

mod selector;
pub use selector::*;

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

#[derive(Default)]
struct FontsData {
    fonts: Vec<Box<FontStore<'static>>>,
    // These are vec-maps. Why? Because length should be short.
    sel_hash: Vec<(u64, FontId)>,
    path_hash: Vec<(u64, FontId)>,
}

impl FontsData {
    fn push(&mut self, font: Box<FontStore<'static>>, sel_hash: u64, path_hash: u64) -> FontId {
        let id = FontId(self.fonts.len() as u32);
        self.fonts.push(font);
        self.sel_hash.push((sel_hash, id));
        self.path_hash.push((path_hash, id));
        id
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
    fonts: RwLock<FontsData>,
}

// public API
impl FontLibrary {
    /// Get a font from its identifier
    pub fn get(&self, id: FontId) -> Font {
        let fonts = self.fonts.read().unwrap();
        assert!(
            id.get() < fonts.fonts.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        let font: &FontRef<'static> = &fonts.fonts[id.get()].ab_glyph;
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
        assert!(
            id.get() < fonts.fonts.len(),
            "FontLibrary: invalid {:?}!",
            id
        );
        harfbuzz_rs::Font::new(fonts.fonts[id.get()].harfbuzz.clone())
    }

    /// Get the number of loaded fonts
    ///
    /// [`FontId`] values are assigned consecutively and fonts are never
    /// un-loaded, so to check all fonts are loaded one only needs to test
    /// whether this value is larger than the number of fonts loaded by the
    /// renderer.
    pub fn num_fonts(&self) -> usize {
        let fonts = self.fonts.read().unwrap();
        fonts.fonts.len()
    }

    /// Get a list of all fonts from `start`
    ///
    /// Uses the half-open range `start..` (so start=0 returns all fonts).
    ///
    /// Note that fonts may be loaded any time a new [`FormattedText`] is set
    /// and then [`Text::prepare`] is called since fonts are loaded on demand.
    /// Fonts are never unloaded, hence if fonts `0..n1` have been loaded by a
    /// renderer such as `glyph_brush` and now [`FontLibrary::num_fonts`] is
    /// `n2 > n1`, then only `ab_glyph_fonts_from(n1)` need be loaded.
    pub fn ab_glyph_fonts_from(&self, start: usize) -> Vec<&'static FontRef<'static>> {
        let fonts = self.fonts.read().unwrap();
        // Safety: each font is boxed so that its address never changes and
        // fonts are never modified or freed before program exit.
        fonts.fonts[start..]
            .iter()
            .map(|font| unsafe { extend_lifetime(&font.ab_glyph) })
            .collect()
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

        let (path, index) = selector.select()?;
        self.load_pathbuf(path, index, sel_hash)
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
    pub fn load_pathbuf(
        &self,
        path: PathBuf,
        index: u32,
        sel_hash: u64,
    ) -> Result<FontId, Box<dyn std::error::Error>> {
        let path_hash = self.hash_path(&path, index);

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
        let id = fonts.push(Box::new(store), sel_hash, path_hash);

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

pub(crate) unsafe fn extend_lifetime<'b, T: ?Sized>(r: &'b T) -> &'static T {
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

/// Access the [`FontLibrary`] singleton
pub fn fonts() -> &'static FontLibrary {
    &*LIBRARY
}
