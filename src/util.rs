// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

#[allow(unused)]
use crate::Forme;
use crate::{Direction, fonts::FontSelector};
use icu_properties::CodePointMapData;
use icu_properties::props::{EnumeratedProperty, LineBreak};
use icu_segmenter::options::LineBreakOptions;
use std::{fmt, iter, ops::Range, str};
use unicode_bidi::{BidiInfo, LTR_LEVEL, Level, ParagraphInfo, RTL_LEVEL};

/// Describes the [state-of-preparation](Forme#states-of-preparation) of a [`Forme`]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Status {
    /// An empty [`Forme`].
    #[default]
    Empty,
    /// A [`Forme`] with [set text](Forme::set_text).
    Shaped,
    /// A [`Forme`] with [prepared lines](Forme::prepare_lines).
    Wrapped,
    /// A [`Forme`] that is ready for display.
    Ready,
}

impl Status {
    /// True if status is `Status::Ready`
    #[inline]
    pub fn is_ready(&self) -> bool {
        *self == Status::Ready
    }
}

/// Font formatting token
#[derive(Clone, Debug, PartialEq)]
pub struct FontToken {
    /// Index in text at which formatting becomes active
    ///
    /// Expected: `start <= text.len()`. (Note: text ending with a mandatory
    /// break implies a following new-line, at least in some cases.)
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

/// Analyzer for text direction
///
/// This is typically not stored but computed on use for one or multiple
/// paragraphs of text (see [`Self::new`] docs).
pub(crate) struct AnalyzedText<'a> {
    text: &'a str,
    default_level: Level,
    levels: Vec<Level>,
    paragraphs: Vec<ParagraphInfo>,
    pub(crate) lb_opts: LineBreakOptions<'a>,
}

impl<'a> std::ops::Deref for AnalyzedText<'a> {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.text
    }
}

impl<'a> AnalyzedText<'a> {
    /// Analyze a `text`
    ///
    /// For correct analysis, `text` should be one or more whole paragraphs
    /// (i.e. up to and optionally including a mandatory line break).
    /// It is valid to analyze a multi-paragraph text as a whole or to split
    /// the text into sub-ranges as yielded by [`LineRanges`] and analyze
    /// each sub-range separately.
    pub fn new(text: &'a str, direction: Direction) -> Self {
        let default_para_level = match direction {
            Direction::Auto => None,
            Direction::AutoRtl => {
                use unicode_bidi::Direction::*;
                match unicode_bidi::get_base_direction(text) {
                    Ltr | Rtl => None,
                    Mixed => Some(RTL_LEVEL),
                }
            }
            Direction::Ltr => Some(LTR_LEVEL),
            Direction::Rtl => Some(RTL_LEVEL),
        };

        let info = BidiInfo::new(text, default_para_level);
        assert_eq!(text.len(), info.levels.len());

        AnalyzedText {
            text,
            default_level: direction.level(),
            levels: info.levels,
            paragraphs: info.paragraphs,
            lb_opts: LineBreakOptions::default(),
        }
    }

    /// Get the default [`Level`]
    #[inline]
    pub(crate) fn default_level(&self) -> Level {
        self.default_level
    }

    /// Get the [`Level`] at the given text index
    #[inline]
    pub(crate) fn level(&self, index: usize) -> Option<Level> {
        self.levels.get(index).copied()
    }

    /// Get paragraph info for the given paragraph index
    #[inline]
    pub(crate) fn paragraph(&self, index: usize) -> Option<&ParagraphInfo> {
        self.paragraphs.get(index)
    }

    /// Find the index of the paragraph containing the given text `index`
    pub(crate) fn find_paragraph(&self, index: usize) -> usize {
        match self
            .paragraphs
            .binary_search_by_key(&index, |para| para.range.start)
        {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

/// Returns `true` when `text` ends with a mandatory break
pub(crate) fn ends_with_hard_break(text: &str) -> bool {
    // This filter is copied from icu_segmenter docs.
    text.chars().next_back().is_some_and(|c| {
        matches!(
            CodePointMapData::<LineBreak>::new().get(c),
            LineBreak::MandatoryBreak
                | LineBreak::CarriageReturn
                | LineBreak::LineFeed
                | LineBreak::NextLine
        )
    })
}

/// Types of line break
//
// Note: this uses a null-terminated UTF-8 encoding internally.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LineBreakEncoding([u8; 4]);

impl LineBreakEncoding {
    /// Carriage Return + Line Feed: `\r\n`
    pub const CR_LF: Self = LineBreakEncoding(*"\r\n\0\0".as_bytes().as_array().unwrap());

    /// Line Feed: `\n`
    pub const LF: Self = LineBreakEncoding(*"\n\0\0\0".as_bytes().as_array().unwrap());

    /// Vertical Tab
    pub const VT: Self = LineBreakEncoding(*"\x0B\0\0\0".as_bytes().as_array().unwrap());

    /// Form Feed
    pub const FF: Self = LineBreakEncoding(*"\x0C\0\0\0".as_bytes().as_array().unwrap());

    /// Carriage Return: `\r`
    pub const CR: Self = LineBreakEncoding(*"\r\0\0\0".as_bytes().as_array().unwrap());

    /// Unicode Next Line
    pub const NEL: Self = LineBreakEncoding(*"\u{85}\0\0".as_bytes().as_array().unwrap());

    /// Unicode Line Separator
    pub const LS: Self = LineBreakEncoding(*"\u{2028}\0".as_bytes().as_array().unwrap());

    /// Unicode Paragraph Separator
    pub const PS: Self = LineBreakEncoding(*"\u{2029}\0".as_bytes().as_array().unwrap());

    /// Get UTF-8 encoding
    pub fn as_str(&self) -> &str {
        let mut end = 4;
        for i in 0..4 {
            if self.0[i] == b'\0' {
                end = i;
                break;
            }
        }

        // SAFETY: contents of self are always valid ASCII; chopping off
        // trailing zero bytes leaves valid UTF-8
        unsafe { str::from_utf8_unchecked(&self.0[..end]) }
    }
}

impl From<char> for LineBreakEncoding {
    fn from(c: char) -> Self {
        let mut buf = [0u8; 4];
        c.encode_utf8(&mut buf);
        LineBreakEncoding(buf)
    }
}

impl fmt::Debug for LineBreakEncoding {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

trait LineBreakExt {
    fn is_mandatory_break(&self) -> bool;
}
impl LineBreakExt for LineBreak {
    fn is_mandatory_break(&self) -> bool {
        *self == LineBreak::BK
            || *self == LineBreak::CR
            || *self == LineBreak::LF
            || *self == LineBreak::NL
    }
}

/// Iterator over lines / paragraphs within the text
///
/// This iterator splits the input text into a sequence of "lines" at mandatory
/// breaks (see [TR14#BK](https://www.unicode.org/reports/tr14/#BK)) and returns
/// the range of each line within the input `text`.
pub struct LineRanges<'a> {
    iter: iter::Peekable<str::CharIndices<'a>>,
    text: &'a str,
    start: usize,
}

impl<'a> LineRanges<'a> {
    /// Construct
    #[inline]
    pub fn new(text: &'a str) -> Self {
        LineRanges {
            iter: text.char_indices().peekable(),
            text,
            start: 0,
        }
    }
}

impl<'a> Iterator for LineRanges<'a> {
    type Item = (Range<usize>, Option<LineBreakEncoding>);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((i, c)) = self.iter.next() {
            let lb = LineBreak::for_char(c);

            if lb.is_mandatory_break() {
                let encoding;
                let end = if lb == LineBreak::CR
                    && self.iter.peek().map(|(_, c)| LineBreak::for_char(*c)) == Some(LineBreak::LF)
                {
                    encoding = LineBreakEncoding::CR_LF;
                    let (i, c) = self.iter.next().unwrap();
                    i + c.len_utf8()
                } else {
                    encoding = LineBreakEncoding::from(c);
                    i + c.len_utf8()
                };

                let range = self.start..i;
                self.start = end;
                return Some((range, Some(encoding)));
            }
        }

        if self.start <= self.text.len() {
            let range = self.start..self.text.len();
            self.start = range.end + 1;
            return Some((range, None));
        }

        None
    }
}

/// Iterator over lines / paragraphs within the text
///
/// This iterator splits the input text into a sequence of "lines" at mandatory
/// breaks (see [TR14#BK](https://www.unicode.org/reports/tr14/#BK)).
///
/// This is a shim over [`LineRanges`] mapping ranges to `str` slices.
pub struct Lines<'a>(LineRanges<'a>);

impl<'a> Lines<'a> {
    /// Construct
    #[inline]
    pub fn new(text: &'a str) -> Self {
        Lines(LineRanges::new(text))
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = (&'a str, Option<LineBreakEncoding>);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|(range, encoding)| (&self.0.text[range], encoding))
    }
}

pub(crate) fn icu_script_as_raw_tag(script: icu_properties::props::Script) -> [u8; 4] {
    let script = icu_locale::subtags::Script::from(script);
    script.into_raw()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn line_range_iter() {
        let mut iter = LineRanges::new("");
        assert_eq!(iter.next(), Some((0..0, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("\n");
        assert_eq!(iter.next(), Some((0..0, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((1..1, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("\r\n");
        assert_eq!(iter.next(), Some((0..0, Some(LineBreakEncoding::CR_LF))));
        assert_eq!(iter.next(), Some((2..2, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("\n\r");
        assert_eq!(iter.next(), Some((0..0, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((1..1, Some(LineBreakEncoding::CR))));
        assert_eq!(iter.next(), Some((2..2, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("\r\r");
        assert_eq!(iter.next(), Some((0..0, Some(LineBreakEncoding::CR))));
        assert_eq!(iter.next(), Some((1..1, Some(LineBreakEncoding::CR))));
        assert_eq!(iter.next(), Some((2..2, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("abc def");
        assert_eq!(iter.next(), Some((0..7, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("abc\n\ndef");
        assert_eq!(iter.next(), Some((0..3, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((4..4, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((5..8, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("abc def\nghi\n");
        assert_eq!(iter.next(), Some((0..7, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((8..11, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((12..12, None)));
        assert_eq!(iter.next(), None);

        let mut iter = LineRanges::new("abc\rdef\nghi\r\njkl\u{85}mno");
        assert_eq!(iter.next(), Some((0..3, Some(LineBreakEncoding::CR))));
        assert_eq!(iter.next(), Some((4..7, Some(LineBreakEncoding::LF))));
        assert_eq!(iter.next(), Some((8..11, Some(LineBreakEncoding::CR_LF))));
        assert_eq!(iter.next(), Some((13..16, Some(LineBreakEncoding::NEL))));
        assert_eq!(iter.next(), Some((18..21, None)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn line_iter() {
        let mut iter = Lines::new("");
        assert_eq!(iter.next(), Some(("", None)));
        assert_eq!(iter.next(), None);

        let mut iter = Lines::new("abc\r\ndef");
        assert_eq!(iter.next(), Some(("abc", Some(LineBreakEncoding::CR_LF))));
        assert_eq!(iter.next(), Some(("def", None)));
        assert_eq!(iter.next(), None);
    }
}
