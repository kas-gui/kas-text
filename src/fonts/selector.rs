// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font selection
//!
//! Many items are copied from font-kit to avoid any public dependency.

use super::families;
use fontdb::{FaceInfo, Source};
pub use fontdb::{Stretch, Style, Weight};
use std::borrow::Cow;
use std::collections::hash_map::{Entry, HashMap};

/// How to add new aliases when others exist
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AddMode {
    Prepend,
    Append,
    Replace,
}

/// Manages the list of available fonts and font selection
pub struct Database {
    db: fontdb::Database,
    family_upper: Vec<String>,
    aliases: HashMap<Cow<'static, str>, Vec<Cow<'static, str>>>,
}

impl Database {
    pub(crate) fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let family_upper = db
            .faces()
            .iter()
            .map(|face| face.family.to_uppercase())
            .collect();

        let mut aliases = HashMap::new();
        aliases.insert(
            "serif".into(),
            families::DEFAULT_SERIF
                .iter()
                .map(|s| (*s).into())
                .collect(),
        );
        aliases.insert(
            "sans-serif".into(),
            families::DEFAULT_SANS_SERIF
                .iter()
                .map(|s| (*s).into())
                .collect(),
        );
        aliases.insert(
            "monospace".into(),
            families::DEFAULT_MONOSPACE
                .iter()
                .map(|s| (*s).into())
                .collect(),
        );
        aliases.insert(
            "cursive".into(),
            families::DEFAULT_CURSIVE
                .iter()
                .map(|s| (*s).into())
                .collect(),
        );
        aliases.insert(
            "fantasy".into(),
            families::DEFAULT_FANTASY
                .iter()
                .map(|s| (*s).into())
                .collect(),
        );

        Database {
            db,
            family_upper,
            aliases,
        }
    }

    /// Add font aliases for family
    ///
    /// When searching for `family`, all `aliases` will be searched too.
    pub fn add_aliases<I>(&mut self, family: Cow<'static, str>, aliases: I, mode: AddMode)
    where
        I: Iterator<Item = Cow<'static, str>>,
    {
        match self.aliases.entry(family) {
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
}

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct FontSelector<'a> {
    families: Vec<Cow<'a, str>>,
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
    /// If an empty vec is passed, the default "sans-serif" font is used.
    #[inline]
    pub fn set_families(&mut self, names: Vec<Cow<'a, str>>) {
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
        db: &'b Database,
        mut add_face: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(&'b Source, u32) -> Result<(), Box<dyn std::error::Error>>,
    {
        // TODO(opt): improve, perhaps moving some computation earlier (e.g.
        // culling aliases which do not resolve fonts), and use faster alias expansion.
        let mut families: Vec<Cow<'b, str>> = self.families.clone();
        if families.is_empty() {
            // We allow an empty family list to resolve to SansSerif.
            families.push("sans-serif".into());
        }

        // Append aliases
        // This is vaguely step 2, but allows generic names to resolve to multiple targets.
        let mut i = 0;
        while i < families.len() {
            if let Some(aliases) = db.aliases.get(&families[i]) {
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
            for (i, upper_name) in db.family_upper.iter().enumerate() {
                if *upper_name == family.to_uppercase() {
                    candidates.push(&db.db.faces()[i]);
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
        let matching_style = *style_preference
            .iter()
            .filter(|&query_style| {
                matching_set
                    .iter()
                    .any(|&index| candidates[index].style == *query_style)
            })
            .next()?;

        matching_set.retain(|&index| candidates[index].style == matching_style);

        // Step 4c (`font-weight`).
        //
        // The spec doesn't say what to do if the weight is between 400 and 500 exclusive, so we
        // just use 450 as the cutoff.
        let weight = self.weight.0;
        let matches = weight >= 400
            && weight < 450
            && matching_set
                .iter()
                .any(|&index| candidates[index].weight.0 == 500);

        let matching_weight = if matches {
            // Check 500 first.
            Weight::MEDIUM
        } else if weight >= 450
            && weight <= 500
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
