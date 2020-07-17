// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — fonts

use ab_glyph::{FontRef, InvalidFont, PxScale, PxScaleFont};
use font_kit::source::SystemSource;
use font_kit::{family_name::FamilyName, handle::Handle, properties::Properties};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

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

/// Type-def: the type of fonts we provide
// Note: FontRef itself is too large to clone cheaply, so use a reference to it
pub type Font = &'static FontRef<'static>;

struct FontStore<'a> {
    ab_glyph: FontRef<'a>,
    #[cfg(feature = "harfbuzz_rs")]
    harfbuzz: harfbuzz_rs::Shared<harfbuzz_rs::Face<'a>>,
}

impl<'a> FontStore<'a> {
    fn new(data: &'a [u8], index: u32) -> Result<Self, InvalidFont> {
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
    fonts: RwLock<Vec<Box<FontStore<'static>>>>,
}

// public API
impl FontLibrary {
    /// Get a font from its identifier
    pub fn get(&self, id: FontId) -> Font {
        let fonts = self.fonts.read().unwrap();
        assert!(id.get() < fonts.len(), "FontLibrary: invalid {:?}!", id);
        let font: &FontRef<'static> = &fonts[id.get()].ab_glyph;
        // Safety: elements of self.fonts are never dropped or modified
        unsafe { extend_lifetime(font) }
    }

    /// Get a scaled font
    pub fn get_scaled(&self, font_id: FontId, font_scale: PxScale) -> PxScaleFont<Font> {
        ab_glyph::Font::into_scaled(self.get(font_id), font_scale)
    }

    /// Get a HarfBuzz font face
    ///
    /// This actually makes an instance on usage, which may be inefficient(?).
    #[cfg(feature = "harfbuzz_rs")]
    pub fn get_harfbuzz(
        &self,
        id: FontId,
        scale: PxScale,
    ) -> harfbuzz_rs::Owned<harfbuzz_rs::Font<'static>> {
        let fonts = self.fonts.read().unwrap();
        assert!(id.get() < fonts.len(), "FontLibrary: invalid {:?}!", id);
        let face = fonts[id.get()].harfbuzz.clone();
        let mut font = harfbuzz_rs::Font::new(face);
        // TODO: is this conversion correct?
        font.set_ppem(scale.x as u32, scale.y as u32);
        font
    }

    /// Get a list of all fonts
    ///
    /// E.g. `glyph_brush` needs this
    pub fn fonts_vec(&self) -> Vec<Font> {
        let fonts = self.fonts.read().unwrap();
        // Safety: each font is boxed so that its address never changes and
        // fonts are never modified or freed before program exit.
        fonts
            .iter()
            .map(|font| unsafe { extend_lifetime(&font.ab_glyph) })
            .collect()
    }

    /// Load a default font
    pub fn load_default(&self) -> Result<FontId, Box<dyn std::error::Error>> {
        // 1st lock: early exit if we already have this font
        let fonts = self.fonts.read().unwrap();
        if fonts.len() > 0 {
            // We already have a default font
            return Ok(FontId(0));
        }
        drop(fonts);

        let families = [FamilyName::SansSerif];
        let properties = Properties::new();
        let handle = SOURCE.with(|source| source.select_best_match(&families, &properties))?;
        let (path, index) = match handle {
            Handle::Path { path, font_index } => (path, font_index),
            // Note: handling the following would require changes to data
            // management and should not occur anyway:
            Handle::Memory { .. } => panic!("Unexpected: font in memory"),
        };

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
        let id = FontId(fonts.len() as u32);
        fonts.push(Box::new(store));

        Ok(id)
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
