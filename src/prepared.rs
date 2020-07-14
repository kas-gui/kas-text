// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use smallvec::SmallVec;

use ab_glyph::{Font, Glyph, ScaleFont};
use glyph_brush_layout::SectionGlyph;
use unicode_segmentation::GraphemeCursor;

use crate::{fonts, rich, shaper, Align, FontId, Range, Vec2};
use crate::{Environment, UpdateEnv};

mod text_runs;
pub(crate) use text_runs::Run;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub(crate) enum Action {
    None,
    Align,
    Wrap,
    Shape,
    Runs,
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
struct LineRun {
    glyph_run: u32,
    glyph_range: Range,
    offset: Vec2,
}
type Line = SmallVec<[LineRun; 1]>;

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
    offset: Vec2,
    glyph_runs: Vec<shaper::GlyphRun>,
    lines: Vec<Line>,
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
    /// `prepared` must be called again and size-requirements may have changed.
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
    /// If this returns true, then [`Text::prepare`] must be called again.
    pub fn update_env<F: FnOnce(&mut UpdateEnv)>(&mut self, f: F) -> bool {
        let mut update = UpdateEnv::new(&mut self.env);
        f(&mut update);
        let action = update.finish().max(self.action);
        match action {
            Action::None => (),
            Action::Align => {
                self.apply_alignment();
            }
            Action::Wrap => {
                self.wrap_lines();
                self.apply_alignment();
            }
            Action::Shape | Action::Runs => {
                return true;
            }
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

        if action >= Action::Align {
            self.apply_alignment();
        }

        self.action = Action::None;

        self.apply_alignment();
    }

    /// Get an iterator over positioned glyphs
    ///
    /// One must call [`Text::prepare`] before this method.
    ///
    /// The `pos` is used to adjust the glyph position: this is the top-left
    /// position of the rect within which glyphs appear (the size of the rect
    /// is that passed to [`Text::set_bounds`]).
    pub fn positioned_glyphs(&self, pos: Vec2) -> GlyphIter {
        assert!(self.ready, "kas-text::prepared::Text: not ready");
        GlyphIter {
            offset: ab_glyph::Point::from(self.offset + pos),
            glyphs: &self.glyphs,
        }
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
    /// May panic on invalid byte index.
    ///
    /// This method is only partially compatible with mult-line text.
    /// Ideally an external line-breaker should be used.
    ///
    /// Note: if the text's bounding rect does not start at the origin, then
    /// the coordinates of the top-left corner should be added to this result.
    pub fn text_glyph_pos(&self, index: usize) -> Vec2 {
        if index == 0 {
            // Short-cut. We also cannot iterate since there may be no glyphs.
            return self.offset;
        }

        let byte = if index < self.text.len() {
            Some(self.text.as_bytes()[index])
        } else {
            None
        };

        // Translate index to part number and part's byte-index.
        let (section, section_index) = 'outer: loop {
            for run in 0..self.runs.len() {
                let range = self.runs[run].range;
                // Note: the index one-past-the-end of a run is valid!
                if range.start() <= index && index <= range.end() {
                    // NOTE: for now, a run corresponds directly to a section
                    break 'outer (run, index - range.start());
                }
            }
            // No corresponding glyph -  TODO - how do we handle this?
            panic!("no glyph");
        };

        let mut iter = self.glyphs.iter();

        let mut advance = false;
        let mut glyph;
        if let Some(byte) = byte {
            // Tiny HACK: line-breaks don't have glyphs
            if byte == b'\r' || byte == b'\n' {
                advance = true;
            }

            glyph = iter.next().unwrap().clone();
            for next in iter {
                if next.section_index > section || next.byte_index > section_index {
                    // Use the previous glyph, e.g. if in the middle of a
                    // multi-byte sequence or index is a combining diacritic.
                    break;
                }
                glyph = next.clone();
            }
        } else {
            advance = true;
            glyph = iter.last().unwrap().clone();
        }

        let mut pos = self.offset + glyph.glyph.position.into();
        let scale_font = fonts().get(glyph.font_id).into_scaled(glyph.glyph.scale);
        if advance {
            pos.0 += scale_font.h_advance(glyph.glyph.id);
        }
        pos.1 -= scale_font.ascent();
        return pos;
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
    }
}

impl Text {
    fn wrap_lines(&mut self) {
        let fonts = fonts();
        let width_bound = self.env.bounds.0;

        // Use a crude estimate of the number of lines:
        let mut lines = Vec::with_capacity(self.raw_text_len() / 16);

        #[derive(Default)]
        struct LineAdder {
            line: Line,
            ascent: f32,
            descent: f32,
            line_gap: f32,
            longest: f32,
            caret: Vec2,
        }
        impl LineAdder {
            fn is_empty(&self) -> bool {
                self.line.is_empty()
            }

            fn add_run<F: Font, SF: ScaleFont<F>, R: Into<Range>>(
                &mut self,
                scale_font: &SF,
                run_index: usize,
                glyph_range: R,
            ) {
                // Adjust vertical position if necessary
                let ascent = scale_font.ascent();
                if ascent > self.ascent {
                    let extra = ascent - self.ascent;
                    for run in &mut self.line {
                        run.offset.1 += extra;
                    }
                    self.caret.1 += extra;
                    self.ascent = ascent;
                }

                self.descent = self.descent.max(scale_font.descent());
                self.line_gap = self.line_gap.max(scale_font.line_gap());

                self.line.push(LineRun {
                    glyph_run: run_index as u32,
                    glyph_range: glyph_range.into(),
                    offset: self.caret,
                });
            }

            fn new_line(&mut self, x: f32) -> Line {
                let mut line = SmallVec::new();
                std::mem::swap(&mut self.line, &mut line);

                self.caret.0 = x;
                self.caret.1 += self.descent + self.line_gap;

                self.ascent = 0.0;
                self.descent = 0.0;
                self.line_gap = 0.0;

                line
            }

            fn finish(&mut self) -> Option<Line> {
                if self.line.is_empty() {
                    None
                } else {
                    self.caret.1 += self.descent;
                    let mut line = SmallVec::new();
                    std::mem::swap(&mut self.line, &mut line);
                    Some(line)
                }
            }
        }
        let mut line = LineAdder::default();

        for (run_index, run) in self.glyph_runs.iter().enumerate() {
            let font = fonts.get(run.font_id);
            let scale_font = font.into_scaled(run.font_scale);

            if run.glyphs.is_empty() {
                // TODO: This is a special case. We want to allow blank lines,
                // but this may need special handling.
                // TODO: special case: an empty run followed by a non-empty run on the line
                todo!()
            }

            if line.is_empty() {
                // We offset the caret horizontally based on the first glyph of the line
                if let Some(glyph) = run.glyphs.iter().as_ref().first() {
                    line.caret.0 += scale_font.h_side_bearing(glyph.id);
                }
            }

            let mut line_len = line.caret.0 + run.end_no_space;
            if line_len <= width_bound {
                // Short-cut: we can add the entire run.
                let glyph_end = run.glyphs.len() as u32;
                line.add_run(&scale_font, run_index, 0..glyph_end);
                line.caret.0 += run.caret;
                line.longest = line.longest.max(line_len);
            } else {
                // We require at least one line break.

                let mut run_breaks = run.breaks.iter().peekable();
                let mut glyph_end = 0;

                loop {
                    // Find how much of the run we can add, ensuring that we
                    // do not leave a line empty but otherwise respecting width.
                    let mut empty = line.is_empty();
                    let glyph_start = glyph_end;
                    loop {
                        if let Some(gb) = run_breaks.peek() {
                            let part_line_len = line.caret.0 + gb.end_no_space;
                            if empty || (part_line_len <= width_bound) {
                                glyph_end = gb.pos;
                                empty = false;
                                run_breaks.next();
                                line_len = part_line_len;
                                continue;
                            }
                        }
                        break;
                    }
                    line.add_run(&scale_font, run_index, glyph_start..glyph_end);
                    line.caret.0 += run.caret;
                    line.longest = line.longest.max(line_len);

                    // If we are already at the end of the run, stop. This
                    // should not happen on the first iteration.
                    if glyph_end as usize >= run.glyphs.len() {
                        break;
                    }

                    // Offset new line since we are not at the start of the run
                    let glyph = run.glyphs[glyph_end as usize];
                    let x = scale_font.h_side_bearing(glyph.id) - glyph.position.0;

                    lines.push(line.new_line(x));
                }
            }
        }

        if let Some(line) = line.finish() {
            lines.push(line);
        }

        self.lines = lines;
        self.required = Vec2(line.longest, line.caret.1);
    }

    fn apply_alignment(&mut self) {
        // TODO: this aligns the whole block of text; we should align each line
        self.offset.0 = match self.env.halign {
            Align::Default | Align::TL | Align::Stretch => 0.0,
            Align::Centre => (self.env.bounds.0 - self.required.0) / 2.0,
            Align::BR => self.env.bounds.0 - self.required.0,
        };
        self.offset.1 = match self.env.valign {
            Align::Default | Align::TL | Align::Stretch => 0.0,
            Align::Centre => (self.env.bounds.1 - self.required.1) / 2.0,
            Align::BR => self.env.bounds.1 - self.required.1,
        };
    }
}

pub struct GlyphIter<'a> {
    offset: ab_glyph::Point,
    glyphs: &'a [SectionGlyph],
}

impl<'a> Iterator for GlyphIter<'a> {
    type Item = SectionGlyph;

    fn next(&mut self) -> Option<Self::Item> {
        if self.glyphs.len() > 0 {
            let mut glyph = self.glyphs[0].clone();
            self.glyphs = &self.glyphs[1..];
            glyph.glyph.position += self.offset;
            Some(glyph)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.glyphs.len();
        (len, Some(len))
    }
}
impl<'a> ExactSizeIterator for GlyphIter<'a> {}
impl<'a> std::iter::FusedIterator for GlyphIter<'a> {}
