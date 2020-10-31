// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS-text: text layout library
//!
//! KAS-text supports plain text input, custom formatted text objects (see the
//! [`format`] module) and a subset of Markdown ([`format::Markdown`]).
//!
//! This library supports the following feature flags:
//!
//! -   `shaping`: enable complex text shaping via the Harfbuzz library (this is
//!     optional since the built-in alternative works sufficiently well for most
//!     languages)
//! -   `markdown`: enable Markdown parsing via `pulldown-cmark`
//! -   `gat`: experimental API improvements using Generic Associated Types;
//!     since Rust's support for this is incomplete, it should be considered no
//!     more than an API preview and usage is not recommended in practice
//!
//! [`format`]: mod@format

#![cfg_attr(doc_cfg, feature(doc_cfg))]
#![cfg_attr(feature = "gat", feature(generic_associated_types))]

mod env;
pub use env::*;

pub mod conv;

mod data;
pub use data::{Range, Vec2};

mod display;
pub use display::*;

pub mod fonts;
pub mod format;

mod text;
pub use text::*;

mod util;
pub use util::{Action, OwningVecIter};

pub(crate) mod shaper;
pub use shaper::{Glyph, GlyphId};
