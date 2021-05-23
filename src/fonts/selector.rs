// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font selection
//!
//! Many items are copied from font-kit to avoid any public dependency.

pub use fontdb::{Family, Stretch, Style, Weight};

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
#[derive(Clone, Debug, Default)]
pub struct FontSelector<'a> {
    names: Vec<Family<'a>>,
    weight: Weight,
    stretch: Stretch,
    style: Style,
}

impl<'a> FontSelector<'a> {
    /// Synonym for default
    ///
    /// Without further parametrisation, this will select a generic sans-serif
    /// font which should be suitable for most uses.
    #[inline]
    pub fn new() -> Self {
        FontSelector::default()
    }

    /// Set self to `rhs`
    ///
    /// This may save a reallocation over direct assignment.
    #[inline]
    pub fn assign(&mut self, rhs: &Self) {
        self.names.clear();
        self.names.extend_from_slice(&rhs.names);
        self.weight = rhs.weight;
        self.stretch = rhs.stretch;
        self.style = rhs.style;
    }

    /// Set family name(s)
    ///
    /// If multiple names are passed, the first to successfully resolve a font
    /// is used. Glyph-level fallback (missing glyph substitution) is not
    /// currently supported.
    ///
    /// If an empty vec is passed, the default sans-serif font is used.
    #[inline]
    pub fn set_families(&mut self, names: Vec<Family<'a>>) {
        self.names = names;
    }

    /// Set style
    #[inline]
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }

    /// Set weight
    #[inline]
    pub fn set_weight(&mut self, weight: Weight) {
        self.weight = weight;
    }

    /// Set stretch
    #[inline]
    pub fn set_stretch(&mut self, stretch: Stretch) {
        self.stretch = stretch;
    }

    /// Hash self
    ///
    /// This struct does not implement `Hash` since it doesn't precisely match
    /// the expected semantics: values may compare equal despite having
    /// different hashes. For our purposes this is acceptable.
    pub fn hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.names.len().hash(&mut hasher);
        for name in &self.names {
            match name {
                Family::Name(name) => {
                    0u16.hash(&mut hasher);
                    name.hash(&mut hasher);
                }
                Family::Serif => 1u16.hash(&mut hasher),
                Family::SansSerif => 2u16.hash(&mut hasher),
                Family::Cursive => 3u16.hash(&mut hasher),
                Family::Fantasy => 4u16.hash(&mut hasher),
                Family::Monospace => 5u16.hash(&mut hasher),
            }
        }
        self.weight.0.hash(&mut hasher);
        self.stretch.to_number().hash(&mut hasher);
        (self.style as u16).hash(&mut hasher);
        hasher.finish()
    }

    /// Resolve a path and collection index from the given criteria
    pub(crate) fn select<'b>(&self, db: &'b fontdb::Database) -> Option<(&'b fontdb::Source, u32)> {
        let mut families = &[fontdb::Family::SansSerif][..];
        if self.names.len() > 0 {
            families = &self.names[..];
        }

        let query = fontdb::Query {
            families,
            weight: self.weight,
            stretch: self.stretch,
            style: self.style,
        };
        db.query(&query)
            .and_then(|id| db.face(id))
            .map(|face| (&*face.source, face.index))
    }
}
