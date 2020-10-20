// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Utility types and traits

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
}
