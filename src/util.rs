// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

use icu_properties::{CodePointMapData, props::LineBreak};
use icu_segmenter::{
    LineSegmenter, iterators::LineBreakIterator, options::LineBreakOptions, scaffold::Utf8,
};
use std::ops::Range;
use unicode_bidi::{BidiInfo, LTR_LEVEL, Level, ParagraphInfo, RTL_LEVEL};

use crate::{Direction, fonts::FontSelector};

/// Describes the state-of-preparation of a [`Forme`][crate::Forme]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Status {
    /// Nothing done yet
    #[default]
    New,
    /// As [`Self::LevelRuns`], except these need resizing
    ResizeLevelRuns,
    /// Source text has been broken into level runs
    LevelRuns,
    /// Line wrapping and horizontal alignment is done
    Wrapped,
    /// The text is ready for display
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
    /// the text into sub-ranges as yielded by [`LineIterator`] and analyze
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

/// Iterator over lines / paragraphs within the text
///
/// This iterator splits the input text into a sequence of "lines" at mandatory
/// breaks (see [TR14#BK](https://www.unicode.org/reports/tr14/#BK)).
/// The resulting slices cover the whole input text in order without overlap.
pub struct LineIterator<'a> {
    break_iter: LineBreakIterator<'static, 'a, Utf8>,
    text: &'a str,
    start: usize,
}

impl<'a> LineIterator<'a> {
    /// Construct
    #[inline]
    pub fn new(text: &'a str) -> Self {
        let segmenter = LineSegmenter::new_auto(Default::default());
        let mut break_iter = segmenter.segment_str(text);
        assert_eq!(break_iter.next(), Some(0)); // the iterator always reports a break at 0
        LineIterator {
            break_iter,
            text,
            start: 0,
        }
    }
}

impl<'a> Iterator for LineIterator<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        for index in self.break_iter.by_ref() {
            if ends_with_hard_break(&self.text[..index]) || index == self.text.len() {
                let range = self.start..index;
                self.start = index;
                return Some(range);
            }
        }
        None
    }
}

pub(crate) fn to_fontique_script(script: icu_properties::props::Script) -> fontique::Script {
    let script = icu_locale::subtags::Script::from(script);
    fontique::Script::from_bytes(script.into_raw())
}

#[cfg(feature = "rustybuzz")]
pub(crate) fn to_ttf_parser_tag(script: icu_properties::props::Script) -> ttf_parser::Tag {
    let script = icu_locale::subtags::Script::from(script);
    ttf_parser::Tag::from_bytes(&script.into_raw())
}
