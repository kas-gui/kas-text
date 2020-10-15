// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Parsers for formatted text

use crate::env::Environment;
use crate::fonts::FontId;

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown")]
pub use markdown::Markdown;

/// Text, optionally with formatting data
///
/// Note: this trait always returns a boxed dyn iterator. This allows usage in
/// non-generic types and carries only a small
/// usage penalty (since formatting data is not accessed frequently).
/// To support generating non-dyn iterators would require use of generic
/// associated types (an unstable nightly feature) to attach the lifetime
/// restriction on the iterator or unsafe code to ignore it.
pub trait FormattableText: std::fmt::Debug {
    /// Produce a boxed clone of self
    fn clone_boxed(&self) -> Box<dyn FormattableText>;

    /// Length of text
    #[inline]
    fn str_len(&self) -> usize {
        self.as_str().len()
    }

    /// Access whole text as contiguous `str`
    fn as_str(&self) -> &str;

    /// Construct an iterator over formatting items
    ///
    /// It is expected that [`FontToken::start`] of yielded items is strictly
    /// increasing; if not, formatting may not be applied correctly.
    ///
    /// For plain text this iterator will be empty.
    fn font_tokens<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = FontToken> + 'a>;
}

/// Extension of [`FormattableText`] allowing editing
pub trait EditableText: FormattableText {
    /// Set unformatted text
    ///
    /// Existing contents and formatting are replaced entirely.
    fn set_string(&mut self, string: String);

    /// Swap the contiguous unformatted text with another `string`
    ///
    /// Any formatting present is removed.
    fn swap_string(&mut self, string: &mut String) {
        let mut temp = self.as_str().to_string();
        std::mem::swap(&mut temp, string);
        self.set_string(temp);
    }

    /// Insert a `char` at the given position
    ///
    /// Formatting is adjusted such that it still affects the same chars (i.e.
    /// all formatting after `index` is postponed by the length of the char).
    fn insert_char(&mut self, index: usize, c: char);

    /// Replace text at `range` with `replace_with`
    ///
    /// Formatting is adjusted such that it still affects the same chars.
    fn replace_range(&mut self, start: usize, end: usize, replace_with: &str);
}

impl<'t> FormattableText for &'t str {
    fn clone_boxed(&self) -> Box<dyn FormattableText> {
        // TODO(specialization): for 'static we can simply use this:
        // Box::new(*self)
        Box::new(self.to_string())
    }

    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens<'a>(&'a self, _: &'a Environment) -> Box<dyn Iterator<Item = FontToken> + 'a> {
        Box::new(std::iter::empty())
    }
}

impl FormattableText for String {
    fn clone_boxed(&self) -> Box<dyn FormattableText> {
        Box::new(self.clone())
    }

    fn as_str(&self) -> &str {
        self
    }

    fn font_tokens<'a>(&'a self, _: &'a Environment) -> Box<dyn Iterator<Item = FontToken> + 'a> {
        Box::new(std::iter::empty())
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

    fn replace_range(&mut self, start: usize, end: usize, replace_with: &str) {
        self.replace_range(start..end, replace_with);
    }
}

impl Clone for Box<dyn FormattableText> {
    fn clone(&self) -> Self {
        (**self).clone_boxed()
    }
}

/// Font formatting token
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontToken {
    /// Index in text at which formatting becomes active
    ///
    /// (Note that we use `u32` not `usize` since it can be assumed text length
    /// will never exeed `u32::MAX`.)
    pub start: u32,
    /// Font identifier
    pub font_id: FontId,
    /// Font size, in dots-per-em (pixel width of an 'M')
    ///
    /// This may be calculated from point size as `pt_size * dpp`, where `dpp`
    /// is the number of pixels per point ([`Environment::dpp`]).
    pub dpem: f32,
}
