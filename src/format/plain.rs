// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Implementations for plain text

use super::{FontToken, FormattableText};
use crate::Effect;

impl FormattableText for str {
    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens(&self, _: f32) -> impl Iterator<Item = FontToken> {
        std::iter::empty()
    }

    fn effect_tokens(&self) -> &[Effect] {
        &[]
    }
}

impl FormattableText for String {
    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens(&self, _: f32) -> impl Iterator<Item = FontToken> {
        std::iter::empty()
    }

    fn effect_tokens(&self) -> &[Effect] {
        &[]
    }
}
