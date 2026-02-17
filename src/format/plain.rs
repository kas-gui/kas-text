// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Implementations for plain text

use super::{FontToken, FormattableText};
use crate::{Effect, fonts::FontSelector};

impl FormattableText for str {
    #[inline]
    fn as_str(&self) -> &str {
        self
    }

    #[inline]
    fn font_tokens(&self, dpem: f32, font: FontSelector) -> impl Iterator<Item = FontToken> {
        let start = 0;
        std::iter::once(FontToken { start, dpem, font })
    }

    #[inline]
    fn effect_tokens(&self) -> &[(u32, Effect)] {
        &[]
    }
}

impl FormattableText for String {
    #[inline]
    fn as_str(&self) -> &str {
        self
    }

    #[inline]
    fn font_tokens(&self, dpem: f32, font: FontSelector) -> impl Iterator<Item = FontToken> {
        let start = 0;
        std::iter::once(FontToken { start, dpem, font })
    }

    #[inline]
    fn effect_tokens(&self) -> &[(u32, Effect)] {
        &[]
    }
}
