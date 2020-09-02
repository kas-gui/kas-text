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
    level: u8,
}

impl MarkerPos {
    /// Returns the embedding level
    ///
    /// According to Unicode Technical Report #9, the embedding level is
    /// guaranteed to be between 0 and 125 (inclusive), with a default level of
    /// zero and where odd levels are right-to-left.
    #[inline]
    pub fn embedding_level(&self) -> u8 {
        self.level
    }

    /// Returns true if the cursor is left-to-right
    #[inline]
    pub fn is_ltr(&self) -> bool {
        self.level % 2 == 0
    }

    /// Returns true if the cursor is right-to-left
    #[inline]
    pub fn is_rtl(&self) -> bool {
        self.level % 2 == 1
    }
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
    /// The result is also not guaranteed to be within the expected window
    /// between 0 and `self.env().bounds`. The user should clamp the result.
    pub fn text_glyph_pos(&self, index: usize) -> MarkerPosIter {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");

        let mut v: [MarkerPos; 2] = Default::default();
        let (a, mut b) = (0, 0);
        let mut push_result = |pos, ascent, descent, level| {
            v[b as usize] = MarkerPos {
                pos,
                ascent,
                descent,
                level,
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
                let i = if glyph_run.level.is_ltr() {
                    run_part.glyph_range.end()
                } else {
                    run_part.glyph_range.start()
                };
                let pos = if i < glyph_run.glyphs.len() {
                    glyph_run.glyphs[i].position
                } else {
                    // NOTE: for RTL we only hit this case if glyphs.len() == 0
                    Vec2(glyph_run.caret, 0.0)
                };

                let pos = run_part.offset + pos;
                push_result(pos, sf.ascent(), sf.descent(), glyph_run.level.number());
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
            push_result(pos, sf.ascent(), sf.descent(), glyph_run.level.number());
            break;
        }

        MarkerPosIter { v, a, b }
    }

    /// Yield a sequence of rectangles to highlight a given range, by lines
    ///
    /// Rectangles span to end and beginning of lines when wrapping lines.
    /// (Note: gaps are possible between runs in the first and last lines. This
    /// is a defect which should be fixed but low priority and trickier than it
    /// might seem due to bi-directional text allowing re-ordering of runs.)
    ///
    /// This locates the ends of a range as with [`Text::text_glyph_pos`], but
    /// yields a separate rect for each "run" within this range (where "run" is
    /// a line or part of a line). Rects are represented by the top-left
    /// vertex and the bottom-right vertex.
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to the result(s).
    /// The result is also not guaranteed to be within the expected window
    /// between 0 and `self.env().bounds`. The user should clamp the result.
    pub fn highlight_lines(&self, range: std::ops::Range<usize>) -> Vec<(Vec2, Vec2)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        if range.len() == 0 {
            return vec![];
        }

        let mut lines = self.lines.iter();
        let mut rects = Vec::with_capacity(self.lines.len());
        let rbound = self.env.bounds.0;

        // Find the first line
        let mut cur_line = 'l1: loop {
            while let Some(line) = lines.next() {
                if line.text_range.includes(range.start) {
                    break 'l1 line;
                }
            }
            return vec![];
        };

        if range.start > cur_line.text_range.start() || range.end <= cur_line.text_range.end() {
            self.highlight_run_range(range.clone(), cur_line.run_range.to_std(), &mut rects);
            if !rects.is_empty() && range.end > cur_line.text_range.end() {
                // find the rect nearest the line's end and extend
                let mut nearest = 0;
                let first_run = cur_line.run_range.start();
                let glyph_run = self.wrapped_runs[first_run].glyph_run as usize;
                if self.glyph_runs[glyph_run].level.is_ltr() {
                    let mut dist = rbound - (rects[0].1).0;
                    for i in 1..rects.len() {
                        let d = rbound - (rects[i].1).0;
                        if d < dist {
                            nearest = i;
                            dist = d;
                        }
                    }
                    (rects[nearest].1).0 = rbound;
                } else {
                    let mut dist = (rects[0].0).0;
                    for i in 1..rects.len() {
                        let d = (rects[i].0).0;
                        if d < dist {
                            nearest = i;
                            dist = d;
                        }
                    }
                    (rects[nearest].0).0 = 0.0;
                }
            }
        } else {
            let a = Vec2(0.0, cur_line.top);
            let b = Vec2(rbound, cur_line.bottom);
            rects.push((a, b));
        }

        if range.end > cur_line.text_range.end() {
            while let Some(line) = lines.next() {
                if range.end <= line.text_range.end() {
                    cur_line = line;
                    break;
                }

                let a = Vec2(0.0, line.top);
                let b = Vec2(rbound, line.bottom);
                rects.push((a, b));
            }

            if cur_line.text_range.start() < range.end {
                self.highlight_run_range(range, cur_line.run_range.to_std(), &mut rects);
            }
        }

        rects
    }

    /// Yield a sequence of rectangles to highlight a given range, by runs
    ///
    /// Rectangles tightly fit each "run" (piece) of text highlighted. (As an
    /// artifact, the highlighting may leave gaps between runs. This may or may
    /// not change in the future.)
    ///
    /// This locates the ends of a range as with [`Text::text_glyph_pos`], but
    /// yields a separate rect for each "run" within this range (where "run" is
    /// is a line or part of a line). Rects are represented by the top-left
    /// vertex and the bottom-right vertex.
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to the result(s).
    /// The result is also not guaranteed to be within the expected window
    /// between 0 and `self.env().bounds`. The user should clamp the result.
    #[inline]
    pub fn highlight_runs(&self, range: std::ops::Range<usize>) -> Vec<(Vec2, Vec2)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        if range.len() == 0 {
            return vec![];
        }

        let mut rects = Vec::with_capacity(self.wrapped_runs.len());
        self.highlight_run_range(range, 0..self.wrapped_runs.len(), &mut rects);
        rects
    }

    /// Produce highlighting rectangles within a range of runs
    ///
    /// Warning: runs are in logical order which does not correspond to display
    /// order. As a result, the order of results (on a line) is not known.
    fn highlight_run_range(
        &self,
        range: std::ops::Range<usize>,
        run_range: std::ops::Range<usize>,
        rects: &mut Vec<(Vec2, Vec2)>,
    ) {
        assert!(run_range.end <= self.wrapped_runs.len());

        let mut push_rect = |mut a: Vec2, mut b: Vec2, offset, ascent, descent| {
            a = a + offset;
            b = b + offset;
            a.1 -= ascent;
            b.1 -= descent;
            rects.push((a, b));
        };
        let mut a;

        let mut i = run_range.start;
        'b: loop {
            if i >= run_range.end {
                return;
            }
            let run_part = &self.wrapped_runs[i];
            if range.start >= run_part.text_end as usize {
                i += 1;
                continue;
            }

            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
            let sf = fonts().get(glyph_run.font_id).scaled(glyph_run.font_scale);

            // else: range.start < run_part.text_end as usize
            if glyph_run.level.is_ltr() {
                for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                    if glyph.index as usize <= range.start {
                        a = glyph.position;
                        break 'b;
                    }
                }
                a = Vec2::ZERO;
            } else {
                for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                    if glyph.index as usize <= range.start {
                        a = glyph.position;
                        a.0 += sf.h_advance(glyph.id);
                        break 'b;
                    }
                }
                a = Vec2(glyph_run.caret, 0.0);
            }
            break 'b;
        }

        let mut first = true;
        'a: while i < run_range.end {
            let run_part = &self.wrapped_runs[i];
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
            let sf = fonts().get(glyph_run.font_id).scaled(glyph_run.font_scale);

            if !first {
                a = if glyph_run.level.is_ltr() {
                    if run_part.glyph_range.start() < glyph_run.glyphs.len() {
                        glyph_run.glyphs[run_part.glyph_range.start()].position
                    } else {
                        Vec2::ZERO
                    }
                } else {
                    if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                        glyph_run.glyphs[run_part.glyph_range.end()].position
                    } else {
                        Vec2(glyph_run.caret, 0.0)
                    }
                };
            }
            first = false;

            if range.end >= run_part.text_end as usize {
                let b;
                if glyph_run.level.is_ltr() {
                    if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                        b = glyph_run.glyphs[run_part.glyph_range.end()].position;
                    } else {
                        b = Vec2(glyph_run.caret, 0.0);
                    }
                } else {
                    let p = glyph_run.glyphs[run_part.glyph_range.start()].position;
                    b = Vec2(a.0, p.1);
                    a.0 = p.0;
                };

                push_rect(a, b, run_part.offset, sf.ascent(), sf.descent());
                i += 1;
                continue;
            }

            // else: range.end < run_part.text_end as usize
            let b;
            'c: loop {
                if glyph_run.level.is_ltr() {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                        if glyph.index as usize <= range.end {
                            b = glyph.position;
                            break 'c;
                        }
                    }
                } else {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                        if glyph.index as usize <= range.end {
                            let mut p = glyph.position;
                            p.0 += sf.h_advance(glyph.id);
                            b = Vec2(a.0, p.1);
                            a.0 = p.0;
                            break 'c;
                        }
                    }
                }
                break 'a;
            }

            push_rect(a, b, run_part.offset, sf.ascent(), sf.descent());
            break;
        }
    }
}
