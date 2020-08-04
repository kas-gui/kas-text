// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text navigation

use super::Text;
use crate::{fonts, Vec2};
use ab_glyph::ScaleFont;

/// Used to return the position of a glyph with associated metrics
#[derive(Copy, Clone, Default, PartialEq)]
pub struct MarkerPos {
    /// (x, y) coordinate of glyph
    pub pos: Vec2,
    /// Ascent (subtract from y to get top)
    pub ascent: f32,
    /// Descent (subtract from y to get bottom)
    pub descent: f32,
}

pub struct MarkerPosIter {
    v: [MarkerPos; 2],
    a: usize,
    b: usize,
}

impl MarkerPosIter {
    /// Directly access the slice of results
    ///
    /// The result excludes elements removed by iteration.
    pub fn as_slice(&self) -> &[MarkerPos] {
        &self.v[self.a..self.b]
    }
}

impl Iterator for MarkerPosIter {
    type Item = MarkerPos;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.a < self.b {
            let i = self.a;
            self.a = i + 1;
            Some(self.v[i])
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.b - self.a;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for MarkerPosIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.a < self.b {
            let i = self.b - 1;
            self.b = i;
            Some(self.v[i])
        } else {
            None
        }
    }
}

impl ExactSizeIterator for MarkerPosIter {}

impl Text {
    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// The index should be no greater than the text length. It is not required
    /// to be on a code-point boundary. Returns an iterator over matching
    /// positions. Length of results is guaranteed to be one of 0, 1 or 2:
    ///
    /// -   0 if the index is not included in any run of text (probably only
    ///     possible within a multi-byte line break or beyond the text length)
    /// -   1 is the normal case
    /// -   2 if the index is at the end of one run of text *and* at the start
    ///     of another; these positions may be the same or may not be (over
    ///     line breaks and with bidirectional text). If only a single position
    ///     is desired, usually the latter is preferred (via `next_back()`).
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to the result(s).
    pub fn text_glyph_pos(&self, index: usize) -> MarkerPosIter {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");

        let mut v: [MarkerPos; 2] = Default::default();
        let (a, mut b) = (0, 0);
        let mut push_result = |pos, ascent, descent| {
            v[b as usize] = MarkerPos {
                pos,
                ascent,
                descent,
            };
            b += 1;
        };

        // We don't care too much about performance: use a naive search strategy
        'a: for run_part in &self.wrapped_runs {
            if index > run_part.text_end as usize {
                continue;
            }

            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
            let sf = fonts().get(glyph_run.font_id).scaled(glyph_run.font_scale);

            // If index is at the end of a run, we potentially get two matches.
            if index == run_part.text_end as usize {
                let pos = if glyph_run.level.is_ltr() {
                    if index < glyph_run.range.end() {
                        assert!(run_part.glyph_range.end() < glyph_run.glyphs.len());
                        glyph_run.glyphs[run_part.glyph_range.end()].position
                    } else {
                        Vec2(glyph_run.caret, 0.0)
                    }
                } else {
                    glyph_run.glyphs[run_part.glyph_range.start()].position
                };

                let pos = run_part.offset + pos;
                let pos = pos.min(self.env.bounds).max(Vec2::ZERO); // text may exceed bounds
                push_result(pos, sf.ascent(), sf.descent());
                continue;
            }

            // else: index < run_part.text_end as usize
            let pos = 'b: loop {
                if glyph_run.level.is_ltr() {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                        if glyph.index as usize <= index {
                            break 'b glyph.position;
                        }
                    }
                } else {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                        if glyph.index as usize <= index {
                            let mut pos = glyph.position;
                            pos.0 += sf.h_advance(glyph.id);
                            break 'b pos;
                        }
                    }
                }
                break 'a;
            };

            let pos = run_part.offset + pos;
            let pos = pos.min(self.env.bounds).max(Vec2::ZERO); // text may exceed bounds
            push_result(pos, sf.ascent(), sf.descent());
            break;
        }

        MarkerPosIter { v, a, b }
    }
}
