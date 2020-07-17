// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use super::Text;
use crate::{fonts, shaper, Align, Range, Vec2};
use ab_glyph::{Font, ScaleFont};

#[derive(Clone, Copy, Debug)]
pub struct RunPart {
    pub glyph_run: u32,
    pub glyph_range: Range,
    pub offset: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub text_range: Range, // range in text
    pub run_range: Range,  // range in wrapped_runs
    pub top: f32,
    pub bottom: f32,
}

impl Text {
    pub fn wrap_lines(&mut self) {
        let fonts = fonts();
        let width_bound = self.env.bounds.0;
        // Justified text requires adding each word separately:
        let justify = self.env.halign == Align::Stretch;

        // Use a crude estimate of the number of runs:
        let mut line = LineAdder::new(self.text_len() / 16, self.env.halign, width_bound);

        for (run_index, run) in self.glyph_runs.iter().enumerate() {
            let scale_font = fonts.get_scaled(run.font_id, run.font_scale);

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
                line.add_part(&scale_font, run_index, 0..glyph_end, line_len, run.range);
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
                        line.add_part(
                            &scale_font,
                            run_index,
                            glyph_start..glyph_end,
                            line_len,
                            run.range,
                        );
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
        self.lines = line.lines;
        self.num_glyphs = line.num_glyphs;
    }
}

#[derive(Default)]
struct LineAdder {
    runs: Vec<RunPart>,
    lines: Vec<Line>,
    line_start: usize,
    text_range: Option<Range>,
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
        LineAdder {
            runs,
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
        mut text_range: Range,
    ) {
        if let Some(range) = self.text_range {
            text_range.start = range.start;
        }
        self.text_range = Some(text_range);

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

        let top = self.caret.1 - self.ascent;
        self.caret.1 -= self.descent;
        self.longest = self.longest.max(self.line_len);
        self.lines.push(Line {
            text_range: self.text_range.unwrap(),
            run_range: (self.line_start..self.runs.len()).into(),
            top,
            bottom: self.caret.1,
        });
        self.text_range = None;
    }

    fn new_line(&mut self, x: f32) {
        self.finish_line();
        self.line_start = self.runs.len();
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
