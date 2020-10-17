// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library

#![cfg_attr(doc_cfg, feature(doc_cfg))]

mod env;
pub use env::*;

pub mod conv;

mod data;
pub use data::{Range, Vec2};

mod display;
pub use display::{Effect, EffectFlags, PrepareAction, TextDisplay};

pub mod fonts;
pub mod format;

mod text;
pub use text::{Text, TextApi};

pub(crate) mod shaper;
pub use shaper::{Glyph, GlyphId};
