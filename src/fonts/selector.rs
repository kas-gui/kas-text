// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — font selection
//!
//! Many items are copied from font-kit to avoid any public dependency.

use font_kit::error::SelectionError;
use font_kit::family_name::FamilyName as fkFamilyName;
use font_kit::handle::Handle;
use font_kit::properties::{self, Properties, Style as fkStyle};
use font_kit::source::SystemSource;
use std::path::PathBuf;

/// A font face selection tool
///
/// This tool selects a font according to the given criteria from available
/// system fonts. Selection criteria are based on CSS.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontSelector {
    names: Vec<FamilyName>,
    properties: Properties,
}

impl FontSelector {
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
        self.properties = rhs.properties;
    }

    /// Set family name(s)
    ///
    /// If multiple names are passed, the first to successfully resolve a font
    /// is used. Glyph-level fallback (missing glyph substitution) is not
    /// currently supported.
    #[inline]
    pub fn set_families(&mut self, names: Vec<FamilyName>) {
        self.names = names;
    }

    /// Set style
    #[inline]
    pub fn set_style(&mut self, style: Style) {
        self.properties.style = style.into();
    }

    /// Set weight
    #[inline]
    pub fn set_weight(&mut self, weight: Weight) {
        self.properties.weight = properties::Weight(weight.0);
    }

    /// Set stretch
    #[inline]
    pub fn set_stretch(&mut self, stretch: Stretch) {
        self.properties.stretch = properties::Stretch(stretch.0);
    }

    /// Resolve a path and collection index from the given criteria
    pub(crate) fn select(self) -> Result<(PathBuf, u32), SelectionError> {
        let mut families = &[fkFamilyName::SansSerif][..];
        let names: Vec<fkFamilyName>;
        if self.names.len() > 0 {
            names = self.names.into_iter().map(|n| n.into()).collect();
            families = &names[..];
        }
        let properties = self.properties;

        let handle = SOURCE.with(|source| source.select_best_match(families, &properties))?;
        Ok(match handle {
            Handle::Path { path, font_index } => (path, font_index),
            // Note: handling the following would require changes to data
            // management and should not occur anyway:
            Handle::Memory { .. } => panic!("Unexpected: font in memory"),
        })
    }
}

/// A possible value for the `font-family` CSS property.
///
/// These descriptions are taken from
/// [CSS Fonts Level 3 § 3.1](https://drafts.csswg.org/css-fonts-3/#font-family-prop).
#[derive(Clone, Debug, PartialEq)]
pub enum FamilyName {
    /// A specific font family, specified by name: e.g. "Arial", "times".
    Title(String),
    /// Serif fonts represent the formal text style for a script.
    Serif,
    /// Glyphs in sans-serif fonts, as the term is used in CSS, are generally low contrast
    /// (vertical and horizontal stems have the close to the same thickness) and have stroke
    /// endings that are plain — without any flaring, cross stroke, or other ornamentation.
    SansSerif,
    /// The sole criterion of a monospace font is that all glyphs have the same fixed width.
    Monospace,
    /// Glyphs in cursive fonts generally use a more informal script style, and the result looks
    /// more like handwritten pen or brush writing than printed letterwork.
    Cursive,
    /// Fantasy fonts are primarily decorative or expressive fonts that contain decorative or
    /// expressive representations of characters.
    Fantasy,
}

impl From<FamilyName> for fkFamilyName {
    fn from(name: FamilyName) -> Self {
        match name {
            FamilyName::Title(name) => fkFamilyName::Title(name),
            FamilyName::Serif => fkFamilyName::Serif,
            FamilyName::SansSerif => fkFamilyName::SansSerif,
            FamilyName::Monospace => fkFamilyName::Monospace,
            FamilyName::Cursive => fkFamilyName::Cursive,
            FamilyName::Fantasy => fkFamilyName::Fantasy,
        }
    }
}

/// Allows italic or oblique faces to be selected
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    /// A face that is neither italic not obliqued.
    Normal,
    /// A form that is generally cursive in nature.
    Italic,
    /// A typically-sloped version of the regular face.
    Oblique,
}

impl Default for Style {
    fn default() -> Style {
        Style::Normal
    }
}

impl From<Style> for fkStyle {
    fn from(style: Style) -> Self {
        match style {
            Style::Normal => fkStyle::Normal,
            Style::Italic => fkStyle::Italic,
            Style::Oblique => fkStyle::Oblique,
        }
    }
}

/// The degree of blackness or stroke thickness of a font
///
/// This value ranges from 100.0 to 900.0. The default value is 400.0 "normal".
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Weight(pub f32);

impl Default for Weight {
    #[inline]
    fn default() -> Weight {
        Weight::NORMAL
    }
}

impl Weight {
    /// Thin weight (100), the thinnest value.
    pub const THIN: Weight = Weight(100.0);
    /// Extra light weight (200).
    pub const EXTRA_LIGHT: Weight = Weight(200.0);
    /// Light weight (300).
    pub const LIGHT: Weight = Weight(300.0);
    /// Normal (400).
    pub const NORMAL: Weight = Weight(400.0);
    /// Medium weight (500, higher than normal).
    pub const MEDIUM: Weight = Weight(500.0);
    /// Semibold weight (600).
    pub const SEMIBOLD: Weight = Weight(600.0);
    /// Bold weight (700).
    pub const BOLD: Weight = Weight(700.0);
    /// Extra-bold weight (800).
    pub const EXTRA_BOLD: Weight = Weight(800.0);
    /// Black weight (900), the thickest value.
    pub const BLACK: Weight = Weight(900.0);
}

/// The width of a font as an approximate fraction of the normal width
///
/// Widths range from 0.5 to 2.0 inclusive, with 1.0 as the normal width.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Stretch(pub f32);

impl Default for Stretch {
    #[inline]
    fn default() -> Stretch {
        Stretch::NORMAL
    }
}

impl Stretch {
    /// Ultra-condensed width (50%), the narrowest possible.
    pub const ULTRA_CONDENSED: Stretch = Stretch(0.5);
    /// Extra-condensed width (62.5%).
    pub const EXTRA_CONDENSED: Stretch = Stretch(0.625);
    /// Condensed width (75%).
    pub const CONDENSED: Stretch = Stretch(0.75);
    /// Semi-condensed width (87.5%).
    pub const SEMI_CONDENSED: Stretch = Stretch(0.875);
    /// Normal width (100%).
    pub const NORMAL: Stretch = Stretch(1.0);
    /// Semi-expanded width (112.5%).
    pub const SEMI_EXPANDED: Stretch = Stretch(1.125);
    /// Expanded width (125%).
    pub const EXPANDED: Stretch = Stretch(1.25);
    /// Extra-expanded width (150%).
    pub const EXTRA_EXPANDED: Stretch = Stretch(1.5);
    /// Ultra-expanded width (200%), the widest possible.
    pub const ULTRA_EXPANDED: Stretch = Stretch(2.0);
}

thread_local! {
    // This type is not Send, so we cannot store in a Mutex within lazy_static.
    // TODO: avoid multiple instances, since initialisation may be slow.
    static SOURCE: SystemSource = SystemSource::new();
}
