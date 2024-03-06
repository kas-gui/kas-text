// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

/// Describes the state-of-preparation of a [`TextDisplay`][crate::TextDisplay]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Status {
    /// Nothing done yet
    #[default]
    New,
    /// Configured
    Configured,
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
