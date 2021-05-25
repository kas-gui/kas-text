// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font selection
//!
//! Many items are copied from font-kit to avoid any public dependency.

use super::families;
use fontdb::{Database, FaceInfo, Source};
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

impl<'a> PartialEq for FontSelector<'a> {
    fn eq(&self, other: &Self) -> bool {
        // This really should be derived...
        fn family_eq((a, b): (&Family, &Family)) -> bool {
            match (a, b) {
                (Family::Name(a), Family::Name(b)) => a == b,
                (Family::Serif, Family::Serif) => true,
                (Family::SansSerif, Family::SansSerif) => true,
                (Family::Cursive, Family::Cursive) => true,
                (Family::Fantasy, Family::Fantasy) => true,
                (Family::Monospace, Family::Monospace) => true,
                _ => false,
            }
        }

        self.names.len() == other.names.len()
            && self.names.iter().zip(other.names.iter()).all(family_eq)
            && self.weight == other.weight
            && self.stretch == other.stretch
            && self.style == other.style
    }
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
        let faces: Vec<(String, &FaceInfo)> = db
            .faces()
            .iter()
            .map(|face| (face.family.to_uppercase(), face))
            .collect();

        // We allow an empty family list to resolve to SansSerif.
        let mut families = &[Family::SansSerif][..];
        if self.names.len() > 0 {
            families = &self.names[..];
        }

        let mut candidates = Vec::new();
        for family in families {
            // Resolve implied family name(s).
            // This is vaguely step 2, but allows generic names to resolve to multiple targets.
            let mut name_arr = [""];
            let names: &[&str] = match family {
                Family::Name(name) => {
                    name_arr[0] = name;
                    &name_arr
                }
                Family::Serif => &families::DEFAULT_SERIF,
                Family::SansSerif => &families::DEFAULT_SANS_SERIF,
                Family::Cursive => &families::DEFAULT_CURSIVE,
                Family::Fantasy => &families::DEFAULT_FANTASY,
                Family::Monospace => &families::DEFAULT_MONOSPACE,
            };

            // Step 3: find any matching font faces, case-insensitively, including localised
            // variants (starts_with may be overly permissive here).
            for name in names.iter() {
                for (upper_name, face) in faces.iter() {
                    // TODO: exact match only or starting-with?
                    // if upper_name.starts_with(&name.to_uppercase()) {
                    if *upper_name == name.to_uppercase() {
                        candidates.push(*face);
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
