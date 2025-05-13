// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS-text: text layout library
//!
//! KAS-text supports plain text input, custom formatted text objects (see the
//! [`format`] module) and a subset of Markdown ([`format::Markdown`],
//! feature-gated).
//!
//! The library also supports glyph rastering (depending on feature flags).
//!
//! [`format`]: mod@format

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub extern crate fontdb;

mod env;
pub use env::*;

mod conv;
pub use conv::DPU;

mod data;
pub use data::{Range, Vec2};

mod display;
pub use display::*;

pub mod fonts;
pub mod format;

#[cfg(any(feature = "ab_glyph", feature = "fontdue"))]
pub mod raster;

mod text;
pub use text::*;

mod util;
pub use util::{OwningVecIter, Status};

pub(crate) mod shaper;
pub use shaper::{Glyph, GlyphId};
