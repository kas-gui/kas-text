// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — simple data types

/// 2D size over `f32`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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

impl std::ops::Sub for Vec2 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Vec2(self.0 - other.0, self.1 - other.1)
    }
}

impl From<Vec2> for (f32, f32) {
    fn from(size: Vec2) -> Self {
        (size.0, size.1)
    }
}

impl From<Vec2> for ab_glyph::Point {
    fn from(size: Vec2) -> ab_glyph::Point {
        ab_glyph::Point {
            x: size.0,
            y: size.1,
        }
    }
}

impl From<ab_glyph::Point> for Vec2 {
    fn from(p: ab_glyph::Point) -> Vec2 {
        Vec2(p.x, p.y)
    }
}

/// Font scale
///
/// This is approximately the pixel-height of a line of text or double the
/// "pt" size. Usually you want to use the same scale for both components,
/// e.g. `FontScale::from(18.0)`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FontScale {
    pub x: f32,
    pub y: f32,
}

impl Default for FontScale {
    fn default() -> Self {
        FontScale::from(1.0)
    }
}

impl From<f32> for FontScale {
    fn from(scale: f32) -> Self {
        FontScale { x: scale, y: scale }
    }
}

impl From<FontScale> for ab_glyph::PxScale {
    fn from(FontScale { x, y }: FontScale) -> ab_glyph::PxScale {
        ab_glyph::PxScale { x, y }
    }
}

impl std::ops::Mul<f32> for FontScale {
    type Output = Self;

    #[inline]
    fn mul(self, f: f32) -> Self::Output {
        FontScale {
            x: self.x * f,
            y: self.y * f,
        }
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
pub struct Range {
    pub start: u32,
    pub end: u32,
}

impl Range {
    /// The start, as `usize`
    pub fn start(&self) -> usize {
        self.start as usize
    }

    /// The end, as `usize`
    pub fn end(&self) -> usize {
        self.end as usize
    }

    /// True if the given value is contained
    pub fn contains(&self, value: usize) -> bool {
        self.start as usize <= value && value < self.end as usize
    }
}

impl std::iter::Iterator for Range {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.start < self.end {
            let result = self.start;
            self.start += 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.end - self.start) as usize;
        (len, Some(len))
    }
}

impl std::iter::DoubleEndedIterator for Range {
    fn next_back(&mut self) -> Option<u32> {
        if self.start < self.end {
            self.end -= 1;
            Some(self.end)
        } else {
            None
        }
    }
}

impl std::iter::ExactSizeIterator for Range {}
impl std::iter::FusedIterator for Range {}

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
        (range.start as usize)..(range.end as usize)
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
        assert!(range.end <= u32::MAX as usize);
        Range {
            start: range.start as u32,
            end: range.end as u32,
        }
    }
}
