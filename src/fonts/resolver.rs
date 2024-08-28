// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font resolver
//!
//! Many items are copied from font-kit to avoid any public dependency.

use super::families;
use fontdb::{Database, FaceInfo, Source, ID};
pub use fontdb::{Stretch, Style, Weight};
use log::{debug, info, trace};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::hash_map::{Entry, HashMap};
use std::fmt;

/// How to add new aliases when others exist
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AddMode {
    Prepend,
    Append,
    Replace,
}

fn to_uppercase<'a>(c: Cow<'a, str>) -> Cow<'a, str> {
    match c {
        Cow::Borrowed(b) if !b.chars().any(|c| c.is_lowercase()) => Cow::Borrowed(b),
        c => Cow::Owned(c.to_uppercase()),
    }
}

/// A tool to resolve a single font face given a family and style
pub struct Resolver {
    load_system_fonts: bool,
    families_upper: HashMap<String, Vec<ID>>,
    // contract: all keys and values are uppercase
    aliases: HashMap<Cow<'static, str>, Vec<Cow<'static, str>>>,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        let mut aliases = HashMap::new();
        // TODO: update families instead of mapping to uppercase here
        aliases.insert(
            "SERIF".into(),
            families::DEFAULT_SERIF
                .iter()
                .map(|s| to_uppercase((*s).into()))
                .collect(),
        );
        aliases.insert(
            "SANS-SERIF".into(),
            families::DEFAULT_SANS_SERIF
                .iter()
                .map(|s| to_uppercase((*s).into()))
                .collect(),
        );
        aliases.insert(
            "MONOSPACE".into(),
            families::DEFAULT_MONOSPACE
                .iter()
                .map(|s| to_uppercase((*s).into()))
                .collect(),
        );
        aliases.insert(
            "CURSIVE".into(),
            families::DEFAULT_CURSIVE
                .iter()
                .map(|s| to_uppercase((*s).into()))
                .collect(),
        );
        aliases.insert(
            "FANTASY".into(),
            families::DEFAULT_FANTASY
                .iter()
                .map(|s| to_uppercase((*s).into()))
                .collect(),
        );

        Resolver {
            load_system_fonts: true,
            families_upper: HashMap::new(),
            aliases,
        }
    }

    /// Access the list of discovered font families
    ///
    /// All family names are uppercase.
    pub fn families_upper(&self) -> impl Iterator<Item = &str> {
        self.families_upper.keys().map(|s| s.as_str())
    }

    /// List all font family alias keys
    ///
    /// All family names are uppercase.
    pub fn alias_keys(&self) -> impl Iterator<Item = &str> {
        self.aliases.keys().map(|k| k.as_ref())
    }

    /// List all aliases for the given family
    ///
    /// The `family` parameter must be upper case (or no matches will be found).
    /// All returned family names are uppercase.
    pub fn aliases_of(&self, family: &str) -> Option<impl Iterator<Item = &str>> {
        self.aliases
            .get(family)
            .map(|result| result.iter().map(|s| s.as_ref()))
    }

    /// Resolve the substituted font family name for this family
    ///
    /// The input must be upper case. The output will be the loaded font's case.
    /// Example: `SANS-SERIF`
    pub fn font_family_from_alias(&self, db: &Database, family: &str) -> Option<String> {
        let families_upper = &self.families_upper;
        self.aliases
            .get(family)
            .and_then(|list| list.iter().next())
            .map(|name| {
                let id = families_upper.get(name.as_ref()).unwrap()[0];
                db.face(id).unwrap().families.first().unwrap().0.clone()
            })
    }

    /// Add font aliases for family
    ///
    /// When searching for `family`, all `aliases` will be searched too. Both
    /// the `family` parameter and all `aliases` are converted to upper case.
    ///
    /// This method may only be used before initialization.
    pub fn add_aliases<I>(&mut self, family: Cow<'static, str>, aliases: I, mode: AddMode)
    where
        I: Iterator<Item = Cow<'static, str>>,
    {
        let aliases = aliases.map(to_uppercase);

        match self.aliases.entry(to_uppercase(family)) {
            Entry::Occupied(mut entry) => {
                let existing = entry.get_mut();
                match mode {
                    AddMode::Prepend => {
                        existing.splice(0..0, aliases);
                    }
                    AddMode::Append => {
                        existing.extend(aliases);
                    }
                    AddMode::Replace => {
                        existing.clear();
                        existing.extend(aliases);
                    }
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(aliases.collect());
            }
        }
    }

    /// Control whether system fonts will be loaded on initialization
    ///
    /// Default value: true
    pub fn set_load_system_fonts(&mut self, load: bool) {
        self.load_system_fonts = load;
    }

    /// Init db and self
    pub(crate) fn init(&mut self, db: &mut Database) {
        info!("Found {} fonts", db.len());

        let families_upper = &mut self.families_upper;
        for face in db.faces() {
            trace!("Discovered: {}", DisplayFaceInfo(face));
            // Use the first name, which according to docs is always en_US
            // (unless missing from the font).
            if let Some(family_name) = face.families.first().map(|pair| &pair.0) {
                families_upper
                    .entry(family_name.to_uppercase())
                    .or_default()
                    .push(face.id);
            }
        }

        for aliases in self.aliases.values_mut() {
            // Remove aliases to missing fonts:
            aliases.retain(|name| families_upper.contains_key(name.as_ref()));

            // Remove duplicates (this is O(n²), but n is usually small):
            let mut i = 0;
            while i < aliases.len() {
                if aliases[0..i].contains(&aliases[i]) {
                    aliases.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        // Set family names in DB (only used in case the DB is used
        // externally, e.g. to render an SVG with resvg).
        if let Some(name) = self.font_family_from_alias(db, "SERIF") {
            info!("Default serif font: {}", name);
            db.set_serif_family(name);
        }
        if let Some(name) = self.font_family_from_alias(db, "SANS-SERIF") {
            info!("Default sans-serif font: {}", name);
            db.set_sans_serif_family(name);
        }
        if let Some(name) = self.font_family_from_alias(db, "MONOSPACE") {
            info!("Default monospace font: {}", name);
            db.set_monospace_family(name);
        }
        if let Some(name) = self.font_family_from_alias(db, "CURSIVE") {
            info!("Default cursive font: {}", name);
            db.set_cursive_family(name);
        }
        if let Some(name) = self.font_family_from_alias(db, "FANTASY") {
            info!("Default fantasy font: {}", name);
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

    /// Set self to `rhs`
    ///
    /// This may save a reallocation over direct assignment.
    #[inline]
    pub fn assign(&mut self, rhs: &Self) {
        self.families.clear();
        self.families.extend_from_slice(&rhs.families);
        self.weight = rhs.weight;
        self.stretch = rhs.stretch;
        self.style = rhs.style;
    }

    /// Set family name(s)
    ///
    /// This supports generic names `serif`, `sans-serif`, `monospace`,
    /// `cursive` and `fantasy`. It also allows specific family names, though
    /// does not currently define compatibility aliases for these (e.g. `arial`
    /// will match the Arial font if found, but should not currently be expected
    /// to resolve other, compatible, fonts).
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
    /// This implements CSS selection logic as defined by
    /// [https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-style-matching](),
    /// steps 1-4. The result is a list of matching font faces which may later
    /// be matched against characters for character-level fallback (step 5).
    ///
    /// All font faces matching steps 1-4 will be returned through the `add_face` closure.
    pub(crate) fn select<'b, F>(
        &'b self,
        reslover: &'b Resolver,
        db: &'b Database,
        mut add_face: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&'b Source, u32) -> Result<(), Box<dyn std::error::Error>>,
    {
        debug!("select(): {:?}", self);
        // TODO(opt): improve, perhaps moving some computation earlier (e.g.
        // culling aliases which do not resolve fonts), and use faster alias expansion.
        let mut families: Vec<Cow<'b, str>> = self.families.clone();
        let sans_serif = Cow::<'static, str>::from("SANS-SERIF");
        if !families.contains(&sans_serif) {
            // All families fall back to sans-serif, ensuring we almost always have a usable font
            families.push(sans_serif);
        }

        // Append aliases
        // This is vaguely step 2, but allows generic names to resolve to multiple targets.
        let mut i = 0;
        while i < families.len() {
            if let Some(aliases) = reslover.aliases.get(&families[i]) {
                let mut j = i + 1;
                for alias in aliases {
                    if !families.contains(alias) {
                        families.insert(j, alias.clone());
                        j += 1;
                    }
                }
            }
            i += 1;
        }

        let mut candidates = Vec::new();
        // Step 3: find any matching font faces, case-insensitively
        for family in families {
            if let Some(ids) = reslover.families_upper.get(family.as_ref()) {
                for id in ids {
                    let candidate = db.face(*id).unwrap();
                    trace!("candidate: {}", DisplayFaceInfo(candidate));
                    candidates.push(candidate);
                }
            }

            // Step 4: if any match from a family, narrow to a single face.
            if !candidates.is_empty() {
                if let Some(index) = self.find_best_match(&candidates) {
                    let candidate = candidates[index];
                    add_face(&candidate.source, candidate.index)?;
                }
                candidates.clear();
            }
        }

        Ok(())
    }

    // https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-style-matching
    // Based on https://github.com/RazrFalcon/fontdb, itself based on https://github.com/servo/font-kit
    #[inline(never)]
    fn find_best_match(&self, candidates: &[&FaceInfo]) -> Option<usize> {
        debug_assert!(!candidates.is_empty());

        // Step 4.
        let mut matching_set: Vec<usize> = (0..candidates.len()).collect();

        // Step 4a (`font-stretch`).
        let matches = matching_set
            .iter()
            .any(|&index| candidates[index].stretch == self.stretch);
        let matching_stretch = if matches {
            // Exact match.
            self.stretch
        } else if self.stretch <= Stretch::Normal {
            // Closest stretch, first checking narrower values and then wider values.
            let stretch = matching_set
                .iter()
                .filter(|&&index| candidates[index].stretch < self.stretch)
                .min_by_key(|&&index| {
                    self.stretch.to_number() - candidates[index].stretch.to_number()
                });

            match stretch {
                Some(&matching_index) => candidates[matching_index].stretch,
                None => {
                    let matching_index = *matching_set.iter().min_by_key(|&&index| {
                        candidates[index].stretch.to_number() - self.stretch.to_number()
                    })?;

                    candidates[matching_index].stretch
                }
            }
        } else {
            // Closest stretch, first checking wider values and then narrower values.
            let stretch = matching_set
                .iter()
                .filter(|&&index| candidates[index].stretch > self.stretch)
                .min_by_key(|&&index| {
                    candidates[index].stretch.to_number() - self.stretch.to_number()
                });

            match stretch {
                Some(&matching_index) => candidates[matching_index].stretch,
                None => {
                    let matching_index = *matching_set.iter().min_by_key(|&&index| {
                        self.stretch.to_number() - candidates[index].stretch.to_number()
                    })?;

                    candidates[matching_index].stretch
                }
            }
        };
        matching_set.retain(|&index| candidates[index].stretch == matching_stretch);

        // Step 4b (`font-style`).
        let style_preference = match self.style {
            Style::Italic => [Style::Italic, Style::Oblique, Style::Normal],
            Style::Oblique => [Style::Oblique, Style::Italic, Style::Normal],
            Style::Normal => [Style::Normal, Style::Oblique, Style::Italic],
        };
        let matching_style = *style_preference.iter().find(|&query_style| {
            matching_set
                .iter()
                .any(|&index| candidates[index].style == *query_style)
        })?;

        matching_set.retain(|&index| candidates[index].style == matching_style);

        // Step 4c (`font-weight`).
        //
        // The spec doesn't say what to do if the weight is between 400 and 500 exclusive, so we
        // just use 450 as the cutoff.
        let weight = self.weight.0;
        let matches = (400..450).contains(&weight)
            && matching_set
                .iter()
                .any(|&index| candidates[index].weight.0 == 500);

        let matching_weight = if matches {
            // Check 500 first.
            Weight::MEDIUM
        } else if (450..=500).contains(&weight)
            && matching_set
                .iter()
                .any(|&index| candidates[index].weight.0 == 400)
        {
            // Check 400 first.
            Weight::NORMAL
        } else if weight <= 500 {
            // Closest weight, first checking thinner values and then fatter ones.
            let idx = matching_set
                .iter()
                .filter(|&&index| candidates[index].weight.0 <= weight)
                .min_by_key(|&&index| weight - candidates[index].weight.0);

            match idx {
                Some(&matching_index) => candidates[matching_index].weight,
                None => {
                    let matching_index = *matching_set
                        .iter()
                        .min_by_key(|&&index| candidates[index].weight.0 - weight)?;
                    candidates[matching_index].weight
                }
            }
        } else {
            // Closest weight, first checking fatter values and then thinner ones.
            let idx = matching_set
                .iter()
                .filter(|&&index| candidates[index].weight.0 >= weight)
                .min_by_key(|&&index| candidates[index].weight.0 - weight);

            match idx {
                Some(&matching_index) => candidates[matching_index].weight,
                None => {
                    let matching_index = *matching_set
                        .iter()
                        .min_by_key(|&&index| weight - candidates[index].weight.0)?;
                    candidates[matching_index].weight
                }
            }
        };
        matching_set.retain(|&index| candidates[index].weight == matching_weight);

        // Ignore step 4d (`font-size`).

        // Return the result.
        matching_set.into_iter().next()
    }
}

struct DisplayFaceInfo<'a>(&'a FaceInfo);
impl<'a> fmt::Display for DisplayFaceInfo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let family = &self.0.families.first().unwrap().0;
        let path = match &self.0.source {
            Source::Binary(_) => None,
            Source::File(path) => Some(path.display()),
            Source::SharedFile(path, _) => Some(path.display()),
        };
        write!(
            f,
            "family=\"{}\", source={:?},{}",
            family, path, self.0.index
        )
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
