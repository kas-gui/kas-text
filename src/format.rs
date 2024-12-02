// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Parsers for formatted text

use crate::fonts::FontId;
use crate::{Effect, OwningVecIter};
#[allow(unused)]
use crate::{Text, TextDisplay}; // for doc-links

mod plain;

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown")]
pub use markdown::{Error as MarkdownError, Markdown};

/// Text, optionally with formatting data
///
/// Any `F: FormattableText` automatically support [`FormattableTextDyn`].
/// Implement either this or [`FormattableTextDyn`], not both.
pub trait FormattableText: std::cmp::PartialEq + std::fmt::Debug {
    type FontTokenIter<'a>: Iterator<Item = FontToken>
    where
        Self: 'a;

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
    /// The default [font size][crate::Text::set_font_size] (`dpem`) is passed
    /// as a reference.
    ///
    /// For plain text this iterator will be empty.
    fn font_tokens<'a>(&'a self, dpem: f32) -> Self::FontTokenIter<'a>;

    /// Get the sequence of effect tokens
    ///
    /// This method has some limitations: (1) it may only return a reference to
    /// an existing sequence, (2) effect tokens cannot be generated dependent
    /// on input state, and (3) it does not incorporate color information. For
    /// most uses it should still be sufficient, but for other cases it may be
    /// preferable not to use this method (use a dummy implementation returning
    /// `&[]` and use inherent methods on the text object via [`Text::text`]).
    fn effect_tokens(&self) -> &[Effect<()>];
}

/// Text, optionally with formatting data
///
/// This is an object-safe version of the [`FormattableText`] trait (i.e.
/// `dyn FormattableTextDyn` is a valid type).
///
/// This trait is auto-implemented for every implementation of [`FormattableText`].
/// The type `&dyn FormattableTextDyn` implements [`FormattableText`].
/// Implement either this or (preferably) [`FormattableText`], not both.
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
    /// The default [font size][crate::Text::set_font_size] (`dpem`) is passed
    /// as a reference.
    ///
    /// For plain text this iterator will be empty.
    fn font_tokens(&self, dpem: f32) -> OwningVecIter<FontToken>;

    /// Get the sequence of effect tokens
    ///
    /// This method has some limitations: (1) it may only return a reference to
    /// an existing sequence, (2) effect tokens cannot be generated dependent
    /// on input state, and (3) it does not incorporate color information. For
    /// most uses it should still be sufficient, but for other cases it may be
    /// preferable not to use this method (use a dummy implementation returning
    /// `&[]` and use inherent methods on the text object via [`Text::text`]).
    fn effect_tokens(&self) -> &[Effect<()>];
}

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

    fn font_tokens(&self, dpem: f32) -> OwningVecIter<FontToken> {
        let iter = FormattableText::font_tokens(self, dpem);
        OwningVecIter::new(iter.collect())
    }

    fn effect_tokens(&self) -> &[Effect<()>] {
        FormattableText::effect_tokens(self)
    }
}

/// References to [`FormattableTextDyn`] always compare unequal
impl<'t> PartialEq for &'t dyn FormattableTextDyn {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl<'t> FormattableText for &'t dyn FormattableTextDyn {
    type FontTokenIter<'a>
        = OwningVecIter<FontToken>
    where
        Self: 'a;

    #[inline]
    fn str_len(&self) -> usize {
        FormattableTextDyn::str_len(*self)
    }

    #[inline]
    fn as_str(&self) -> &str {
        FormattableTextDyn::as_str(*self)
    }

    #[inline]
    fn font_tokens(&self, dpem: f32) -> OwningVecIter<FontToken> {
        FormattableTextDyn::font_tokens(*self, dpem)
    }

    fn effect_tokens(&self) -> &[Effect<()>] {
        FormattableTextDyn::effect_tokens(*self)
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
    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str);
}

impl Clone for Box<dyn FormattableTextDyn> {
    fn clone(&self) -> Self {
        (**self).clone_boxed()
    }
}

/// Font formatting token
#[derive(Clone, Debug, PartialEq)]
pub struct FontToken {
    /// Index in text at which formatting becomes active
    ///
    /// (Note that we use `u32` not `usize` since it can be assumed text length
    /// will never exceed `u32::MAX`.)
    pub start: u32,
    /// Font identifier
    pub font_id: FontId,
    /// Font size, in dots-per-em (pixel width of an 'M')
    ///
    /// This may be calculated from point size as `pt_size * dpp`, where `dpp`
    /// is the number of pixels per point (see [`crate::fonts`] documentation).
    pub dpem: f32,
}
