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
//! *Probably* this module should be replaced by system-specific font config
//! eventually.
//!
//! Fonts are chosen based based on the following criteria:
//!
//! 1.  Included by default with at least one recent operating system
//! 2.  Apparance, both quality and being fairly standard
//!
//! Font family ordering indicates usage preference.

const DEFAULT_SERIF: [&'static str; 12] = [
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
    "Jomolhari",
    "Liberation Serif",
];

const DEFAULT_SANS_SERIF: [&'static str; 16] = [
    "sans-serif",
    "Tahoma",
    "Noto Sans",
    "DejaVu Sans",
    "Open Sans",
    "Droid Sans",
    "Arial",
    "Arial Hebrew",
    "Verdana",
    "Cantarell",
    "Vera Sans",
    "Roboto",
    "Lato",
    "Liberation Sans",
    "Helvetica",
    "Lucida Sans Unicode",
];

const DEFAULT_MONOSPACE: [&'static str; 18] = [
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

const DEFAULT_CURSIVE: [&'static str; 5] = [
    "cursive",
    "Gabriola",
    "Segoe Script",
    "Candara",
    "Comic Sans MS",
];

const DEFAULT_FANTASY: [&'static str; 5] = [
    "fantasy",
    "Segoe Print",
    "Impact",
    "Apple Chancery",
    "Papyrus",
];

/// Use this to set default font families after loading fonts
pub fn set_defaults(db: &mut fontdb::Database) {
    // fontdb does not set a default font for each category, so we should do that now.
    macro_rules! set_family {
        ($lt:tt, $FAMILY:ident, $set_fn:ident) => {
            $lt: for name in $FAMILY.iter().cloned() {
                for face in db.faces() {
                    if name == face.family {
                        db.$set_fn(name);
                        break $lt;
                    }
                }
            }
        }
    }
    set_family!('a, DEFAULT_SERIF, set_serif_family);
    set_family!('b, DEFAULT_SANS_SERIF, set_sans_serif_family);
    set_family!('c, DEFAULT_MONOSPACE, set_monospace_family);
    set_family!('d, DEFAULT_CURSIVE, set_cursive_family);
    set_family!('e, DEFAULT_FANTASY, set_fantasy_family);
}
