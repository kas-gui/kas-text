// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

use icu_properties::{CodePointMapData, props::LineBreak};
use icu_segmenter::{LineSegmenter, iterators::LineBreakIterator, scaffold::Utf8};
use std::ops::Range;

/// Describes the state-of-preparation of a [`TextDisplay`][crate::TextDisplay]
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

/// An iterator over a `Vec` which clones elements
pub struct OwningVecIter<T: Clone> {
    v: Vec<T>,
    i: usize,
}

impl<T: Clone> OwningVecIter<T> {
    /// Construct from a `Vec`
    pub fn new(v: Vec<T>) -> Self {
        let i = 0;
        OwningVecIter { v, i }
    }
}

impl<T: Clone> Iterator for OwningVecIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.v.len() {
            let item = self.v[self.i].clone();
            self.i += 1;
            Some(item)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.v.len() - self.i;
        (len, Some(len))
    }
}

impl<T: Clone> ExactSizeIterator for OwningVecIter<T> {}
impl<T: Clone> std::iter::FusedIterator for OwningVecIter<T> {}

/// Returns `true` when `text` ends with a hard break, assuming that it ends
/// with a valid line break.
///
/// This filter is copied from icu_segmenter docs.
pub(crate) fn ends_with_hard_break(text: &str) -> bool {
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
        while let Some(index) = self.break_iter.next() {
            if ends_with_hard_break(&self.text[..index]) || index == self.text.len() {
                let range = self.start..index;
                self.start = index;
                return Some(range);
            }
        }
        None
    }
}
