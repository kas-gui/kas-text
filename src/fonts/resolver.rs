// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font resolver
//!
//! Many items are copied from font-kit to avoid any public dependency.

use super::{FontStyle, FontWeight, FontWidth};
use fontique::{
    Attributes, Collection, FamilyId, GenericFamily, QueryFamily, QueryFont, QueryStatus, Script,
    SourceCache,
};
use log::debug;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};

/// A tool to resolve a single font face given a family and style
pub struct Resolver {
    collection: Collection,
    cache: SourceCache,
    /// Cached family selectors:
    families: HashMap<FamilySelector, FamilySet>,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        Resolver {
            collection: Collection::new(Default::default()),
            cache: SourceCache::new(Default::default()),
            families: HashMap::new(),
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

    /// Construct a [`FamilySelector`] for the given `families`
    pub fn select_families<I, F>(&mut self, families: I) -> FamilySelector
    where
        I: IntoIterator<Item = F>,
        F: Into<FamilyName>,
    {
        let set = FamilySet(families.into_iter().map(|f| f.into()).collect());
        let hash = self.families.hasher().hash_one(&set);
        let sel = FamilySelector(hash | (1 << 63));

        match self.families.entry(sel) {
            Entry::Vacant(entry) => {
                entry.insert(set);
            }
            Entry::Occupied(entry) => {
                // Unlikely but possible case:
                log::warn!("Resolver::select_families: hash collision for family selector {set:?} and {:?}", entry.get());
                // TODO: inject a random value into the FamilySet and rehash?
            }
        }

        sel
    }

    /// Resolve families from a [`FamilySelector`]
    ///
    /// Returns an empty [`Vec`] on error.
    pub fn resolve_families(&self, selector: &FamilySelector) -> Vec<FamilyName> {
        if let Some(gf) = selector.as_generic() {
            vec![FamilyName::Generic(gf)]
        } else if let Some(set) = self.families.get(selector) {
            set.0.clone()
        } else {
            vec![]
        }
    }
}

/// A family name
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FamilyName {
    /// A family named with a `String`
    Named(String),
    /// A generic family
    #[cfg_attr(feature = "serde", serde(with = "remote::GenericFamily"))]
    Generic(GenericFamily),
}

impl From<GenericFamily> for FamilyName {
    fn from(gf: GenericFamily) -> Self {
        FamilyName::Generic(gf)
    }
}

impl<'a> From<&'a FamilyName> for QueryFamily<'a> {
    fn from(family: &'a FamilyName) -> Self {
        match family {
            FamilyName::Named(name) => QueryFamily::Named(name),
            FamilyName::Generic(gf) => QueryFamily::Generic(*gf),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FamilySet(Vec<FamilyName>);

/// A (cached) family selector
///
/// This may be constructed directly for some generic families; for other
/// families use [`Resolver::select_families`].
///
/// This is a small, `Copy` type (a newtype over `u64`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FamilySelector(u64);

impl FamilySelector {
    /// Use a serif font
    pub const SERIF: FamilySelector = FamilySelector(0);

    /// Use a sans-serif font
    pub const SANS_SERIF: FamilySelector = FamilySelector(1);

    /// Use a monospace font
    pub const MONOSPACE: FamilySelector = FamilySelector(2);

    /// Use a cursive font
    pub const CURSIVE: FamilySelector = FamilySelector(3);

    /// Use the system UI font
    pub const SYSTEM_UI: FamilySelector = FamilySelector(5);

    /// Use an emoji font
    pub const FANG_SONG: FamilySelector = FamilySelector(12);

    fn as_generic(self) -> Option<GenericFamily> {
        match self.0 {
            0 => Some(GenericFamily::Serif),
            1 => Some(GenericFamily::SansSerif),
            2 => Some(GenericFamily::Monospace),
            3 => Some(GenericFamily::Cursive),
            5 => Some(GenericFamily::SystemUi),
            12 => Some(GenericFamily::FangSong),
            _ => None,
        }
    }
}

/// Default-constructs to [`FamilySelector::SYSTEM_UI`].
impl Default for FamilySelector {
    fn default() -> Self {
        FamilySelector::SYSTEM_UI
    }
}

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
///
/// This can be converted [from](From) a [`FamilySelector`], selecting the
/// default styles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct FontSelector {
    /// Family selector
    pub family: FamilySelector,
    /// Weight
    pub weight: FontWeight,
    /// Width
    pub width: FontWidth,
    /// Italic / oblique style
    pub style: FontStyle,
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

    /// Resolve font faces for each matching font
    ///
    /// All font faces matching steps 1-4 will be returned through the `add_face` closure.
    pub(crate) fn select<F>(&self, resolver: &mut Resolver, script: Script, add_face: F)
    where
        F: FnMut(&QueryFont) -> QueryStatus,
    {
        let mut query = resolver.collection.query(&mut resolver.cache);
        if let Some(gf) = self.family.as_generic() {
            debug!(
                "select: Script::{:?}, GenericFamily::{:?}, {:?}, {:?}, {:?}",
                &script, gf, &self.weight, &self.width, &self.style
            );

            query.set_families([gf]);
        } else if let Some(set) = resolver.families.get(&self.family) {
            debug!(
                "select: Script::{:?}, {:?}, {:?}, {:?}, {:?}",
                &script, set, &self.weight, &self.width, &self.style
            );

            query.set_families(set.0.iter());
        }

        query.set_attributes(Attributes {
            width: self.width.into(),
            style: self.style.into(),
            weight: self.weight.into(),
        });

        query.set_fallbacks(script);

        query.matches_with(add_face);
    }
}

impl From<FamilySelector> for FontSelector {
    #[inline]
    fn from(family: FamilySelector) -> Self {
        FontSelector {
            family,
            ..Default::default()
        }
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
