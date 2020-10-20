// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Parsers for formatted text

use crate::fonts::FontId;
use crate::{Environment, OwningVecIter};

mod plain;

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown")]
pub use markdown::Markdown;

/// Text, optionally with formatting data
///
/// Any `F: FormattableText` automatically support [`FormattableTextDyn`].
/// Implement either this or [`FormattableTextDyn`], not both.
///
/// This trait can only be written as intended using Generic Associated Types
/// ("gat", unstable nightly feature), thus `font_tokens` has a different
/// signature with/without feature `gat` and the associated type
/// `FontTokenIterator` is only present with feature `gat`.
pub trait FormattableText: std::fmt::Debug {
    #[cfg_attr(doc_cfg, doc(cfg(feature = "gat")))]
    #[cfg(feature = "gat")]
    type FontTokenIterator<'a>: Iterator<Item = FontToken>;

    /// Length of text
    ///
    /// Default implementation uses [`FormattableText::as_str`].
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
    #[cfg(feature = "gat")]
    fn font_tokens<'a>(&'a self, env: &'a Environment) -> Self::FontTokenIterator<'a>;

    /// Construct an iterator over formatting items
    ///
    /// It is expected that [`FontToken::start`] of yielded items is strictly
    /// increasing; if not, formatting may not be applied correctly.
    ///
    /// For plain text this iterator will be empty.
    #[cfg(not(feature = "gat"))]
    fn font_tokens(&self, env: &Environment) -> OwningVecIter<FontToken>;
}

/// Text, optionally with formatting data
///
/// The type `&dyn FormattableTextDyn` supports [`FormattableText`].
/// Implement either this or [`FormattableText`], not both.
pub trait FormattableTextDyn: std::fmt::Debug {
    /// Produce a boxed clone of self
    fn clone_boxed(&self) -> Box<dyn FormattableTextDyn>;

    /// Length of text
    fn str_len(&self) -> usize;

    /// Access whole text as contiguous `str`
    fn as_str(&self) -> &str;

    /// Construct an iterator over formatting items
    ///
    /// It is expected that [`FontToken::start`] of yielded items is strictly
    /// increasing; if not, formatting may not be applied correctly.
    ///
    /// For plain text this iterator will be empty.
    fn font_tokens(&self, env: &Environment) -> OwningVecIter<FontToken>;
}

// #[cfg(feature = "gat")]
impl<F: FormattableText + Clone + 'static> FormattableTextDyn for F {
    fn clone_boxed(&self) -> Box<dyn FormattableTextDyn> {
        Box::new(self.clone())
    }

    fn str_len(&self) -> usize {
        FormattableText::str_len(self)
    }
    fn as_str(&self) -> &str {
        FormattableText::as_str(self)
    }

    fn font_tokens(&self, env: &Environment) -> OwningVecIter<FontToken> {
        let iter = FormattableText::font_tokens(self, env);
        #[cfg(feature = "gat")]
        {
            OwningVecIter::new(iter.collect())
        }
        #[cfg(not(feature = "gat"))]
        {
            iter
        }
    }
}

impl<'t> FormattableText for &'t dyn FormattableTextDyn {
    #[cfg(feature = "gat")]
    type FontTokenIterator<'a> = OwningVecIter<FontToken>;

    #[inline]
    fn str_len(&self) -> usize {
        FormattableTextDyn::str_len(*self)
    }

    #[inline]
    fn as_str(&self) -> &str {
        FormattableTextDyn::as_str(*self)
    }

    #[inline]
    fn font_tokens(&self, env: &Environment) -> OwningVecIter<FontToken> {
        FormattableTextDyn::font_tokens(*self, env)
    }
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

impl Clone for Box<dyn FormattableTextDyn> {
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
    /// is the number of pixels per point (see [`crate::fonts`] documentation).
    pub dpem: f32,
}
