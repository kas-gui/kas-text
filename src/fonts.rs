// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font selection and loading
//!
//! Fonts are managed by the [`FontLibrary`], of which a static singleton
//! exists and can be accessed via [`fonts`].
//!
//! ### FontId and the default font
//!
//! The [`FontId`] type is a numeric identifier for loaded fonts. It may be
//! default-constructed to access the *default* font, with number 0.
//!
//! To make this work, the user of this library *must* load the default font
//! before all other fonts and before any operation requiring font metrics:
//! ```
//! if let Err(e) = kas_text::fonts::fonts().load_default() {
//!     panic!("Error loading font: {}", e);
//! }
//! // from now on, kas_text::fonts::FontId::default() identifies the default font
//! ```
//!
//! ### Font sizes
//!
//! Typically, font sizes are specified in "Points". Several other units and
//! measures come into play here. Lets start with those dating back to the
//! early printing press:
//!
//! -   1 *Point* = 1/72 inch (~0.35mm), by the usual DTP standard
//! -   1 *Em* is the width of a capital `M` (inclusive of margin) in some font
//! -   The *point size* of a font refers to the number of *points* per *em*
//!
//! Thus, with a "12 point font", one 'M' occupies 12/72 of an inch on paper.
//!
//! In digital typography, one must translate to/from pixel sizes. Here we have:
//!
//! -   DPI (Dots Per Inch) is the number of pixels per inch
//! -   A *scale factor* is a measure of the number of pixels relative to a
//!     standard DPI, usually 96
//!
//! We introduce two measures used by this library:
//!
//! -   DPP (Dots Per Point): `dpp = dpi / 72 = scale_factor × (96 / 72)`
//! -   DPEM (Dots Per Em): `dpem = point_size × dpp`
//!
//! Warning: on MacOS and Apple systems, a *point* sometimes refers to a
//! (virtual) pixel, yielding `dpp = 1` (or `dpp = 2` on Retina screens, or
//! something else entirely on iPhones). On any system, DPI/DPP/scale factor
//! values may be set according to the user's taste rather than physical
//! measurements.
//!
//! Finally, note that digital font files have an internally defined unit
//! known as the *font unit*. This is not normally used directly but is used
//! by the `DPU` type.

use crate::conv::{to_u32, to_usize, LineMetrics, DPU};
use crate::GlyphId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard};
use thiserror::Error;
pub(crate) use ttf_parser::Face;

mod selector;
pub use selector::*;

impl From<GlyphId> for ttf_parser::GlyphId {
    fn from(id: GlyphId) -> Self {
        ttf_parser::GlyphId(id.0)
    }
}

/// Font loading errors
#[derive(Error, Debug)]
enum FontError {
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

/// Handle to a loaded font face
#[derive(Copy, Clone, Debug)]
pub struct FaceRef(&'static Face<'static>);

impl FaceRef {
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
    pub(crate) fn dpu(self, dpem: f32) -> DPU {
        DPU(dpem / f32::from(self.0.units_per_em().unwrap()))
    }

    /// Get a scaled reference
    ///
    /// Units: `dpem` is dots (pixels) per Em (module documentation).
    #[inline]
    pub(crate) fn scale_by_dpem(self, dpem: f32) -> ScaledFaceRef {
        ScaledFaceRef(self.0, self.dpu(dpem))
    }

    /// Get a scaled reference
    ///
    /// Units: `dpu` is dots (pixels) per font-unit (see module documentation).
    #[inline]
    pub(crate) fn scale_by_dpu(self, dpu: DPU) -> ScaledFaceRef {
        ScaledFaceRef(self.0, dpu)
    }

    /// Get the height of horizontal text in pixels
    ///
    /// Units: `dpem` is dots (pixels) per Em (module documentation).
    #[inline]
    pub fn height(&self, dpem: f32) -> f32 {
        self.scale_by_dpem(dpem).height()
    }
}

/// Handle to a loaded font face
#[derive(Copy, Clone, Debug)]
pub(crate) struct ScaledFaceRef(&'static Face<'static>, DPU);
impl ScaledFaceRef {
    /// Unscaled face
    #[inline]
    #[allow(unused)] // built-in shaper only
    pub fn face(&self) -> &Face<'static> {
        self.0
    }

    #[inline]
    #[allow(unused)] // built-in shaper only
    pub fn dpu(&self) -> DPU {
        self.1
    }

    #[inline]
    #[allow(unused)] // built-in shaper only
    pub(crate) fn glyph_id(&self, c: char) -> GlyphId {
        // GlyphId 0 is required to be a special glyph representing a missing
        // character (see cmap table / TrueType specification).
        GlyphId(self.0.glyph_index(c).map(|id| id.0).unwrap_or(0))
    }

    #[inline]
    pub fn h_advance(&self, id: GlyphId) -> f32 {
        let x = self.0.glyph_hor_advance(id.into()).unwrap();
        self.1.u16_to_px(x)
    }

    #[inline]
    pub fn h_side_bearing(&self, id: GlyphId) -> f32 {
        let x = self.0.glyph_hor_side_bearing(id.into()).unwrap();
        self.1.i16_to_px(x)
    }

    #[inline]
    pub fn ascent(&self) -> f32 {
        self.1.i16_to_px(self.0.ascender())
    }

    #[inline]
    pub fn descent(&self) -> f32 {
        self.1.i16_to_px(self.0.descender())
    }

    #[inline]
    pub fn line_gap(&self) -> f32 {
        self.1.i16_to_px(self.0.line_gap())
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.1.i16_to_px(self.0.height())
    }

    #[inline]
    pub fn underline_metrics(&self) -> Option<LineMetrics> {
        self.0
            .underline_metrics()
            .map(|m| self.1.to_line_metrics(m))
    }

    #[inline]
    pub fn strikethrough_metrics(&self) -> Option<LineMetrics> {
        self.0
            .strikeout_metrics()
            .map(|m| self.1.to_line_metrics(m))
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
pub struct FontLibrary {
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
            let slice = &data.entry(path.clone()).or_insert(v)[..];
            // Safety: as above
            unsafe { extend_lifetime(slice) }
        };
        drop(data);

        // 3rd lock: insert into font list
        let store = FaceStore::new(path, slice, index)?;
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
