// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use super::Text;
use crate::shaper::{GlyphBreak, GlyphRun};
use crate::{fonts, Align, FontLibrary, Range, Vec2};
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
        // Use a crude estimate of the number of runs:
        let mut adder = LineAdder::new(self.text_len() / 16, self.env.halign, self.env.bounds.0);

        // Almost everything in "this" method depends on the line direction, so
        // we determine that then call the appropriate implementation.
        for line in self.line_runs.iter() {
            let mut append = false;
            let line_start = line.range.start();
            for (index, run) in self.glyph_runs[line.range.to_std()].iter().enumerate() {
                match line.rtl {
                    false => adder.add_ltr(&fonts, line_start + index, run, append),
                    true => adder.add_rtl(&fonts, line_start + index, run, append),
                }
                append = true;
            }
        }

        self.required = adder.finish(self.env.valign, self.env.bounds.1);
        self.wrapped_runs = adder.runs;
        self.lines = adder.lines;
        self.num_glyphs = adder.num_glyphs;
    }
}

impl LineAdder {
    fn add_ltr(&mut self, fonts: &FontLibrary, run_index: usize, run: &GlyphRun, append: bool) {
        let scale_font = fonts.get(run.font_id).scaled(run.font_scale);

        if !append || self.line_is_rtl {
            self.new_line(0.0);
        }

        let mut line_len = self.caret.0 + run.end_no_space();
        let mut glyph_end = run.glyphs.len() as u32;
        if !self.justify && line_len <= self.width_bound {
            // Short-cut: we can add the entire run.
            self.add_part(&scale_font, run_index, 0..glyph_end, line_len, false, &run);
            self.caret.0 += run.caret;
        } else if run.level.is_rtl() {
            // It makes little sense to wrap a run against its direction.
            // Instead we push the run to the next line, and if it still needs
            // splitting switch the direction of that line.

            line_len = run.end_no_space();
            if line_len < self.width_bound {
                self.new_line(0.0);
                self.add_part(&scale_font, run_index, 0..glyph_end, line_len, false, &run);
                self.caret.0 += run.caret;
            } else {
                self.add_rtl(fonts, run_index, run, false);
            }
        } else {
            // We perform line-wrapping on this run.

            let mut run_breaks = run.breaks.iter().peekable();
            let run_gb = GlyphBreak {
                pos: run.glyphs.len() as u32,
                no_space_end: run.end_no_space(),
            };
            glyph_end = 0;

            loop {
                // Find how much of the run we can add, ensuring that we
                // do not leave a line empty but otherwise respecting width.
                let mut empty = self.line_is_empty();
                let mut line_break = true;
                let glyph_start = glyph_end;
                loop {
                    let gb = run_breaks.peek().map(|gb| **gb).unwrap_or(run_gb);
                    let part_line_len = self.caret.0 + gb.no_space_end;
                    if empty || (part_line_len <= self.width_bound) {
                        empty = empty && gb.pos == glyph_end;
                        glyph_end = gb.pos;
                        run_breaks.next();
                        line_len = part_line_len;
                        if glyph_end == run_gb.pos {
                            break;
                        } else if self.justify {
                            // When justifying text we must add each word
                            // separately to allow independent positioning.
                            line_break = false;
                            break;
                        }
                        continue;
                    }
                    break;
                }

                if glyph_start < glyph_end {
                    let glyph_range = glyph_start..glyph_end;
                    self.add_part(&scale_font, run_index, glyph_range, line_len, false, &run);
                }

                // If we are already at the end of the run, stop. This
                // should not happen on the first iteration.
                if glyph_end as usize >= run.glyphs.len() {
                    self.caret.0 = line_len;
                    break;
                }

                if line_break {
                    // Offset new line since we are not at the start of the run
                    let glyph = run.glyphs[glyph_end as usize];
                    self.new_line(-glyph.position.0);
                }
            }
        }
    }

    fn add_rtl(&mut self, fonts: &FontLibrary, run_index: usize, run: &GlyphRun, append: bool) {
        let scale_font = fonts.get(run.font_id).scaled(run.font_scale);

        if !append || !self.line_is_rtl {
            self.new_line(0.0);
        }

        // In RTL mode, caret.0 starts at zero goes negative. We avoid adding
        // or subtracting width_bound until alignment since it may be infinite
        // during size requirement computations.

        let mut initial_caret = self.caret.0;
        self.caret.0 -= run.caret;
        let mut line_len = run.caret - run.start_no_space() - initial_caret;
        let glyph_end = run.glyphs.len() as u32;
        if !self.justify && line_len <= self.width_bound {
            // Short-cut: we can add the entire run.
            self.add_part(&scale_font, run_index, 0..glyph_end, line_len, true, &run);
        } else if run.level.is_ltr() {
            // It makes little sense to wrap a run against its direction.
            // Instead we push the run to the next line, and if it still needs
            // splitting switch the direction of that line.

            line_len = run.caret - run.start_no_space();
            if line_len < self.width_bound {
                self.new_line(0.0);
                self.caret.0 -= run.caret;
                self.add_part(&scale_font, run_index, 0..glyph_end, line_len, true, &run);
            } else {
                return self.add_ltr(fonts, run_index, run, false);
            }
        } else {
            // We perform line-wrapping on this run.

            let mut run_breaks = run.breaks.iter().rev().peekable();
            let run_gb = GlyphBreak {
                pos: 0,
                no_space_end: run.start_no_space(),
            };
            let mut glyph_start = run.glyphs.len() as u32;

            loop {
                // Find how much of the run we can add, ensuring that we
                // do not leave a line empty but otherwise respecting width.
                let mut empty = self.line_is_empty();
                let mut line_break = true;
                let glyph_end = glyph_start;
                loop {
                    let gb = run_breaks.peek().map(|gb| **gb).unwrap_or(run_gb);
                    let part_line_len = run.caret - gb.no_space_end - initial_caret;
                    if empty || (part_line_len <= self.width_bound) {
                        empty = empty && gb.pos == glyph_start;
                        glyph_start = gb.pos;
                        run_breaks.next();
                        line_len = part_line_len;
                        if glyph_start == 0 {
                            break;
                        } else if self.justify {
                            // When justifying text we must add each word
                            // separately to allow independent positioning.
                            line_break = false;
                            break;
                        }
                        continue;
                    }
                    break;
                }

                if glyph_start < glyph_end {
                    let glyph_range = glyph_start..glyph_end;
                    self.add_part(&scale_font, run_index, glyph_range, line_len, true, &run);
                }

                // If we are already at the end of the run, stop. This
                // should not happen on the first iteration.
                if glyph_start == 0 {
                    self.caret.0 = -line_len;
                    break;
                }

                if line_break {
                    // Offset new line since we are not at the start of the run
                    let g_pos = run
                        .glyphs
                        .get(glyph_start as usize)
                        .map(|g| g.position.0)
                        .unwrap_or(run.caret);
                    self.new_line(-g_pos);
                    initial_caret = run.caret - g_pos;
                }
            }
        }
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
    justify: bool,
    line_is_rtl: bool,
    width_bound: f32,
}
impl LineAdder {
    fn new(run_capacity: usize, halign: Align, width_bound: f32) -> Self {
        let runs = Vec::with_capacity(run_capacity);
        let justify = halign == Align::Stretch;
        LineAdder {
            runs,
            halign,
            justify,
            width_bound,
            ..Default::default()
        }
    }

    /// Does the current line have any content?
    fn line_is_empty(&self) -> bool {
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
        rtl: bool,
        run: &GlyphRun,
    ) {
        // FIXME: we may not have the full range
        let mut text_range = run.range;
        if let Some(range) = self.text_range {
            text_range.start = range.start;
        }
        self.text_range = Some(text_range);

        if self.line_is_empty() {
            self.line_is_rtl = rtl;
        }

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
            Align::Default => match self.line_is_rtl {
                false => 0.0,
                true => self.width_bound,
            },
            Align::TL => match self.line_is_rtl {
                false => 0.0,
                true => self.line_len,
            },
            Align::Centre => {
                let mut offset = 0.5 * (self.width_bound - self.line_len);
                if self.line_is_rtl {
                    offset += self.line_len;
                }
                offset
            }
            Align::BR => match self.line_is_rtl {
                false => self.width_bound - self.line_len,
                true => self.width_bound,
            },
            Align::Stretch => {
                // Justify text: expand the gaps between runs
                // We should have at least one run, so subtraction won't wrap:
                let num_gaps = self.runs.len() - self.line_start - 1;
                let per_gap = (self.width_bound - self.line_len) / (num_gaps as f32);

                if !self.line_is_rtl {
                    let mut offset = per_gap;
                    for run in &mut self.runs[(self.line_start + 1)..] {
                        run.offset.0 += offset;
                        offset += per_gap;
                    }
                } else {
                    let mut offset = self.width_bound;
                    for run in &mut self.runs[self.line_start..] {
                        run.offset.0 += offset;
                        offset -= per_gap;
                    }
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
            text_range: self.text_range.unwrap_or(Range::from(0u32..0)), // FIXME
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
