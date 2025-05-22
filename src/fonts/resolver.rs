// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font resolver
//!
//! Many items are copied from font-kit to avoid any public dependency.

use super::{FontStyle, FontWeight, FontWidth};
use fontique::{
    Attributes, Collection, FamilyId, GenericFamily, QueryFamily, QueryFont, QueryStatus,
    SourceCache,
};
use log::debug;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

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

    /// Get a font family name from an id
    pub fn font_family(&mut self, id: FamilyId) -> Option<&str> {
        self.collection.family_name(id)
    }

    /// Get a font family name for some generic font family
    pub fn font_family_from_generic(&mut self, generic: GenericFamily) -> Option<&str> {
        let id = self.collection.generic_families(generic).next()?;
        self.collection.family_name(id)
    }
}

/// Family descriptor
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FamilySelector {
    /// A family named with a `String`
    Named(String),
    // /// A family named with a `&str`
    // NameRef(&'static str),
    /// A generic family
    #[cfg_attr(feature = "serde", serde(with = "remote::GenericFamily"))]
    Generic(GenericFamily),
}

impl From<GenericFamily> for FamilySelector {
    fn from(gf: GenericFamily) -> Self {
        FamilySelector::Generic(gf)
    }
}

impl<'a> From<&'a FamilySelector> for QueryFamily<'a> {
    fn from(family: &'a FamilySelector) -> Self {
        match family {
            FamilySelector::Named(name) => QueryFamily::Named(&name),
            // FamilySelector::NameRef(name) => QueryFamily::Named(name),
            FamilySelector::Generic(gf) => QueryFamily::Generic(*gf),
        }
    }
}

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FontSelector {
    families: Vec<FamilySelector>,
    #[cfg_attr(feature = "serde", serde(default))]
    weight: FontWeight,
    #[cfg_attr(feature = "serde", serde(default))]
    width: FontWidth,
    #[cfg_attr(feature = "serde", serde(default))]
    style: FontStyle,
}

impl FontSelector {
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
    pub fn set_families(&mut self, families: impl IntoIterator<Item: Into<FamilySelector>>) {
        self.families = families.into_iter().map(|item| item.into()).collect();
    }

    /// Set style
    #[inline]
    pub fn set_style(&mut self, style: FontStyle) {
        self.style = style;
    }

    /// Set weight
    #[inline]
    pub fn set_weight(&mut self, weight: FontWeight) {
        self.weight = weight;
    }

    /// Set width (stretch)
    #[inline]
    pub fn set_width(&mut self, width: FontWidth) {
        self.width = width;
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
            query.set_families(self.families.iter());
        }
        query.set_attributes(Attributes {
            width: self.width.into(),
            style: self.style.into(),
            weight: self.weight.into(),
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

    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
    #[repr(u8)]
    #[serde(remote = "fontique::GenericFamily")]
    pub enum GenericFamily {
        Serif = 0,
        SansSerif = 1,
        Monospace = 2,
        Cursive = 3,
        Fantasy = 4,
        SystemUi = 5,
        UiSerif = 6,
        UiSansSerif = 7,
        UiMonospace = 8,
        UiRounded = 9,
        Emoji = 10,
        Math = 11,
        FangSong = 12,
    }
}
