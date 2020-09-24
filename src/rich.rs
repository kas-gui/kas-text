// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Models of rich text in abstract from an environment

#[cfg(feature = "markdown")]
mod markdown;

use crate::fonts::FontId;
use std::convert::TryFrom;
use thiserror::Error;

/// A rich text representation
///
/// This format may be used to input and share text, but does not include
/// details specific to the presentation or presentation environment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    /// The raw text
    ///
    /// This is a contiguous version of the text to be presented, without
    /// (non-unicode) formatting details. Note that Unicode control characters
    /// may be present, e.g. U+2029 (paragraph separator) and explicit
    /// directional formatting characters.
    pub text: String,
    /// Formatting specifiers
    pub formatting: FormatList,
}

impl Text {
    /// Parse input as Markdown
    #[cfg(feature = "markdown")]
    #[inline]
    pub fn from_md(text: &str) -> Self {
        markdown::parse(text)
    }

    /// The length of all concatenated runs
    pub fn len(&self) -> usize {
        self.text.len()
    }
}

/// Generate an unformatted `String` from the concatenation of all runs
impl ToString for Text {
    fn to_string(&self) -> String {
        self.text.clone()
    }
}

impl From<Text> for String {
    fn from(text: Text) -> String {
        text.text
    }
}

impl From<String> for Text {
    fn from(text: String) -> Text {
        Text {
            text,
            ..Default::default()
        }
    }
}

impl<'a> From<&'a str> for Text {
    fn from(text: &'a str) -> Text {
        Text::from(text.to_string())
    }
}

/// A sequence of [`FormatSpecifier`]
///
/// This is a sequence of zero-or-more [`FormatSpecifier`]s. Each must have
/// `start` index greater than the previous specifier. The first index may be
/// greater than zero.
///
/// This is a wrapper over `Vec<FormatSpecifier>` to enforce ordering.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormatList {
    pub(crate) seq: Vec<FormatSpecifier>,
}

impl FormatList {
    /// Append or replace last item
    #[inline]
    pub fn set_last(&mut self, item: FormatSpecifier) {
        if let Some(last) = self.seq.last_mut() {
            if last.start >= item.start {
                *last = item;
                return;
            }
        }
        self.seq.push(item);
    }

    /// True if the list empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.seq.is_empty()
    }

    /// Returns the number of items
    #[inline]
    pub fn len(&self) -> usize {
        self.seq.len()
    }

    /// Returns the number of elements the vector can hold without reallocating.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.seq.capacity()
    }

    /// Reserves capacity for at least `additional` more elements to be inserted
    /// into the list. See documentation of [`Vec::reserve`].
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.seq.reserve(additional);
    }

    /// Remove all items
    #[inline]
    pub fn clear(&mut self) {
        self.seq.clear();
    }

    /// Append an item
    #[inline]
    pub fn push(&mut self, item: FormatSpecifier) -> Result<(), FormatListError> {
        if self
            .seq
            .last()
            .map(|last| last.start >= item.start)
            .unwrap_or(false)
        {
            return Err(FormatListError::Order);
        }
        self.seq.push(item);
        Ok(())
    }

    /// Remove the last item
    #[inline]
    pub fn pop(&mut self) -> Option<FormatSpecifier> {
        self.seq.pop()
    }

    /// Inserts an item at position `index` within the specifier list
    #[inline]
    pub fn insert(&mut self, index: usize, item: FormatSpecifier) -> Result<(), FormatListError> {
        if index > self.seq.len() {
            return Err(FormatListError::Order);
        }
        self.seq.insert(index, item);
        Ok(())
    }

    /// Removes the item at position `index`
    ///
    /// Panics if `index` is out of bounds.
    #[inline]
    pub fn remove(&mut self, index: usize) -> FormatSpecifier {
        self.seq.remove(index)
    }

    /// Append items from an iterator
    ///
    /// Note: this may not be as well optimised as [`Vec::extend`] due to
    /// requirements of order check.
    #[inline]
    pub fn extend<T: IntoIterator<Item = FormatSpecifier>>(
        &mut self,
        iter: T,
    ) -> Result<(), FormatListError> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }

    /// Iterate over specifiers
    #[inline]
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a FormatSpecifier> {
        self.seq.iter()
    }
}

impl TryFrom<Vec<FormatSpecifier>> for FormatList {
    type Error = FormatListError;

    fn try_from(seq: Vec<FormatSpecifier>) -> Result<Self, Self::Error> {
        if let Some(first) = seq.get(0) {
            let mut start = first.start;
            for item in &seq[1..] {
                if item.start <= start {
                    return Err(FormatListError::Order);
                }
                start = item.start;
            }
        }
        Ok(FormatList { seq })
    }
}

impl TryFrom<&[FormatSpecifier]> for FormatList {
    type Error = FormatListError;

    fn try_from(seq: &[FormatSpecifier]) -> Result<Self, Self::Error> {
        let seq = Vec::from(seq);
        FormatList::try_from(seq)
    }
}

/// Specifies format information for a range of text
///
/// Text ranges are specified via `start` only, and extend to the next
/// `FormatSpecifier`. Each *optionally* adjusts font properties; properties
/// not explicitly specified will be inherited from the previous range or
/// (at the start) from the environment.
///
/// This type can be default-constructed, but the default instance does nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormatSpecifier {
    pub(crate) start: u32,              // index in text
    pub(crate) font_id: Option<FontId>, // if None, use previous/default value
    pub(crate) pt_size: f32,            // if NAN, use previous/environment value
}

impl Default for FormatSpecifier {
    #[inline]
    fn default() -> Self {
        FormatSpecifier {
            start: 0,
            font_id: None,
            pt_size: f32::NAN,
        }
    }
}

impl FormatSpecifier {
    /// Construct with `start` and a font selector
    #[inline]
    pub fn new_font(start: usize, font_id: FontId) -> Self {
        let start = start as u32;
        FormatSpecifier {
            start,
            font_id: Some(font_id),
            ..Default::default()
        }
    }

    /// Construct with `start` and a font size in Points
    #[inline]
    pub fn new_size(start: usize, pt_size: f32) -> Self {
        let start = start as u32;
        FormatSpecifier {
            start,
            pt_size,
            ..Default::default()
        }
    }

    /// Construct with `start`, a font selector and a font size
    #[inline]
    pub fn new_font_size(start: usize, font_id: FontId, pt_size: f32) -> Self {
        let start = start as u32;
        FormatSpecifier {
            start,
            font_id: Some(font_id),
            pt_size,
        }
    }

    /// Adjust the starting index
    #[inline]
    pub fn set_start(&mut self, start: usize) {
        self.start = start as u32;
    }

    /// Adjust the font
    ///
    /// If `None` is passed, this `FormatSpecifier` will not adjust the font.
    #[inline]
    pub fn set_font(&mut self, font_id: Option<FontId>) {
        self.font_id = font_id;
    }

    /// Adjust the font size (Points)
    ///
    /// If `f32::NAN` is passed, this `FormatSpecifier` will not adjust the
    /// font size.
    #[inline]
    pub fn set_size(&mut self, pt_size: f32) {
        self.pt_size = pt_size;
    }
}

/// Error returned if [`FormatList`]'s order constraints are not met.
#[derive(Error, Debug)]
pub enum FormatListError {
    #[error("order constraints of FormatList violated")]
    Order,
}
