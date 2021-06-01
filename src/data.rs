// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — simple data types

use crate::conv::{to_u32, to_usize};

/// 2D vector (position/size/offset) over `f32`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Vec2(pub f32, pub f32);

impl Vec2 {
    /// Zero
    pub const ZERO: Vec2 = Vec2(0.0, 0.0);

    /// Positive infinity
    pub const INFINITY: Vec2 = Vec2(f32::INFINITY, f32::INFINITY);

    /// Take the absolute value of each component
    #[inline]
    pub fn abs(self) -> Self {
        Vec2(self.0.abs(), self.1.abs())
    }

    /// Return the minimum, componentwise
    #[inline]
    pub fn min(self, other: Self) -> Self {
        Vec2(self.0.min(other.0), self.1.min(other.1))
    }

    /// Return the maximum, componentwise
    #[inline]
    pub fn max(self, other: Self) -> Self {
        Vec2(self.0.max(other.0), self.1.max(other.1))
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self {
        Vec2(self.0 + other.0, self.1 + other.1)
    }
}
impl std::ops::AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
        self.1 += rhs.1;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec2(self.0 - other.0, self.1 - other.1)
    }
}
impl std::ops::SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
        self.1 -= rhs.1;
    }
}

impl From<Vec2> for (f32, f32) {
    fn from(size: Vec2) -> Self {
        (size.0, size.1)
    }
}

/// Range type
///
/// Essentially this is just a `std::ops::Range<u32>`, but with convenient
/// implementations.
///
/// Note that we consider `u32` large enough for any text we wish to display
/// and the library is too complex to be useful on 16-bit CPUs, so using `u32`
/// makes more sense than `usize`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Range {
    pub start: u32,
    pub end: u32,
}

impl Range {
    /// The start, as `usize`
    pub fn start(self) -> usize {
        to_usize(self.start)
    }

    /// The end, as `usize`
    pub fn end(self) -> usize {
        to_usize(self.end)
    }

    /// The number of iterable items, as `usize`
    pub fn len(self) -> usize {
        to_usize(self.end) - to_usize(self.start)
    }

    /// True if the given value is contained, inclusive of end points
    pub fn includes(self, value: usize) -> bool {
        to_usize(self.start) <= value && value <= to_usize(self.end)
    }

    /// Convert to a standard range
    pub fn to_std(self) -> std::ops::Range<usize> {
        to_usize(self.start)..to_usize(self.end)
    }

    /// Convert to `usize` and iterate
    pub fn iter(self) -> impl Iterator<Item = usize> {
        self.to_std()
    }
}

impl std::ops::Index<Range> for String {
    type Output = str;

    fn index(&self, range: Range) -> &str {
        let range = std::ops::Range::<usize>::from(range);
        &self[range]
    }
}

impl std::ops::Index<Range> for str {
    type Output = str;

    fn index(&self, range: Range) -> &str {
        let range = std::ops::Range::<usize>::from(range);
        &self[range]
    }
}

impl<T> std::ops::Index<Range> for [T] {
    type Output = [T];

    fn index(&self, range: Range) -> &[T] {
        let range = std::ops::Range::<usize>::from(range);
        &self[range]
    }
}

impl std::ops::IndexMut<Range> for String {
    fn index_mut(&mut self, range: Range) -> &mut str {
        let range = std::ops::Range::<usize>::from(range);
        &mut self[range]
    }
}

impl std::ops::IndexMut<Range> for str {
    fn index_mut(&mut self, range: Range) -> &mut str {
        let range = std::ops::Range::<usize>::from(range);
        &mut self[range]
    }
}

impl<T> std::ops::IndexMut<Range> for [T] {
    fn index_mut(&mut self, range: Range) -> &mut [T] {
        let range = std::ops::Range::<usize>::from(range);
        &mut self[range]
    }
}

impl From<Range> for std::ops::Range<usize> {
    fn from(range: Range) -> std::ops::Range<usize> {
        to_usize(range.start)..to_usize(range.end)
    }
}

impl From<std::ops::Range<u32>> for Range {
    fn from(range: std::ops::Range<u32>) -> Range {
        Range {
            start: range.start,
            end: range.end,
        }
    }
}

impl From<std::ops::Range<usize>> for Range {
    fn from(range: std::ops::Range<usize>) -> Range {
        Range {
            start: to_u32(range.start),
            end: to_u32(range.end),
        }
    }
}
