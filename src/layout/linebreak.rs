// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Simple layout engine — copied from glyph_brush_layout

use std::{fmt, hash::Hash, str};

/// Indicator that a character is a line break, soft or hard. Includes the offset (byte-index)
/// position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineBreak {
    /// Soft line break (offset).
    Soft(usize),
    /// Hard line break (offset).
    Hard(usize),
}

impl LineBreak {
    /// Returns the offset of the line break, the index after the breaking character.
    #[inline]
    pub fn offset(&self) -> usize {
        match *self {
            LineBreak::Soft(offset) | LineBreak::Hard(offset) => offset,
        }
    }
}

/// Producer of a [`LineBreak`](enum.LineBreak.html) iterator. Used to allow to the
/// [`Layout`](enum.Layout.html) to be line break aware in a generic way.
pub trait LineBreaker: fmt::Debug + Copy + Hash {
    fn line_breaks<'a>(&self, glyph_info: &'a str) -> Box<dyn Iterator<Item = LineBreak> + 'a>;
}

/// Built-in linebreaking logic.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum BuiltInLineBreaker {
    /// LineBreaker that follows Unicode Standard Annex #14. That effectively means it
    /// wraps words in a way that should work for most cases.
    UnicodeLineBreaker,
}

impl Default for BuiltInLineBreaker {
    #[inline]
    fn default() -> Self {
        BuiltInLineBreaker::UnicodeLineBreaker
    }
}

impl LineBreaker for BuiltInLineBreaker {
    #[inline]
    fn line_breaks<'a>(&self, text: &'a str) -> Box<dyn Iterator<Item = LineBreak> + 'a> {
        match *self {
            BuiltInLineBreaker::UnicodeLineBreaker => Box::new(
                xi_unicode::LineBreakIterator::new(text).map(|(offset, hard)| {
                    if hard {
                        LineBreak::Hard(offset)
                    } else {
                        LineBreak::Soft(offset)
                    }
                }),
            ),
        }
    }
}

/// Line breakers can't easily tell the difference between the end of a slice being a hard
/// break and the last character being itself a hard or soft break. This trait allows testing
/// of eol characters being "true" eol line breakers.
pub(crate) trait EolLineBreak<B: LineBreaker> {
    fn eol_line_break(&self, line_breaker: &B) -> Option<LineBreak>;

    #[inline]
    fn is_eol_hard_break(&self, line_breaker: &B) -> bool {
        if let Some(LineBreak::Hard(..)) = self.eol_line_break(line_breaker) {
            return true;
        }
        false
    }

    #[inline]
    fn is_eol_soft_break(&self, line_breaker: &B) -> bool {
        if let Some(LineBreak::Soft(..)) = self.eol_line_break(line_breaker) {
            return true;
        }
        false
    }
}

impl<B: LineBreaker> EolLineBreak<B> for char {
    #[inline]
    fn eol_line_break(&self, line_breaker: &B) -> Option<LineBreak> {
        // to check if the previous end char (say '$') should hard break construct
        // a str "$ " an check if the line break logic flags a hard break at index 1
        let mut last_end_bytes: [u8; 5] = [b' '; 5];
        self.encode_utf8(&mut last_end_bytes);
        let len_utf8 = self.len_utf8();
        if let Ok(last_end_padded) = str::from_utf8(&last_end_bytes[0..=len_utf8]) {
            match line_breaker.line_breaks(last_end_padded).next() {
                l @ Some(LineBreak::Soft(1)) | l @ Some(LineBreak::Hard(1)) => return l,
                _ => {}
            }
        }

        // check for soft breaks using str "$a"
        last_end_bytes[len_utf8] = b'a';
        if let Ok(last_end_padded) = str::from_utf8(&last_end_bytes[0..=len_utf8]) {
            match line_breaker.line_breaks(last_end_padded).next() {
                l @ Some(LineBreak::Soft(1)) | l @ Some(LineBreak::Hard(1)) => return l,
                _ => {}
            }
        }

        None
    }
}

#[cfg(test)]
mod eol_line_break {
    use super::*;

    #[test]
    fn hard_break_char() {
        assert_eq!(
            '\n'.eol_line_break(&BuiltInLineBreaker::default()),
            Some(LineBreak::Hard(1))
        );
    }

    #[test]
    fn soft_break_char() {
        assert_eq!(
            ' '.eol_line_break(&BuiltInLineBreaker::default()),
            Some(LineBreak::Soft(1))
        );
    }
}
