// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text representation

use crate::env::Environment;
use crate::fonts::FontId;

/// Text representation trait
///
/// All texts, regardless of formatting, must be able to represent themselves as
/// a single contiguous `&str`. Formatting is provided as an optional extra.
///
/// This trait supports usage via generics aka compile-time polymorphism. For
/// run-time polymorphism, use the [`DynText`] type and [`Text::as_dyn_text`].
pub trait Text: std::fmt::Debug + 'static {
    #[cfg(not(feature = "gat"))]
    type FmtIter: Iterator<Item = Format>;
    #[cfg(feature = "gat")]
    type FmtIter<'a>: Iterator<Item = Format>;

    /// Length of text
    #[inline]
    fn len(&self) -> usize {
        self.as_str().len()
    }

    /// Access whole text as contiguous `str`
    fn as_str(&self) -> &str;

    /// Construct an iterator over formatting items
    ///
    /// This is marked `unsafe`: some implementations must transmute lifetimes
    /// to `'static`. Callers are required to unsure that the struct `self`
    /// outlives the iterator produced by this method.
    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Self::FmtIter;
    /// Construct an iterator over formatting items
    ///
    /// It is expected that [`Format::start`] of yielded items is strictly
    /// increasing; if not, formatting may not be applied correctly.
    ///
    /// For plain text this iterator is empty.
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Self::FmtIter<'a>;

    /// Convert a `Sized` item to polymorphic form
    #[cfg_attr(doc_cfg, doc(cfg(feature = "gat")))]
    #[cfg(feature = "gat")]
    fn as_dyn_text(self) -> DynText
    where
        Self: Sized + 'static,
    {
        DynText(Box::new(self))
    }
}

/// Extension over [`Text`] supporting text-edit operations
pub trait EditableText: Text {
    /// Set unformatted text
    ///
    /// Existing contents and formatting are replaced entirely.
    fn set_string(&mut self, string: String);

    /// Swap the contiguous unformatted text with another `string`
    ///
    /// Any formatting present is removed.
    fn swap_string(&mut self, string: &mut String);

    /// Insert a `char` at the given position
    ///
    /// Formatting is adjusted such that it still affects the same chars (i.e.
    /// all formatting after `index` is postponed by the length of the char).
    fn insert_char(&mut self, index: usize, c: char);

    /// Replace text at `range` with `replace_with`
    ///
    /// Formatting is adjusted such that it still affects the same chars.
    fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize> + std::iter::ExactSizeIterator + Clone;
}

/// Formatting marker
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Format {
    /// Index in text at which formatting becomes active
    ///
    /// (Note that we use `u32` not `usize` since it can be assumed text length
    /// will never exeed `u32::MAX`.)
    pub start: u32,
    /// Font identifier
    pub font_id: FontId,
    /// Font size, in dots-per-em (pixel width of an 'M')
    ///
    /// This may be calculated from point size as `pt_size * dpp`, where `dpp`
    /// is the number of pixels per point ([`Environment::dpp`]).
    pub dpem: f32,
}

impl Text for &'static str {
    #[cfg(not(feature = "gat"))]
    type FmtIter = std::iter::Empty<Format>;
    #[cfg(feature = "gat")]
    type FmtIter<'a> = std::iter::Empty<Format>;

    fn as_str(&self) -> &str {
        self
    }

    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, _: &'a Environment) -> Self::FmtIter {
        std::iter::empty()
    }
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, _: &'a Environment) -> Self::FmtIter<'a> {
        std::iter::empty()
    }
}

impl Text for String {
    #[cfg(not(feature = "gat"))]
    type FmtIter = std::iter::Empty<Format>;
    #[cfg(feature = "gat")]
    type FmtIter<'a> = std::iter::Empty<Format>;

    fn as_str(&self) -> &str {
        self
    }

    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, _: &'a Environment) -> Self::FmtIter {
        std::iter::empty()
    }
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, _: &'a Environment) -> Self::FmtIter<'a> {
        std::iter::empty()
    }
}

impl EditableText for String {
    fn set_string(&mut self, string: String) {
        *self = string;
    }

    fn swap_string(&mut self, string: &mut String) {
        std::mem::swap(self, string);
    }

    fn insert_char(&mut self, index: usize, c: char) {
        self.insert(index, c);
    }

    fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize> + std::iter::ExactSizeIterator + Clone,
    {
        self.replace_range(range, replace_with);
    }
}

/// Dynamic variant of [`Text`]
trait DynTextT: std::fmt::Debug {
    /// Length of text
    #[inline]
    fn len(&self) -> usize {
        self.as_str().len()
    }

    /// Access whole text as contiguous `str`
    fn as_str(&self) -> &str;

    /// Construct an iterator over formatting items
    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format>>;
    /// Construct an iterator over formatting items
    ///
    /// It is expected that [`Format::start`] of yielded items is strictly
    /// increasing; if not, formatting may not be applied correctly.
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format> + 'a>;
}

impl<T: Text + 'static> DynTextT for T {
    #[inline]
    fn len(&self) -> usize {
        T::len(self)
    }

    fn as_str(&self) -> &str {
        T::as_str(self)
    }

    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format>> {
        Box::new(T::fmt_iter(self, env))
    }
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format> + 'a> {
        Box::new(T::fmt_iter(self, env))
    }
}

/// Wrapper type supporting run-time polymorphism over [`Text`]
#[derive(Debug)]
pub struct DynText(Box<dyn DynTextT>);

impl Text for DynText {
    #[cfg(not(feature = "gat"))]
    type FmtIter = Box<dyn Iterator<Item = Format> + 'static>;
    #[cfg(feature = "gat")]
    type FmtIter<'a> = Box<dyn Iterator<Item = Format> + 'a>;

    #[inline]
    fn len(&self) -> usize {
        DynTextT::len(self)
    }

    #[inline]
    fn as_str(&self) -> &str {
        DynTextT::as_str(self)
    }

    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format>> {
        Box::new(DynTextT::fmt_iter(self, env))
    }
    #[cfg(feature = "gat")]
    #[inline]
    fn fmt_iter<'a>(&'a self, env: &'a Environment) -> Box<dyn Iterator<Item = Format> + 'a> {
        Box::new(DynTextT::fmt_iter(self, env))
    }
}
