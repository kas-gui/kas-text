// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Implementations for plain text

use super::{EditableText, FontToken, FormattableText};
use crate::Effect;

impl<'t> FormattableText for &'t str {
    type FontTokenIter<'a> = std::iter::Empty<FontToken>
    where
        Self: 'a;

    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens<'a>(&'a self, _: f32) -> Self::FontTokenIter<'a> {
        std::iter::empty()
    }

    fn effect_tokens(&self) -> &[Effect<()>] {
        &[]
    }
}

impl FormattableText for String {
    type FontTokenIter<'a> = std::iter::Empty<FontToken>;

    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens<'a>(&'a self, _: f32) -> Self::FontTokenIter<'a> {
        std::iter::empty()
    }

    fn effect_tokens(&self) -> &[Effect<()>] {
        &[]
    }
}

impl EditableText for String {
    fn set_string(&mut self, string: String) {
        *self = string;
    }

    fn swap_string(&mut self, string: &mut String) {
        std::mem::swap(self, string);
    }

    fn insert_char(&mut self, index: usize, c: char) {
        self.insert(index, c);
    }

    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.replace_range(range, replace_with);
    }
}
