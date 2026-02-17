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
    /// Access whole text as contiguous `str`
    fn as_str(&self) -> &str;

    /// Return an iterator of font tokens
    ///
    /// These tokens are used to select the font and font size.
    /// Each text object has a configured
    /// [font size][crate::Text::set_font_size] and [`FontSelector`]; these
    /// values are passed as a reference (`dpem` and `font`).
    ///
    /// The iterator is expected to yield a non-empty stream of tokens such that
    /// the first [`FontToken::start`] value is `0` and remaining `start` values
    /// are strictly increasing, with each value being less than
    /// `self.as_str().len()` and at `char` boundaries (i.e. an index value
    /// returned by [`str::char_indices`]. In case the returned iterator is
    /// empty or the first [`FontToken::start`] value is greater than zero the
    /// reference `dpem` and `font` values are used.
    ///
    /// Any changes to the result of this method require full re-preparation of
    /// text since this affects run breaking and font resolution.
    fn font_tokens(&self, dpem: f32, font: FontSelector) -> impl Iterator<Item = FontToken>;

    /// Return the sequence of effect tokens
    ///
    /// The `effects` sequence may be used for rendering effects: glyph color,
    /// background color, strike-through, underline. Use `&[]` for no effects
    /// (effectively using [`Effect::default()`] everywhere), or use a sequence
    /// such that `effects[i].0` values are strictly increasing. A glyph for
    /// index `j` in the source text will use effect `effects[i].1` where `i` is
    /// the largest value such that `effects[i].0 <= j`, or
    /// [`Effect::default()`] if no such `i` exists.
    ///
    /// Changes to the result of this method do not require any re-preparation
    /// of text.
    fn effect_tokens(&self) -> &[(u32, Effect)];
}

impl<F: FormattableText + ?Sized> FormattableText for &F {
    fn as_str(&self) -> &str {
        F::as_str(self)
    }

    fn font_tokens(&self, dpem: f32, font: FontSelector) -> impl Iterator<Item = FontToken> {
        F::font_tokens(self, dpem, font)
    }

    fn effect_tokens(&self) -> &[(u32, Effect)] {
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
