// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Methods using positioned glyphs

use super::{Line, TextDisplay};
use crate::conv::to_usize;
use crate::fonts::{self, FaceId, ScaledFaceRef};
use crate::{Glyph, Range, Vec2, shaper};
use std::fmt::Debug;

/// Used to return the position of a glyph with associated metrics
#[derive(Copy, Clone, Debug, Default, PartialEq)]
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

/// A sequence of positioned glyphs with effects
///
/// Yielded by [`TextDisplay::runs`].
pub struct GlyphRun<'a, E> {
    run: &'a shaper::GlyphRun,
    range: Range,
    offset: Vec2,
    top: f32,
    bottom: f32,
    effects: &'a [(u32, E)],
}

impl<'a, E: Copy + Default> GlyphRun<'a, E> {
    /// Get the [`FaceId`] for this run
    #[inline]
    pub fn face_id(&self) -> FaceId {
        self.run.face_id
    }

    /// Get the font size for this run
    ///
    /// Units are dots-per-Em (see [crate::fonts]).
    #[inline]
    pub fn dpem(&self) -> f32 {
        self.run.dpem
    }

    /// Get the [`ScaledFaceRef`] for this run
    ///
    /// This may be useful to access font metrics.
    #[inline]
    pub fn scaled_face(&self) -> ScaledFaceRef<'_> {
        fonts::library()
            .get_face(self.run.face_id)
            .scale_by_dpu(self.run.dpu)
    }

    /// Get the `top` position of the line
    ///
    /// Note that there may be multiple runs per line and that these may not all
    /// have the same [`ScaledFaceRef::ascent`] value when multiple fonts are
    /// used, thus it is usually preferable to use the this value for
    /// background colors (highlighting).
    #[inline]
    pub fn line_top(&self) -> f32 {
        self.top
    }

    /// Get the `bottom` position of the line
    ///
    /// Note that there may be multiple runs per line and that these may not all
    /// have the same [`ScaledFaceRef::descent`] value when multiple fonts are
    /// used, thus it is usually preferable to use the this value for
    /// background colors (highlighting).
    #[inline]
    pub fn line_bottom(&self) -> f32 {
        self.bottom
    }

    /// Get an iterator over glyphs for this run
    ///
    /// This method ignores effects; if you want those call
    /// [`Self::glyphs_with_effects`] instead.
    pub fn glyphs(&self) -> impl Iterator<Item = Glyph> + '_ {
        self.run.glyphs[self.range.to_std()]
            .iter()
            .map(|glyph| Glyph {
                index: glyph.index,
                id: glyph.id,
                position: glyph.position + self.offset,
            })
    }

    /// Yield glyphs and effects for this run
    ///
    /// The callback `f` is called for each `glyph` in unspecified order. The
    /// corresponding `effect` is passed.
    ///
    /// The callback `g` is called for sub-ranges of glyphs with parameters
    /// `(p, x2, effect)`; `p.0` and `x2` are the x-axis positions of the left
    /// and right edges of this sub-range respectively while `x.1` is the
    /// vertical position of glyphs (the baseline). This may be combined with
    /// information from [`Self::scaled_face`] to draw underline, strike-through
    /// and background effects.
    ///
    /// Note: this is more computationally expensive than [`GlyphRun::glyphs`],
    /// so you may prefer to call that. Optionally one may choose to cache the
    /// result, though this is not really necessary.
    pub fn glyphs_with_effects<F, G>(&self, mut f: F, mut g: G)
    where
        F: FnMut(Glyph, E),
        G: FnMut(Vec2, f32, E),
    {
        let ltr = self.run.level.is_ltr();

        let mut effect_cur = usize::MAX;
        let mut effect_next = 0;

        // We iterate in left-to-right order regardless of text direction.
        // Start by finding the effect applicable to the left-most glyph.
        let left_index = self.run.glyphs[self.range.start()].index;
        while self
            .effects
            .get(effect_next)
            .map(|e| e.0 <= left_index)
            .unwrap_or(false)
        {
            effect_cur = effect_next;
            effect_next += 1;
        }

        let mut next_start = self
            .effects
            .get(effect_next)
            .map(|e| e.0)
            .unwrap_or(u32::MAX);

        let mut fmt = self
            .effects
            .get(effect_cur)
            .cloned()
            .unwrap_or((0, E::default()));

        // In case an effect applies to the left-most glyph, it starts from that
        // glyph's x coordinate.
        let mut range_start = self.run.glyphs[self.range.start()].position + self.offset;

        // Iterate over glyphs in left-to-right order.
        for mut glyph in self.run.glyphs[self.range.to_std()].iter().cloned() {
            glyph.position += self.offset;

            // Does the effect change?
            if (ltr && next_start <= glyph.index) || (!ltr && fmt.0 > glyph.index) {
                let x2 = glyph.position.0;
                g(range_start, x2, fmt.1);

                if ltr {
                    // Find the next active effect
                    effect_cur = effect_next;
                    effect_next += 1;
                    next_start = self
                        .effects
                        .get(effect_next)
                        .map(|e| e.0)
                        .inspect(|start| debug_assert!(*start > glyph.index))
                        .unwrap_or(u32::MAX);
                } else {
                    // Find the previous active effect
                    effect_cur = effect_cur.wrapping_sub(1);
                    debug_assert!(
                        self.effects.get(effect_cur).map(|e| e.0).unwrap_or(0) <= glyph.index
                    );
                }
                fmt = self
                    .effects
                    .get(effect_cur)
                    .cloned()
                    .unwrap_or((0, E::default()));

                range_start = glyph.position;
            }

            f(glyph, fmt.1);
        }

        // Effects end at the following glyph's start (or end of this run part)
        let x2 = if self.range.end() < self.run.glyphs.len() {
            self.run.glyphs[self.range.end()].position.0
        } else {
            self.run.caret
        } + self.offset.0;
        g(range_start, x2, fmt.1);
    }
}

impl TextDisplay {
    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
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
    /// The result is not guaranteed to be within [`Self::bounding_box`].
    /// Depending on the use-case, the caller may need to clamp the resulting
    /// position.
    pub fn text_glyph_pos(&self, index: usize) -> MarkerPosIter {
        let mut v: [MarkerPos; 2] = Default::default();
        let (a, mut b) = (0, 0);
        let mut push_result = |pos, ascent, descent, level| {
            v[b] = MarkerPos {
                pos,
                ascent,
                descent,
                level,
            };
            b += 1;
        };

        // We don't care too much about performance: use a naive search strategy
        'a: for run_part in &self.wrapped_runs {
            if index > to_usize(run_part.text_end) {
                continue;
            }

            let glyph_run = &self.runs[to_usize(run_part.glyph_run)];
            let sf = fonts::library()
                .get_face(glyph_run.face_id)
                .scale_by_dpu(glyph_run.dpu);

            // If index is at the end of a run, we potentially get two matches.
            if index == to_usize(run_part.text_end) {
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

            // else: index < to_usize(run_part.text_end)
            let pos = 'b: {
                if glyph_run.level.is_ltr() {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                        if to_usize(glyph.index) <= index {
                            break 'b glyph.position;
                        }
                    }
                } else {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                        if to_usize(glyph.index) <= index {
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

    /// Iterate over runs of positioned glyphs
    ///
    /// All glyphs are translated by the given `offset` (this is practically
    /// free).
    ///
    /// The `effects` sequence may be used for rendering effects: glyph color,
    /// background color, strike-through, underline. Use `&[]` for no effects
    /// (effectively using the default value of `E` everywhere), or use a
    /// sequence such that `effects[i].0` values are strictly increasing and
    /// each mapping to a distinct glyph cluster. A
    /// glyph for index `j` in the source text will use effect `effects[i].1`
    /// where `i` is the largest value such that `effects[i].0 <= j`, or the
    /// default value of `E` if no such `i` exists.
    ///
    /// Runs are yielded in undefined order.
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    pub fn runs<'a, E: Copy + Debug + Default>(
        &'a self,
        offset: Vec2,
        effects: &'a [(u32, E)],
    ) -> impl Iterator<Item = GlyphRun<'a, E>> + 'a {
        #[cfg(debug_assertions)]
        {
            let mut start = None;
            for effect in effects {
                if let Some(i) = start
                    && effect.0 <= i
                {
                    panic!(
                        "TextDisplay::runs: effect start indices are not strictly increasing in {effects:?}"
                    );
                }
                start = Some(effect.0);
            }
        }

        let mut line_iter = self.lines.iter();
        let mut line = line_iter.next().unwrap();
        self.wrapped_runs
            .iter()
            .filter(|part| !part.glyph_range.is_empty())
            .map(move |part| {
                while part.text_end > line.text_range.end {
                    line = line_iter.next().unwrap();
                }
                GlyphRun {
                    run: &self.runs[to_usize(part.glyph_run)],
                    range: part.glyph_range,
                    offset: offset + part.offset,
                    top: offset.1 + line.top,
                    bottom: offset.1 + line.bottom,
                    effects,
                }
            })
    }

    /// Yield a sequence of rectangles to highlight a given text range
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// Calls `f(top_left, bottom_right)` for each highlighting rectangle.
    pub fn highlight_range(&self, range: std::ops::Range<usize>, f: &mut dyn FnMut(Vec2, Vec2)) {
        for line in &self.lines {
            let line_range = line.text_range();
            if line_range.end <= range.start {
                continue;
            } else if range.end <= line_range.start {
                break;
            } else if range.start <= line_range.start && line_range.end <= range.end {
                let tl = Vec2(self.l_bound, line.top);
                let br = Vec2(self.r_bound, line.bottom);
                f(tl, br);
            } else {
                self.highlight_line(line.clone(), range.clone(), f);
            }
        }
    }

    /// Produce highlighting rectangles within a range of runs
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// Warning: runs are in logical order which does not correspond to display
    /// order. As a result, the order of results (on a line) is not known.
    fn highlight_line(
        &self,
        line: Line,
        range: std::ops::Range<usize>,
        f: &mut dyn FnMut(Vec2, Vec2),
    ) {
        let fonts = fonts::library();

        let mut a;

        let mut i = line.run_range.start();
        let line_range_end = line.run_range.end();
        'b: loop {
            if i >= line_range_end {
                return;
            }
            let run_part = &self.wrapped_runs[i];
            if range.start >= to_usize(run_part.text_end) {
                i += 1;
                continue;
            }

            let glyph_run = &self.runs[to_usize(run_part.glyph_run)];

            if glyph_run.level.is_ltr() {
                for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                    if to_usize(glyph.index) <= range.start {
                        a = glyph.position.0;
                        break 'b;
                    }
                }
                a = 0.0;
            } else {
                let sf = fonts
                    .get_face(glyph_run.face_id)
                    .scale_by_dpu(glyph_run.dpu);
                for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                    if to_usize(glyph.index) <= range.start {
                        a = glyph.position.0;
                        a += sf.h_advance(glyph.id);
                        break 'b;
                    }
                }
                a = glyph_run.caret;
            }
            break 'b;
        }

        let mut first = true;
        'a: while i < line_range_end {
            let run_part = &self.wrapped_runs[i];
            let offset = run_part.offset.0;
            let glyph_run = &self.runs[to_usize(run_part.glyph_run)];

            if !first {
                a = if glyph_run.level.is_ltr() {
                    if run_part.glyph_range.start() < glyph_run.glyphs.len() {
                        glyph_run.glyphs[run_part.glyph_range.start()].position.0
                    } else {
                        0.0
                    }
                } else {
                    if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                        glyph_run.glyphs[run_part.glyph_range.end()].position.0
                    } else {
                        glyph_run.caret
                    }
                };
            }
            first = false;

            if range.end >= to_usize(run_part.text_end) {
                let b;
                if glyph_run.level.is_ltr() {
                    if run_part.glyph_range.end() < glyph_run.glyphs.len() {
                        b = glyph_run.glyphs[run_part.glyph_range.end()].position.0;
                    } else {
                        b = glyph_run.caret;
                    }
                } else {
                    let p = if run_part.glyph_range.start() < glyph_run.glyphs.len() {
                        glyph_run.glyphs[run_part.glyph_range.start()].position.0
                    } else {
                        // NOTE: for RTL we only hit this case if glyphs.len() == 0
                        glyph_run.caret
                    };
                    b = a;
                    a = p;
                };

                f(Vec2(a + offset, line.top), Vec2(b + offset, line.bottom));
                i += 1;
                continue;
            }

            let sf = fonts
                .get_face(glyph_run.face_id)
                .scale_by_dpu(glyph_run.dpu);

            let b;
            'c: {
                if glyph_run.level.is_ltr() {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter().rev() {
                        if to_usize(glyph.index) <= range.end {
                            b = glyph.position.0;
                            break 'c;
                        }
                    }
                } else {
                    for glyph in glyph_run.glyphs[run_part.glyph_range.to_std()].iter() {
                        if to_usize(glyph.index) <= range.end {
                            b = a;
                            a = glyph.position.0 + sf.h_advance(glyph.id);
                            break 'c;
                        }
                    }
                }
                break 'a;
            }

            f(Vec2(a + offset, line.top), Vec2(b + offset, line.bottom));
            break;
        }
    }
}
