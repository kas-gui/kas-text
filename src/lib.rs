// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library

#![cfg_attr(doc_cfg, feature(doc_cfg))]

mod env;
pub use env::*;

mod data;
pub use data::*;

pub mod fonts;
pub mod parser;

mod prepared;
pub use prepared::*;

pub(crate) mod shaper;
pub use shaper::Glyph;

/// A string with formatting information
///
/// This type supports construction from `String` and `&str` (no formatting).
/// It may also be constructed from any [`parser::Parser`].
/// ```
/// # use kas_text::FormattedString;
/// let s1 = FormattedString::from("plain text");
/// // if `markdown` feature is enabled:
/// // let s2 = FormattedString::from(Markdown::new("*Markdown* text"));
/// ```
pub struct FormattedString {
    pub(crate) text: String,
    pub(crate) fmt: Box<dyn parser::FormatData>,
}

impl FormattedString {
    /// Read contiguous unformatted text
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Extract unformatting `String`
    pub fn take_string(self) -> String {
        self.text
    }
}
