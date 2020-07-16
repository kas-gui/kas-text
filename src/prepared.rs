// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use ab_glyph::{Font, PxScale, ScaleFont};

use crate::{fonts, rich, shaper, FontId, Glyph, Vec2};
use crate::{Environment, UpdateEnv};

mod text_runs;
mod wrap_lines;
pub(crate) use text_runs::Run;
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
#[derive(Clone, Debug, Default)]
pub struct Text {
    env: Environment,
    text: String,
    runs: SmallVec<[Run; 1]>,
    font_id: FontId,
    action: Action,
    required: Vec2,
    glyph_runs: Vec<shaper::GlyphRun>,
    wrapped_runs: Vec<RunPart>,
    // Indexes of line-starts within wrapped_runs:
    lines: Vec<Line>,
    num_glyphs: usize,
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

    /// Length of raw text
    ///
    /// It is valid to reference text within the range `0..raw_text_len()`,
    /// even if not all text within this range will be displayed (due to runs).
    pub fn raw_text_len(&self) -> usize {
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

    /// Update the environment
    ///
    /// Returns true when [`Text::prepare`] must be called (again).
    /// Note that [`Text::prepare`] is not called automatically since it must
    /// not be called before fonts are loaded.
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) -> bool {
        let mut update = UpdateEnv::new(&mut self.env);
        f(&mut update);
        let action = update.finish().max(self.action);
        match action {
            Action::None => (),
            Action::Wrap => self.wrap_lines(),
            Action::Shape | Action::Runs => return true,
        }
        self.action = Action::None;
        false
    }

    /// Prepare
    ///
    /// Calculates glyph layouts for use.
    pub fn prepare(&mut self) {
        let action = self.action;

        if action >= Action::Runs {
            self.prepare_runs();
        }

        if action >= Action::Shape {
            self.glyph_runs = self
                .runs
                .iter()
                .map(|run| {
                    shaper::shape(self.font_id, self.env.font_scale.into(), &self.text, &run)
                })
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

        let mut glyphs = Vec::with_capacity(self.num_glyphs);
        for run_part in self.wrapped_runs.iter().cloned() {
            let run = &self.glyph_runs[run_part.glyph_run as usize];
            let font_id = run.font_id;
            let font_scale = run.font_scale;

            for mut glyph in run.glyphs[run_part.glyph_range.to_std()].iter().cloned() {
                glyph.position = glyph.position + run_part.offset;
                glyphs.push(f(text, font_id, font_scale, glyph));
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
        for run_part in &self.wrapped_runs {
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];

            let end_index = glyph_run
                .glyphs
                .get(run_part.glyph_range.end as usize)
                .map(|glyph| glyph.index)
                .unwrap_or(glyph_run.range.end) as usize;
            if index > end_index {
                continue;
            }

            let mut pos = Vec2(glyph_run.caret, 0.0);
            if index < end_index {
                for glyph in &glyph_run.glyphs[run_part.glyph_range.to_std()] {
                    if glyph.index as usize > index {
                        break;
                    }
                    pos = glyph.position;
                }
            }

            let sf = fonts()
                .get(glyph_run.font_id)
                .into_scaled(glyph_run.font_scale);
            let pos = run_part.offset + pos;
            return Some((pos, sf.ascent(), sf.descent()));
        }
        None
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

            let sf = fonts()
                .get(glyph_run.font_id)
                .into_scaled(glyph_run.font_scale);

            start_pos.1 -= sf.ascent();
            end_pos.1 -= sf.descent();
            rects.push((run_part.offset + start_pos, run_part.offset + end_pos));

            if stop {
                break;
            }
        }
        rects
    }

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// Note: if the font's rect does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        let mut start = 0;
        let mut iter = self.lines.iter();
        while let Some(line) = iter.next() {
            // Save, so that we use last line if pos lies below last
            start = line.run_range.start();
            if pos.1 <= line.bottom {
                break;
            }
        }
        let end = iter
            .next()
            .map(|line| line.run_range.start())
            .unwrap_or(self.wrapped_runs.len());

        let mut best = 0;
        let mut best_dist = f32::INFINITY;

        for run_part in &self.wrapped_runs[start..end] {
            let glyph_run = &self.glyph_runs[run_part.glyph_run as usize];
            let rel_pos = pos.0 - run_part.offset.0;

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

        best as usize
    }
}
