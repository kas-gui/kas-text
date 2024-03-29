// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Common font names
//!
//! The purpose of this module is to select preferred fonts for each category
//! among those available on a system, with fallback options (both for missing
//! fonts and for missing glyphs).
//!
//! NOTE: these lists were put together quickly by a non-expert, and with very
//! limited testing, thus may have significant defects.
//!
//! *Probably* this module should be replaced by system-specific font
//! configuration eventually.
//!
//! Fonts are chosen based based on the following criteria:
//!
//! 1.  Included by default with at least one recent operating system
//! 2.  Appearance, both quality and being fairly standard
//!
//! Font family ordering indicates usage preference.

pub const DEFAULT_SERIF: &[&str] = &[
    "serif",
    "Palatino Linotype",
    "Palatino",
    "Georgia",
    "Droid Serif",
    "Hoefler Text",
    "Times New Roman",
    "Times",
    "Times CY",
    "DejaVu Serif",
    "Liberation Serif",
];

pub const DEFAULT_SANS_SERIF: &[&str] = &[
    "sans-serif",
    "Tahoma",
    "Noto Sans",
    "DejaVu Sans",
    "Open Sans",
    "Droid Sans",
    "Arial",
    "Arial Hebrew",
    "Verdana",
    "Vera Sans",
    "Roboto",
    "Lato",
    "Liberation Sans",
    "Helvetica",
    "Lucida Sans Unicode",
];

pub const DEFAULT_MONOSPACE: &[&str] = &[
    "monospace",
    "Consolas",
    "Droid Sans Mono",
    "Menlo",
    "Noto Mono",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Roboto Mono",
    "Monaco",
    "Monaco CY",
    "Source Code Pro",
    "Source Sans Pro",
    "Andalé Mono",
    "Andale Mono",
    "Lucida Console",
    "Liberation Mono",
    "Courier New",
    "Courier",
];

pub const DEFAULT_CURSIVE: &[&str] = &[
    "cursive",
    "Gabriola",
    "Segoe Script",
    "Candara",
    "Comic Sans MS",
];

pub const DEFAULT_FANTASY: &[&str] = &[
    "fantasy",
    "Segoe Print",
    "Impact",
    "Apple Chancery",
    "Papyrus",
];
