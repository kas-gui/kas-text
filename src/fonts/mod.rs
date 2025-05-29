// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Font selection and loading
//!
//! Fonts are managed by the [`FontLibrary`], of which a static singleton
//! exists and can be accessed via [`library()`].
//!
//! ### `FontId` and the default font
//!
//! The [`FontId`] type is a numeric identifier for a selected font. It may be
//! default-constructed to access the *default* font, with number 0.
//!
//! To make this work, the user of this library *must* load the default font
//! before all other fonts and before any operation requiring font metrics:
//! ```
//! if let Err(e) = kas_text::fonts::library().init() {
//!     panic!("Error loading font: {}", e);
//! }
//! // from now on, kas_text::fonts::FontId::default() identifies the default font
//! ```
//!
//! ### `FaceId` vs `FontId`
//!
//! Why do both [`FaceId`] and [`FontId`] exist? Font fallbacks. A [`FontId`]
//! identifies a list of font faces; when selecting glyphs the first face which
//! includes that glyph is selected. Thus, when iterating over glyphs for
//! rendering purposes, each has an associated [`FaceId`].
//!
//! ### Font sizes
//!
//! Typically, font sizes are specified in "Points". Several other units and
//! measures come into play here. Lets start with those dating back to the
//! early printing press:
//!
//! -   1 *Point* = 1/72 inch (~0.35mm), by the usual DTP standard
//! -   1 *Em* is the width of a capital `M` (inclusive of margin) in a font
//! -   The *point size* of a font refers to the number of *points* per *em*
//!     in this font
//!
//! Thus, with a "12 point font", one 'M' occupies 12/72 of an inch on paper.
//!
//! In digital typography, one must translate to/from pixel sizes. Here we have:
//!
//! -   DPI (Dots Per Inch) is the number of pixels per inch
//! -   A *scale factor* is a measure of the number of pixels relative to a
//!     standard DPI, usually 96
//!
//! We introduce two measures used by this library:
//!
//! -   DPP (Dots Per Point): `dpp = dpi / 72 = scale_factor × (96 / 72)`
//! -   DPEM (Dots Per Em): `dpem = point_size × dpp`
//!
//! Warning: on MacOS and Apple systems, a *point* sometimes refers to a
//! (virtual) pixel, yielding `dpp = 1` (or `dpp = 2` on Retina screens, or
//! something else entirely on iPhones). On any system, DPI/DPP/scale factor
//! values may be set according to the user's taste rather than physical
//! measurements.
//!
//! Finally, note that digital font files have an internally defined unit
//! known as the *font unit*. We introduce one final unit:
//!
//! -   [`crate::DPU`]: pixels per font unit

use crate::GlyphId;

mod attributes;
mod face;
mod library;
mod resolver;

pub use attributes::{FontStyle, FontWeight, FontWidth};
pub use face::{FaceRef, ScaledFaceRef};
pub use fontique::GenericFamily;
pub use library::{library, FaceId, FaceStore, FontId, FontLibrary, InvalidFontId, NoFontMatch};
pub use resolver::*;

impl From<GlyphId> for ttf_parser::GlyphId {
    fn from(id: GlyphId) -> Self {
        ttf_parser::GlyphId(id.0)
    }
}
