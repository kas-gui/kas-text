// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library

#![cfg_attr(feature = "gat", feature(generic_associated_types))]
#![cfg_attr(doc_cfg, feature(doc_cfg))]

pub use ab_glyph::PxScale;

mod env;
pub use env::*;

mod data;
pub use data::*;

pub mod fonts;
pub mod prepared;
pub mod rich;
pub mod text;

pub(crate) mod shaper;
pub use shaper::Glyph;
