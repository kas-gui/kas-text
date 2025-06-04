// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Methods using positioned glyphs

#![allow(clippy::collapsible_else_if)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::never_loop)]
#![allow(clippy::needless_range_loop)]

use super::{Line, TextDisplay};
use crate::conv::to_usize;
use crate::fonts::{self, FaceId};
use crate::{shaper, Glyph, Range, Vec2};

/// Effect formatting marker
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Effect<X> {
    /// Index in text at which formatting becomes active
    ///
    /// (Note that we use `u32` not `usize` since it can be assumed text length
    /// will never exceed `u32::MAX`.)
    pub start: u32,
    /// Effect flags
    pub flags: EffectFlags,
    /// User payload
    pub aux: X,
}

impl<X> Effect<X> {
    /// Construct a "default" instance with the supplied `aux` value
    pub fn default(aux: X) -> Self {
        Effect {
            start: 0,
            flags: EffectFlags::empty(),
            aux,
        }
    }
}

bitflags::bitflags! {
    /// Text effects
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct EffectFlags: u32 {
        /// Glyph is underlined
        const UNDERLINE = 1 << 0;
        /// Glyph is crossed through by a center-line
        const STRIKETHROUGH = 1 << 1;
    }
}

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

/// A sequence of positioned glyphs
///
/// Yielded by [`TextDisplay::runs`].
pub struct GlyphRun<'a> {
    run: &'a shaper::GlyphRun,
    range: Range,
    offset: Vec2,
}

impl<'a> GlyphRun<'a> {
    /// Get the font face used for this run
    #[inline]
    pub fn face_id(&self) -> FaceId {
        self.run.face_id
    }

    /// Get the font size used for this run
    ///
    /// Units are dots-per-Em (see [crate::fonts]).
    #[inline]
    pub fn dpem(&self) -> f32 {
        self.run.dpem
    }

    /// Get an iterator over glyphs for this run
    pub fn glyphs(&self) -> impl Iterator<Item = Glyph> + '_ {
        self.run.glyphs[self.range.to_std()]
            .iter()
            .map(|glyph| Glyph {
                index: glyph.index,
                id: glyph.id,
                position: glyph.position + self.offset,
            })
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
            let pos = 'b: loop {
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

    /// Get the number of glyphs
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    ///
    /// This method is a simple memory-read.
    #[inline]
    #[cfg(feature = "num_glyphs")]
    pub fn num_glyphs(&self) -> usize {
        to_usize(self.num_glyphs)
    }

    /// Iterate over runs of positioned glyphs
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// Runs are yielded in undefined order. The total number of
    /// glyphs yielded will equal [`TextDisplay::num_glyphs`].
    pub fn runs(&self) -> impl Iterator<Item = GlyphRun<'_>> {
        self.wrapped_runs.iter().map(|part| GlyphRun {
            run: &self.runs[to_usize(part.glyph_run)],
            range: part.glyph_range,
            offset: part.offset,
        })
    }

    /// Like [`TextDisplay::glyphs`] but with added effects
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// If the list `effects` is empty or has first entry with `start > 0`, the
    /// result of `Effect::default(default_aux)` is used. The user payload of
    /// type `X` is simply passed through to `f` and `g` calls and may be useful
    /// for color information.
    ///
    /// The callback `f` receives `face_id, dpem, glyph, i, aux` where
    /// `dpu` and `height` are both measures of the font size (pixels per font
    /// unit and pixels per height, respectively), and `i` is the index within
    /// `effects` (or `usize::MAX` when a default-constructed effect token is
    /// used).
    ///
    /// The callback `g` receives positioning for each underline/strike-through
    /// segment: `x1, x2, y_top, h` where `h` is the thickness (height). Note
    /// that it is possible to have `h < 1.0` and `y_top, y_top + h` to round to
    /// the same number; the renderer is responsible for ensuring such lines
    /// are actually visible. The last parameters are `i, aux` as for `f`.
    ///
    /// Note: this is significantly more computationally expensive than
    /// [`TextDisplay::glyphs`]. Optionally one may choose to cache the result,
    /// though this is not really necessary.
    pub fn glyphs_with_effects<X, F, G>(
        &self,
        effects: &[Effect<X>],
        default_aux: X,
        mut f: F,
        mut g: G,
    ) where
        X: Copy,
        F: FnMut(FaceId, f32, Glyph, usize, X),
        G: FnMut(f32, f32, f32, f32, usize, X),
    {
        let fonts = fonts::library();

        let mut effect_cur = usize::MAX;
        let mut effect_next = 0;
        let mut next_start = effects
            .get(effect_next)
            .map(|e| e.start)
            .unwrap_or(u32::MAX);

        // self.wrapped_runs is in logical order
        for run_part in self.wrapped_runs.iter().cloned() {
            if run_part.glyph_range.is_empty() {
                continue;
            }

            let run = &self.runs[to_usize(run_part.glyph_run)];
            let face_id = run.face_id;
            let dpem = run.dpem;

            let mut underline = None;
            let mut strikethrough = None;

            let ltr = run.level.is_ltr();
            if !ltr {
                let last_index = run.glyphs[run_part.glyph_range.start()].index;
                while effects
                    .get(effect_next)
                    .map(|e| e.start <= last_index)
                    .unwrap_or(false)
                {
                    effect_cur = effect_next;
                    effect_next += 1;
                }
                next_start = effects
                    .get(effect_next)
                    .map(|e| e.start)
                    .unwrap_or(u32::MAX);
            }
            let mut fmt = effects
                .get(effect_cur)
                .cloned()
                .unwrap_or(Effect::default(default_aux));

            if !fmt.flags.is_empty() {
                let sf = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                let glyph = &run.glyphs[run_part.glyph_range.start()];
                let position = glyph.position + run_part.offset;
                if fmt.flags.contains(EffectFlags::UNDERLINE) {
                    if let Some(metrics) = sf.underline_metrics() {
                        let y_top = position.1 - metrics.position;
                        let h = metrics.thickness;
                        let x1 = position.0;
                        underline = Some((x1, y_top, h, fmt.aux));
                    }
                }
                if fmt.flags.contains(EffectFlags::STRIKETHROUGH) {
                    if let Some(metrics) = sf.strikethrough_metrics() {
                        let y_top = position.1 - metrics.position;
                        let h = metrics.thickness;
                        let x1 = position.0;
                        strikethrough = Some((x1, y_top, h, fmt.aux));
                    }
                }
            }

            for mut glyph in run.glyphs[run_part.glyph_range.to_std()].iter().cloned() {
                glyph.position += run_part.offset;

                // If run is RTL, glyph index is decreasing
                if (ltr && next_start <= glyph.index) || (!ltr && fmt.start > glyph.index) {
                    if ltr {
                        loop {
                            effect_cur = effect_next;
                            effect_next += 1;
                            if effects
                                .get(effect_next)
                                .map(|e| e.start > glyph.index)
                                .unwrap_or(true)
                            {
                                break;
                            }
                        }
                        next_start = effects
                            .get(effect_next)
                            .map(|e| e.start)
                            .unwrap_or(u32::MAX);
                    } else {
                        loop {
                            effect_cur = effect_cur.wrapping_sub(1);
                            if effects.get(effect_cur).map(|e| e.start).unwrap_or(0) <= glyph.index
                            {
                                break;
                            }
                        }
                    }
                    fmt = effects
                        .get(effect_cur)
                        .cloned()
                        .unwrap_or(Effect::default(default_aux));

                    if underline.is_some() != fmt.flags.contains(EffectFlags::UNDERLINE) {
                        let sf = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                        if let Some((x1, y_top, h, aux)) = underline {
                            let x2 = glyph.position.0;
                            g(x1, x2, y_top, h, effect_cur, aux);
                            underline = None;
                        } else if let Some(metrics) = sf.underline_metrics() {
                            let y_top = glyph.position.1 - metrics.position;
                            let h = metrics.thickness;
                            let x1 = glyph.position.0;
                            underline = Some((x1, y_top, h, fmt.aux));
                        }
                    }
                    if strikethrough.is_some() != fmt.flags.contains(EffectFlags::STRIKETHROUGH) {
                        let sf = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                        if let Some((x1, y_top, h, aux)) = strikethrough {
                            let x2 = glyph.position.0;
                            g(x1, x2, y_top, h, effect_cur, aux);
                            strikethrough = None;
                        } else if let Some(metrics) = sf.strikethrough_metrics() {
                            let y_top = glyph.position.1 - metrics.position;
                            let h = metrics.thickness;
                            let x1 = glyph.position.0;
                            strikethrough = Some((x1, y_top, h, fmt.aux));
                        }
                    }
                }

                f(face_id, dpem, glyph, effect_cur, fmt.aux);
            }

            // In case of RTL, we need to correct the value for the next run
            effect_cur = effect_next.wrapping_sub(1);

            if let Some((x1, y_top, h, aux)) = underline {
                let x2 = if run_part.glyph_range.end() < run.glyphs.len() {
                    run.glyphs[run_part.glyph_range.end()].position.0
                } else {
                    run.caret
                } + run_part.offset.0;
                g(x1, x2, y_top, h, effect_cur, aux);
            }
            if let Some((x1, y_top, h, aux)) = strikethrough {
                let x2 = if run_part.glyph_range.end() < run.glyphs.len() {
                    run.glyphs[run_part.glyph_range.end()].position.0
                } else {
                    run.caret
                } + run_part.offset.0;
                g(x1, x2, y_top, h, effect_cur, aux);
            }
        }
    }

    /// Yield a sequence of rectangles to highlight a given text range
    ///
    /// [Requires status][Self#status-of-preparation]:
    /// text is fully prepared for display.
    ///
    /// Calls `f(top_left, bottom_right)` for each highlighting rectangle.
    pub fn highlight_range(&self, range: std::ops::Range<usize>, f: &mut dyn FnMut(Vec2, Vec2)) {
        for line in &self.lines {
            let line_range: std::ops::Range<usize> = line.text_range.into();
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
            'c: loop {
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
