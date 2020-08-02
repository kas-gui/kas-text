// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use ab_glyph::{PxScale, ScaleFont};

use crate::{fonts, rich, shaper, FontId, Glyph, Vec2};
use crate::{Environment, UpdateEnv};

mod text_runs;
mod wrap_lines;
pub(crate) use text_runs::{LineRun, Run};
use wrap_lines::{Line, RunPart};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(crate) enum Action {
    None,  // do nothing
    Wrap,  // do line-wrapping and alignment
    Shape, // do text shaping and above
    Runs,  // do splitting into runs, BIDI and above
}

impl Default for Action {
    fn default() -> Self {
        Action::None
    }
}

impl Action {
    fn is_none(&self) -> bool {
        *self == Action::None
    }
}

/// Text, prepared for display in a given enviroment
///
/// Text is laid out for display in a box with the given size bounds.
///
/// The type can be default-constructed with no text.
///
/// The text range is `0..self.text_len()`. Any index within this range
/// (inclusive of end point) is a code-point boundary is valid for use in all
/// functions taking an index. (Most of the time, non-code-point boundaries
/// are also accepted.)
///
/// Navigating to the start or end of a line can be done with [`Text::find_line`].
///
/// The functions [`Text::nav_left`] and [`Text::nav_right`] allow navigating
/// one *glyph* left or right. This may not be what you want: ligatures count
/// as a single glyph and combining diacritics may count as separate glyphs.
/// Alternatively, one may use a library such as `unicode-segmentation` which
/// provides a `GraphemeCursor` to step back/forward one "grapheme".
///
/// Navigating "up" and "down" is trickier; use [`Text::text_glyph_pos`]
/// to get the position of the cursor, [`Text::find_line`] to get the line
/// number, then [`Text::line_index_nearest`] to find the new index.
#[derive(Clone, Debug, Default)]
pub struct Text {
    env: Environment,
    /// Contiguous text in logical order
    text: String,
    /// Level runs within the text, in logical order
    runs: SmallVec<[Run; 1]>,
    /// Subsets of runs forming a line, with line direction
    line_runs: SmallVec<[LineRun; 1]>,
    font_id: FontId,
    action: Action,
    required: Vec2,
    /// Runs of glyphs (same order as `runs` sequence)
    glyph_runs: Vec<shaper::GlyphRun>,
    /// Contiguous runs, in logical order
    ///
    /// Within a line, runs may not be in visual order due to BIDI reversals.
    wrapped_runs: Vec<RunPart>,
    /// Visual (wrapped) lines, in visual and logical order
    lines: Vec<Line>,
    num_glyphs: u32,
}

impl Text {
    /// Construct from an environment and a text model
    ///
    /// This struct must be made ready for use before
    /// To do so, call [`Text::prepare`].
    pub fn new(env: Environment, text: rich::Text) -> Text {
        let mut result = Text {
            env,
            ..Default::default()
        };
        result.set_text(text);
        result
    }

    /// Reconstruct the [`rich::Text`] model defining this `Text`
    pub fn clone_text(&self) -> rich::Text {
        rich::Text {
            text: self.text.clone(),
        }
    }

    /// Length of text
    ///
    /// It is valid to reference text within the range `0..text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    pub fn text_len(&self) -> usize {
        self.text.len()
    }

    /// Set the text
    ///
    /// Returns true when the contents have changed, in which case
    /// [`Text::prepare`] must be called and size-requirements may have changed.
    /// Note that [`Text::prepare`] is not called automatically since it must
    /// not be called before fonts are loaded.
    pub fn set_text(&mut self, text: rich::Text) -> bool {
        if self.text == text.text && self.runs.len() == 1 {
            return false; // no change
        }

        self.text = text.text;
        self.font_id = Default::default();
        self.prepare_runs();
        self.action = Action::Shape;
        true
    }

    /// Read the environment
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Update the environment and prepare for drawing
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) {
        let mut update = UpdateEnv::new(&mut self.env);
        f(&mut update);
        let action = update.finish().max(self.action);
        if action == Action::None {
            return;
        }

        if action >= Action::Runs {
            self.prepare_runs();
        }

        if action >= Action::Shape {
            let dpem = self.env.pt_size * self.env.dpp;
            self.glyph_runs = self
                .runs
                .iter()
                .map(|run| shaper::shape(self.font_id, dpem, &self.text, &run))
                .collect();
        }

        if action >= Action::Wrap {
            self.wrap_lines();
        }

        self.action = Action::None;
    }

    /// Produce a list of positioned glyphs
    ///
    /// One must call [`Text::prepare`] before this method.
    ///
    /// The function `f` may be used to apply required type transformations.
    /// Note that since internally this `Text` type assumes text is between the
    /// origin and the bound given by the environment, the function may also
    /// wish to translate glyphs.
    ///
    /// Although glyph positions are already calculated prior to calling this
    /// method, it is still computationally-intensive enough that it may be
    /// worth caching the result for reuse. Since the type is defined by the
    /// function `f`, caching is left to the caller.
    pub fn positioned_glyphs<G, F: Fn(&str, FontId, PxScale, Glyph) -> G>(&self, f: F) -> Vec<G> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        let text = &self.text;

        let mut glyphs = Vec::with_capacity(self.num_glyphs as usize);
        for run_part in self.wrapped_runs.iter().cloned() {
            let run = &self.glyph_runs[run_part.glyph_run as usize];
            let font_id = run.font_id;
            let font_scale = run.font_scale;

            for mut glyph in run.glyphs[run_part.glyph_range.to_std()].iter().cloned() {
                glyph.position = glyph.position + run_part.offset;
                glyphs.push(f(text, font_id, font_scale.into(), glyph));
            }
        }
        glyphs
    }

    /// Calculate size requirements
    ///
    /// One must set initial size bounds and call [`Text::prepare`]
    /// before this method. Note that initial size bounds may cause wrapping
    /// and may cause parts of the text outside the bounds to be cut off.
    pub fn required_size(&self) -> Vec2 {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        self.required
    }

    /// Get the number of lines
    pub fn num_lines(&self) -> usize {
        self.lines.len()
    }

    /// Find the line containing text `index`
    ///
    /// Returns the line number and the text-range of the line.
    ///
    /// Returns `None` in case `index` does not line on or at the end of a line
    /// (which means either that `index` is beyond the end of the text or that
    /// `index` is within a mult-byte line break).
    pub fn find_line(&self, index: usize) -> Option<(usize, std::ops::Range<usize>)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        for (n, line) in self.lines.iter().enumerate() {
            if line.text_range.includes(index) {
                return Some((n, line.text_range.to_std()));
            }
        }
        None
    }

    /// Get the range of a line, by line number
    pub fn line_range(&self, line: usize) -> Option<std::ops::Range<usize>> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        self.lines.get(line).map(|line| line.text_range.to_std())
    }

    /// Find the next glyph index to the left of the given glyph
    ///
    /// Warning: this counts *glyphs*, which makes the result dependent on
    /// text shaping. Combining diacritics *may* count as discrete glyphs.
    /// Ligatures will count as a single glyph.
    pub fn nav_left(&self, index: usize) -> usize {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");

        for (ri, glyph_run) in self.glyph_runs.iter().enumerate() {
            if index > glyph_run.range.end() {
                continue;
            }

            let mut gi = glyph_run.glyphs.len();
            if index < glyph_run.range.end() {
                gi = 0;
                for (i, glyph) in glyph_run.glyphs.iter().enumerate() {
                    if glyph.index as usize > index {
                        break;
                    }
                    gi = i;
                }
            }

            if gi > 0 {
                return glyph_run.glyphs[gi - 1].index as usize;
            } else if ri > 0 {
                // We already know index > glyph_run.range.end()
                return self.glyph_runs[ri - 1].range.end();
            } else {
                // we can't go left of the first run!
                return 0;
            }
        }

        // We are beyond the end.
        self.text.len()
    }

    /// Find the next glyph index to the right of the given glyph
    ///
    /// Warning: this counts *glyphs*, which makes the result dependent on
    /// text shaping. Combining diacritics *may* count as discrete glyphs.
    /// Ligatures will count as a single glyph.
    pub fn nav_right(&self, index: usize) -> usize {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");

        for (mut ri, glyph_run) in self.glyph_runs.iter().enumerate().rev() {
            if index < glyph_run.range.start() {
                continue;
            }

            if index < glyph_run.range.end() {
                let mut gi = 0;
                for (i, glyph) in glyph_run.glyphs.iter().enumerate() {
                    if glyph.index as usize > index {
                        break;
                    }
                    gi = i;
                }

                gi += 1;
                if gi < glyph_run.glyphs.len() {
                    return glyph_run.glyphs[gi].index as usize;
                } else {
                    return glyph_run.range.end();
                }
            }

            if index == glyph_run.range.end() {
                ri += 1;
                if ri < self.glyph_runs.len() {
                    // We already know index < glyph_run.range.start()
                    return self.glyph_runs[ri].range.start();
                } else {
                    return self.text.len();
                }
            }
        }

        // There should not be any position to the left of the first run.
        // (If this *is* possible, then certain other nav functions need fixing.)
        panic!("kas-text bug!");
    }

    /// Find the starting position (top-left) of the glyph at the given index
    ///
    /// Returns `Some(pos, ascent, descent)` on success, where `pos.1 - ascent`
    /// and `pos.1 - descent` are the top and bottom of the glyph position.
    ///
    /// Note that this only searches *visible* text sections for a valid index.
    /// In case the `index` is not within a slice of visible text, this returns
    /// `None`. So long as `index` is within a visible slice (or at its end),
    /// it does not need to be on a valid code-point.
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to this result.
    pub fn text_glyph_pos(&self, index: usize) -> Option<(Vec2, f32, f32)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");

        // We don't care too much about performance: use a naive search strategy
        'a: for run_part in &self.wrapped_runs {
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];

            if !glyph_run.range.includes(index) {
                continue;
            }
            let mut pos = Vec2(glyph_run.caret, 0.0);
            'b: loop {
                if index == glyph_run.range.end() {
                    break 'b;
                }

                let run_end = run_part.glyph_range.end();
                if run_end < glyph_run.glyphs.len() {
                    let glyph = glyph_run.glyphs[run_end];
                    if index > glyph.index as usize {
                        continue 'a;
                    } else if index == glyph.index as usize {
                        pos = glyph.position;
                        break 'b;
                    }
                }

                for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                    if glyph.index as usize > index {
                        break;
                    }
                    pos = glyph.position;
                }
                break 'b;
            }

            let sf = fonts().get(glyph_run.font_id).scaled(glyph_run.font_scale);
            let pos = run_part.offset + pos;
            return Some((pos, sf.ascent(), sf.descent()));
        }
        None
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// Note: if the font's rect does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        let mut n = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.top > pos.1 {
                break;
            }
            n = i;
        }
        // We now know that `n` is a valid line (we must have at least one).
        self.line_index_nearest(n, pos.0).unwrap()
    }

    /// Find the text index nearest horizontal-coordinate `x` on `line`
    ///
    /// This is similar to [`Text::text_index_nearest`], but allows the line to
    /// be specified explicitly. Returns `None` only on invalid `line`.
    pub fn line_index_nearest(&self, line: usize, x: f32) -> Option<usize> {
        if line >= self.lines.len() {
            return None;
        }
        let line = self.lines[line];
        let run_range = line.run_range.to_std();

        let mut best = line.text_range.start;
        let mut best_dist = f32::INFINITY;

        for run_part in &self.wrapped_runs[run_range] {
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
            let rel_pos = x - run_part.offset.0;

            for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                let dist = (glyph.position.0 - rel_pos).abs();
                if dist < best_dist {
                    best = glyph.index;
                    best_dist = dist;
                }
            }

            let dist = (glyph_run.caret - rel_pos).abs();
            if dist < best_dist {
                best = glyph_run.range.end;
                best_dist = dist;
            }
        }

        Some(best as usize)
    }

    /// Yield a sequence of rectangles to highlight a given range, by lines
    ///
    /// Rectangles span to end and beginning of lines when wrapping lines.
    ///
    /// This locates the ends of a range as with [`Text::text_glyph_pos`], but
    /// yields a separate rect for each "run" within this range (where "run" is
    /// is a line or part of a line). Rects are represented by the top-left
    /// vertex and the bottom-right vertex.
    pub fn highlight_lines<R: Into<std::ops::Range<usize>>>(&self, range: R) -> Vec<(Vec2, Vec2)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        let range = range.into();
        if range.len() == 0 {
            return vec![];
        }

        let mut lines = self.lines.iter();
        let mut rects = Vec::with_capacity(self.lines.len());

        let mut cur_line = 'l1: loop {
            while let Some(line) = lines.next() {
                if line.text_range.includes(range.start) {
                    break 'l1 line;
                }
            }
            return vec![];
        };
        let mut tl = Vec2(0.0, cur_line.top);

        if range.start > cur_line.text_range.start() {
            for run_part in &self.wrapped_runs[cur_line.run_range.to_std()] {
                let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];

                if range.start <= glyph_run.range.end() {
                    let mut pos = glyph_run.caret;
                    if range.start < glyph_run.range.end() {
                        pos = 0.0;
                        for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                            if glyph.index as usize > range.start {
                                break;
                            }
                            pos = glyph.position.0;
                        }
                    }
                    tl.0 = run_part.offset.0 + pos;
                    break;
                }
            }
        }

        let mut br = Vec2(self.env.bounds.0, cur_line.bottom);
        if range.end > cur_line.text_range.end() {
            rects.push((tl, br));
            tl = Vec2(0.0, cur_line.bottom);

            while let Some(line) = lines.next() {
                if range.end <= line.text_range.end() {
                    cur_line = line;
                    break;
                }

                br.1 = line.bottom;
            }

            if br.1 > tl.1 {
                rects.push((tl, br));
                tl = Vec2(0.0, cur_line.top);
            }
        }

        if cur_line.text_range.includes(range.end) {
            br.1 = cur_line.bottom;
            for run_part in &self.wrapped_runs[cur_line.run_range.to_std()] {
                let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];

                if range.end <= glyph_run.range.end() {
                    let mut pos = glyph_run.caret;
                    if range.end < glyph_run.range.end() {
                        let glyph_range = run_part.glyph_range.to_std();
                        for glyph in glyph_run.glyphs[glyph_range].iter().rev() {
                            if range.end > glyph.index as usize {
                                break;
                            }
                            pos = glyph.position.0;
                        }
                    }
                    br.0 = run_part.offset.0 + pos;
                    break;
                }
            }
            rects.push((tl, br));
        }

        // TODO(opt): length is at most 3. Use a different data type?
        rects
    }

    /// Yield a sequence of rectangles to highlight a given range, by runs
    ///
    /// Rectangles tightly fit each "run" (piece) of text highlighted.
    ///
    /// This locates the ends of a range as with [`Text::text_glyph_pos`], but
    /// yields a separate rect for each "run" within this range (where "run" is
    /// is a line or part of a line). Rects are represented by the top-left
    /// vertex and the bottom-right vertex.
    pub fn highlight_runs<R: Into<std::ops::Range<usize>>>(&self, range: R) -> Vec<(Vec2, Vec2)> {
        assert!(self.action.is_none(), "kas-text::prepared::Text: not ready");
        let range = range.into();
        if range.len() == 0 {
            return vec![];
        }

        let mut rects = Vec::with_capacity(self.wrapped_runs.len());
        for run_part in &self.wrapped_runs {
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];

            let end_index = glyph_run
                .glyphs
                .get(run_part.glyph_range.end as usize)
                .map(|glyph| glyph.index)
                .unwrap_or(glyph_run.range.end) as usize;
            if end_index <= range.start {
                continue;
            }

            let glyph_range = run_part.glyph_range.to_std();

            let start;
            let mut start_pos = 'l1: loop {
                let mut pos = Vec2::ZERO;
                for (i, glyph) in glyph_run.glyphs[glyph_range.clone()].iter().enumerate() {
                    if glyph.index as usize > range.start {
                        start = i;
                        break 'l1 pos;
                    }
                    pos = glyph.position;
                }
                start = glyph_range.end;
                break pos;
            };

            let mut stop = false;
            let mut end_pos = if end_index <= range.end {
                Vec2(glyph_run.caret, 0.0)
            } else {
                'l2: loop {
                    let mut pos = start_pos;
                    for glyph in &glyph_run.glyphs[start..glyph_range.end] {
                        if glyph.index as usize > range.end {
                            stop = true;
                            break 'l2 pos;
                        }
                        pos = glyph.position;
                    }
                    break Vec2(glyph_run.caret, 0.0);
                }
            };

            let sf = fonts().get(glyph_run.font_id).scaled(glyph_run.font_scale);
            start_pos.1 -= sf.ascent();
            end_pos.1 -= sf.descent();
            rects.push((run_part.offset + start_pos, run_part.offset + end_pos));

            if stop {
                break;
            }
        }
        rects
    }
}
