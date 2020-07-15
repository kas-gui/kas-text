// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use ab_glyph::{Font, PxScale, ScaleFont};
// use glyph_brush_layout::SectionGlyph;
// use unicode_segmentation::GraphemeCursor;

use crate::{fonts, rich, shaper, FontId, Glyph, Range, Vec2};
use crate::{Align, Environment, UpdateEnv};

mod text_runs;
pub(crate) use text_runs::Run;

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

#[derive(Clone, Copy, Debug)]
struct RunPart {
    glyph_run: u32,
    glyph_range: Range,
    offset: Vec2,
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
    lines: Vec<u32>,
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
                .unwrap_or(glyph_run.end_index) as usize;
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

    /// Find the text index for the glyph nearest the given `pos`
    ///
    /// Note: if the font's rect does not start at the origin, then its top-left
    /// coordinate should first be subtracted from `pos`.
    ///
    /// This includes the index immediately after the last glyph, thus
    /// `result ≤ text.len()`.
    ///
    /// This method is only partially compatible with mult-line text.
    /// Ideally an external line-breaker should be used.
    pub fn text_index_nearest(&self, pos: Vec2) -> usize {
        todo!()
        /*
        if self.runs.len() == 0 {
            return 0; // short-cut
        }
        let text_len = self.text.len();
        // NOTE: if self.runs.len() > 1 then base_to_mid may change, making the
        // row selection a little inaccurate. This method is best used with only
        // a single row of text anyway, so we consider this acceptable.
        // This also affects scale_font.h_advance at line-breaks. We consider
        // this a hack anyway and so tolerate some inaccuracy.
        let scale = self.env.font_scale;
        let scale_font = fonts().get(self.font_id).into_scaled(scale);
        let base_to_mid = -0.5 * (scale_font.ascent() + scale_font.descent());
        // Note: scale_font.line_gap() is 0.0 (why?). Use ascent() instead.
        let half_line_gap = (0.5 * scale_font.ascent()).abs();

        let mut iter = self.glyphs.iter();

        // Find the (horiz, vert) distance between pos and the glyph.
        let dist = |glyph: &Glyph| {
            let p = glyph.position;
            let glyph_pos = Vec2(p.x, p.y + base_to_mid);
            (pos - glyph_pos).abs()
        };
        let test_best = |best: Vec2, glyph: &Glyph| {
            let dist = dist(glyph);
            if dist.1 < best.1 {
                Some(dist)
            } else if dist.1 == best.1 && dist.0 < best.0 {
                Some(dist)
            } else {
                None
            }
        };

        let mut last: SectionGlyph = iter.next().unwrap().clone();
        let mut last_y = last.glyph.position.y;
        let mut best = (last.byte_index, dist(&last.glyph));
        for next in iter {
            // Heuristic to detect a new line. This is a HACK to handle
            // multi-line texts since line-end positions are not represented by
            // virtual glyphs (unlike spaces).
            if (next.glyph.position.y - last_y).abs() > half_line_gap {
                last.glyph.position.x += scale_font.h_advance(last.glyph.id);
                if let Some(new_best) = test_best(best.1, &last.glyph) {
                    let index = last.byte_index;
                    let mut cursor = GraphemeCursor::new(index, text_len, true);
                    let mut cum_len = 0;
                    let text = 'outer: loop {
                        for run in &self.runs {
                            let len = run.range.len();
                            if index < cum_len + len {
                                break 'outer &self.text[run.range];
                            }
                            cum_len += len;
                        }
                        unreachable!();
                    };
                    let byte = cursor
                        .next_boundary(text, cum_len)
                        .unwrap()
                        .unwrap_or(last.byte_index);
                    best = (byte, new_best);
                }
            }

            last = next.clone();
            last_y = last.glyph.position.y;
            if let Some(new_best) = test_best(best.1, &last.glyph) {
                best = (last.byte_index, new_best);
            }
        }

        // We must also consider the position after the last glyph
        last.glyph.position.x += scale_font.h_advance(last.glyph.id);
        if let Some(new_best) = test_best(best.1, &last.glyph) {
            best = (text_len, new_best);
        }

        assert!(
            best.0 <= text_len,
            "text_index_nearest: index beyond text length!"
        );
        best.0
        */
    }
}

impl Text {
    fn wrap_lines(&mut self) {
        let fonts = fonts();
        let width_bound = self.env.bounds.0;
        // Justified text requires adding each word separately:
        let justify = self.env.halign == Align::Stretch;

        #[derive(Default)]
        struct LineAdder {
            runs: Vec<RunPart>,
            line_starts: Vec<u32>,
            line_start: usize,
            line_len: f32,
            ascent: f32,
            descent: f32,
            line_gap: f32,
            longest: f32,
            caret: Vec2,
            num_glyphs: usize,
            halign: Align,
            width_bound: f32,
        }
        impl LineAdder {
            fn new(run_capacity: usize, halign: Align, width_bound: f32) -> Self {
                let runs = Vec::with_capacity(run_capacity);
                let line_starts = vec![0];
                LineAdder {
                    runs,
                    line_starts,
                    halign,
                    width_bound,
                    ..Default::default()
                }
            }

            /// Does the current line have any content?
            fn is_empty(&self) -> bool {
                self.runs[self.line_start..]
                    .iter()
                    .all(|run| run.glyph_range.len() == 0)
            }

            fn add_part<F: Font, SF: ScaleFont<F>>(
                &mut self,
                scale_font: &SF,
                run_index: usize,
                glyph_range: std::ops::Range<u32>,
                line_len: f32,
            ) {
                // Adjust vertical position if necessary
                let ascent = scale_font.ascent();
                if ascent > self.ascent {
                    let extra = ascent - self.ascent;
                    for run in &mut self.runs[self.line_start..] {
                        run.offset.1 += extra;
                    }
                    self.caret.1 += extra;
                    self.ascent = ascent;
                }

                self.descent = self.descent.min(scale_font.descent());
                self.line_gap = self.line_gap.max(scale_font.line_gap());

                self.num_glyphs += glyph_range.len();
                self.runs.push(RunPart {
                    glyph_run: run_index as u32,
                    glyph_range: glyph_range.into(),
                    offset: self.caret,
                });

                self.line_len = line_len;
            }

            fn finish_line(&mut self) {
                let offset = match self.halign {
                    // TODO(bidi): Default and Stretch depend on text direction
                    Align::Default | Align::TL => 0.0,
                    Align::Centre => 0.5 * (self.width_bound - self.line_len),
                    Align::BR => self.width_bound - self.line_len,
                    Align::Stretch => {
                        // Justify text: expand the gaps between runs
                        // We should have at least one run, so subtraction won't wrap:
                        let num_gaps = self.runs.len() - self.line_start - 1;
                        let per_gap = (self.width_bound - self.line_len) / (num_gaps as f32);

                        let mut i = 1;
                        for run in &mut self.runs[(self.line_start + 1)..] {
                            run.offset.0 += per_gap * i as f32;
                            i += 1;
                        }

                        0.0 // do not offset below
                    }
                };
                if offset != 0.0 {
                    for run in &mut self.runs[self.line_start..] {
                        run.offset.0 += offset;
                    }
                }

                self.caret.1 -= self.descent;
                self.longest = self.longest.max(self.line_len);
            }

            fn new_line(&mut self, x: f32) {
                self.finish_line();
                self.line_start = self.runs.len();
                self.line_starts.push(self.line_start as u32);
                self.caret.0 = x;
                self.caret.1 += self.line_gap;

                self.ascent = 0.0;
                self.descent = 0.0;
                self.line_gap = 0.0;
            }

            // Returns: required dimensions
            fn finish(&mut self, valign: Align, height_bound: f32) -> Vec2 {
                // If any (even empty) run was added to the line, add v-space
                if self.line_start != self.runs.len() {
                    self.finish_line();
                }

                let height = self.caret.1;
                let offset = match valign {
                    Align::Default | Align::TL | Align::Stretch => 0.0, // nothing to do
                    Align::Centre => 0.5 * (height_bound - height),
                    Align::BR => height_bound - height,
                };
                if offset != 0.0 {
                    for run in &mut self.runs {
                        run.offset.1 += offset;
                    }
                }

                Vec2(self.longest, height)
            }
        }
        // Use a crude estimate of the number of runs:
        let mut line = LineAdder::new(self.raw_text_len() / 16, self.env.halign, width_bound);

        for (run_index, run) in self.glyph_runs.iter().enumerate() {
            let font = fonts.get(run.font_id);
            let scale_font = font.into_scaled(run.font_scale);

            if !line.runs.is_empty() && !run.append_to_prev {
                line.new_line(0.0);
            }

            if line.is_empty() {
                // We offset the caret horizontally based on the first glyph of the line
                if let Some(glyph) = run.glyphs.iter().as_ref().first() {
                    line.caret.0 += scale_font.h_side_bearing(glyph.id);
                }
            }

            let mut line_len = line.caret.0 + run.end_no_space;
            if !justify && line_len <= width_bound {
                // Short-cut: we can add the entire run.
                let glyph_end = run.glyphs.len() as u32;
                line.add_part(&scale_font, run_index, 0..glyph_end, line_len);
                line.caret.0 += run.caret;
            } else {
                // We require at least one line break.

                let mut run_breaks = run.breaks.iter().peekable();
                let run_gb = shaper::GlyphBreak {
                    pos: run.glyphs.len() as u32,
                    end_no_space: run.end_no_space,
                };
                let mut glyph_end = 0;

                loop {
                    // Find how much of the run we can add, ensuring that we
                    // do not leave a line empty but otherwise respecting width.
                    let mut empty = line.is_empty();
                    let mut add_run = false;
                    let mut line_break = true;
                    let glyph_start = glyph_end;
                    loop {
                        let gb = run_breaks.peek().map(|gb| **gb).unwrap_or(run_gb);
                        let part_line_len = line.caret.0 + gb.end_no_space;
                        if empty || (part_line_len <= width_bound) {
                            empty = empty && gb.pos == glyph_end;
                            add_run = true;
                            glyph_end = gb.pos;
                            run_breaks.next();
                            line_len = part_line_len;
                            if glyph_end == run_gb.pos {
                                break;
                            } else if justify {
                                // When justifying text we must add each word
                                // separately to allow independent positioning.
                                line_break = false;
                                break;
                            }
                            continue;
                        }
                        break;
                    }

                    if add_run {
                        line.add_part(&scale_font, run_index, glyph_start..glyph_end, line_len);
                    }

                    // If we are already at the end of the run, stop. This
                    // should not happen on the first iteration.
                    if glyph_end as usize >= run.glyphs.len() {
                        line.caret.0 = line_len;
                        break;
                    }

                    if line_break {
                        // Offset new line since we are not at the start of the run
                        let glyph = run.glyphs[glyph_end as usize];
                        let x = scale_font.h_side_bearing(glyph.id) - glyph.position.0;
                        line.new_line(x);
                    }
                }
            }
        }

        self.required = line.finish(self.env.valign, self.env.bounds.1);
        self.wrapped_runs = line.runs;
        self.lines = line.line_starts;
        self.num_glyphs = line.num_glyphs;
    }
}
