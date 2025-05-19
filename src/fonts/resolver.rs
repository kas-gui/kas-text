// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font resolver
//!
//! Many items are copied from font-kit to avoid any public dependency.

use easy_cast::Cast;
use fontdb::Database;
pub use fontdb::{Stretch, Style, Weight};
use fontique::{
    Attributes, Collection, FontStyle, FontWeight, FontWidth, GenericFamily, QueryFont,
    QueryStatus, SourceCache,
};
use log::{debug, info};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

fn to_uppercase<'a>(c: Cow<'a, str>) -> Cow<'a, str> {
    match c {
        Cow::Borrowed(b) if !b.chars().any(|c| c.is_lowercase()) => Cow::Borrowed(b),
        c => Cow::Owned(c.to_uppercase()),
    }
}

/// A tool to resolve a single font face given a family and style
pub struct Resolver {
    collection: Collection,
    cache: SourceCache,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        Resolver {
            collection: Collection::new(Default::default()),
            cache: SourceCache::new(Default::default()),
        }
    }

    /// Get a font family name for some generic font family
    pub fn font_family_from_generic(&mut self, generic: GenericFamily) -> Option<&str> {
        let id = self.collection.generic_families(generic).next()?;
        self.collection.family_name(id)
    }

    /// Init db and self
    pub(crate) fn init(&mut self, db: &mut Database) {
        info!("Found {} fonts", db.len());

        // Set family names in DB (only used in case the DB is used
        // externally, e.g. to render an SVG with resvg).
        if let Some(name) = self.font_family_from_generic(GenericFamily::Serif) {
            info!("Default serif font: {name}");
            db.set_serif_family(name);
        }
        if let Some(name) = self.font_family_from_generic(GenericFamily::SansSerif) {
            info!("Default sans-serif font: {name}");
            db.set_sans_serif_family(name);
        }
        if let Some(name) = self.font_family_from_generic(GenericFamily::Monospace) {
            info!("Default monospace font: {name}");
            db.set_monospace_family(name);
        }
        if let Some(name) = self.font_family_from_generic(GenericFamily::Cursive) {
            info!("Default cursive font: {name}");
            db.set_cursive_family(name);
        }
        if let Some(name) = self.font_family_from_generic(GenericFamily::Fantasy) {
            info!("Default fantasy font: {name}");
            db.set_fantasy_family(name);
        }
    }
}

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FontSelector<'a> {
    // contract: all entries are upper case
    families: Vec<Cow<'a, str>>,
    #[cfg_attr(feature = "serde", serde(default, with = "remote::Weight"))]
    weight: Weight,
    #[cfg_attr(feature = "serde", serde(default, with = "remote::Stretch"))]
    stretch: Stretch,
    #[cfg_attr(feature = "serde", serde(default, with = "remote::Style"))]
    style: Style,
}

impl<'a> FontSelector<'a> {
    /// Synonym for default
    ///
    /// Without further parametrization, this will select a generic sans-serif
    /// font which should be suitable for most uses.
    #[inline]
    pub fn new() -> Self {
        FontSelector::default()
    }

    /// Set family name(s)
    ///
    /// If multiple names are passed, the first to successfully resolve a font
    /// is used. Glyph-level fallback (missing glyph substitution) is not
    /// currently supported.
    ///
    /// If an empty vector is passed, the default "sans-serif" font is used.
    #[inline]
    pub fn set_families(&mut self, mut names: Vec<Cow<'a, str>>) {
        for x in &mut names {
            let mut y = Default::default();
            std::mem::swap(x, &mut y);
            *x = to_uppercase(y);
        }
        self.families = names;
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

    /// Resolve font faces for each matching font
    ///
    /// All font faces matching steps 1-4 will be returned through the `add_face` closure.
    pub(crate) fn select<F>(
        &self,
        resolver: &mut Resolver,
        mut add_face: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&QueryFont) -> Result<QueryStatus, Box<dyn std::error::Error>>,
    {
        debug!("select(): {self:?}");

        let mut query = resolver.collection.query(&mut resolver.cache);
        if self.families.is_empty() {
            query.set_families([
                GenericFamily::SystemUi,
                GenericFamily::UiSansSerif,
                GenericFamily::SansSerif,
            ]);
        } else {
            query.set_families(self.families.iter().map(|f| &**f));
        }
        query.set_attributes(Attributes {
            width: FontWidth::NORMAL,
            style: match self.style {
                Style::Normal => FontStyle::Normal,
                Style::Italic => FontStyle::Italic,
                Style::Oblique => FontStyle::Oblique(None),
            },
            weight: FontWeight::new(self.weight.0.cast()),
        });

        let mut result = Ok(());
        query.matches_with(|face| match add_face(face) {
            Ok(status) => status,
            Err(e) => {
                result = Err(e);
                QueryStatus::Stop
            }
        });
        result
    }
}

// See: https://serde.rs/remote-derive.html
#[cfg(feature = "serde")]
mod remote {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
    #[serde(remote = "fontdb::Weight")]
    pub struct Weight(pub u16);

    #[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize)]
    #[serde(remote = "fontdb::Stretch")]
    pub enum Stretch {
        UltraCondensed,
        ExtraCondensed,
        Condensed,
        SemiCondensed,
        Normal,
        SemiExpanded,
        Expanded,
        ExtraExpanded,
        UltraExpanded,
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
    #[serde(remote = "fontdb::Style")]
    pub enum Style {
        /// A face that is neither italic not obliqued.
        Normal,
        /// A form that is generally cursive in nature.
        Italic,
        /// A typically-sloped version of the regular face.
        Oblique,
    }
}
