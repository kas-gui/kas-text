// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library

pub use glyph_brush_layout::SectionGlyph;

mod data;
pub use data::*;

mod fonts;
pub use fonts::*;

pub mod model;
#[doc(no_inline)]
pub use model::Text as ModelText;

mod prepared;
pub use prepared::*;
