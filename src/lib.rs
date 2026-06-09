// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! # Kas-text: font library and text engine
//!
//! ## Font library
//!
//! The [`fonts`] module represents a "library" tracking discovered fonts along
//! with helpers for font selection and reading font properties.
//!
//! ## Text engine
//!
//! The [`Forme`] struct is able to transform an `&str` into a set of type-set
//! glyphs, and rapidly re-flow these glyphs to meet any page width.
//!
//! ## Formatted text
//!
//! The [`Text`] struct provides a slightly higher-level API, using the
//! [`format`](mod@format) module to control formatting of content.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(text_direction_codepoint_in_literal)]

mod env;
pub use env::*;

mod conv;
pub use conv::{DPU, LineMetrics};

mod data;
use data::Range;
pub use data::Vec2;

mod forme;
pub use forme::*;

pub mod fonts;
#[cfg(feature = "text")]
pub mod format;

#[cfg(feature = "text")]
mod text;
#[cfg(feature = "text")]
pub use text::*;

mod util;
pub use util::{FontToken, LineIterator, Status};

pub(crate) mod shaper;
pub use shaper::{Glyph, GlyphId};
