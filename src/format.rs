// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Parsers for formatted text

use crate::Effect;
use crate::fonts::FontSelector;
#[allow(unused)]
use crate::{Text, TextDisplay}; // for doc-links

mod plain;

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown")]
pub use markdown::{Error as MarkdownError, Markdown};

/// Text, optionally with formatting data
pub trait FormattableText: std::cmp::PartialEq {
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
    fn font_tokens(&self, dpem: f32) -> impl Iterator<Item = FontToken>;

    /// Return the sequence of effect tokens
    ///
    /// These tokens are used to select the font color and
    /// [effects](crate::EffectFlags).
    ///
    /// The values of [`Effect::start`] are expected to be strictly increasing
    /// in order, less than [`Self::str_len`]. In case the slice is empty or the
    /// first [`Effect::start`] value is greater than zero, values from
    /// [`Effect::default()`] are used.
    ///
    /// Changes to the result of this method do not require any re-preparation
    /// of text.
    fn effect_tokens(&self) -> &[Effect];
}

impl<F: FormattableText + ?Sized> FormattableText for &F {
    fn as_str(&self) -> &str {
        F::as_str(self)
    }

    fn font_tokens(&self, dpem: f32) -> impl Iterator<Item = FontToken> {
        F::font_tokens(self, dpem)
    }

    fn effect_tokens(&self) -> &[Effect] {
        F::effect_tokens(self)
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
    /// Font size, in dots-per-em (pixel width of an 'M')
    ///
    /// This may be calculated from point size as `pt_size * dpp`, where `dpp`
    /// is the number of pixels per point (see [`crate::fonts`] documentation).
    pub dpem: f32,
    /// Font selector
    pub font: FontSelector,
}
