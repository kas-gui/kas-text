// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

use std::ops::Range;
use std::str::{CharIndices, Chars};
use swash::text::cluster::Boundary;

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

/// Iterator over lines / paragraphs within the text
///
/// This iterator splits the input text into a sequence of "lines" at mandatory
/// breaks (see [TR14#BK](https://www.unicode.org/reports/tr14/#BK)).
/// The resulting slices cover the whole input text in order without overlap.
pub struct LineIterator<'a> {
    analyzer: swash::text::Analyze<Chars<'a>>,
    char_indices: CharIndices<'a>,
    start: usize,
    len: usize,
}

impl<'a> LineIterator<'a> {
    /// Construct
    #[inline]
    pub fn new(text: &'a str) -> Self {
        LineIterator {
            analyzer: swash::text::analyze(text.chars()),
            char_indices: text.char_indices(),
            start: 0,
            len: text.len(),
        }
    }
}

impl<'a> Iterator for LineIterator<'a> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.len {
            return None;
        }

        while let Some((index, _)) = self.char_indices.next() {
            let (_, boundary) = self.analyzer.next().unwrap();

            if index > 0 && boundary == Boundary::Mandatory {
                let range = (self.start..index).into();
                self.start = index;
                return Some(range);
            }
        }

        debug_assert!(self.analyzer.next().is_none());

        let range = (self.start..self.len).into();
        self.start = self.len;
        Some(range)
    }
}
