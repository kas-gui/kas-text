// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

/// Describes required text-preparation actions
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Action {
    /// Nothing to do
    None,
    /// Fix vertical alignment
    VAlign,
    /// Do wrapping and alignment
    Wrap,
    /// Resize text, and above
    Resize,
    /// Break text into runs, associate fonts, and above
    All,
}

impl Action {
    /// True if action is `Action::None`
    #[inline]
    pub fn is_ready(&self) -> bool {
        *self == Action::None
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
