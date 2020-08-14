// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text prepared for display

use super::Text;
use crate::shaper::{GlyphBreak, GlyphRun};
use crate::{fonts, Align, Environment, FontLibrary, Range, Vec2};
use ab_glyph::{Font, ScaleFont};
use unicode_bidi::Level;

#[derive(Clone, Copy, Debug)]
pub struct RunPart {
    pub text_end: u32,
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
    pub(crate) fn wrap_lines(&mut self) {
        let fonts = fonts();
        // Use a crude estimate of the number of runs:
        let mut adder = LineAdder::new(self.text_len() / 16, &self.env);

        // Almost everything in "this" method depends on the line direction, so
        // we determine that then call the appropriate implementation.
        for line in self.line_runs.iter() {
            let mut append = false;
            let mut index = line.range.start();
            while index < line.range.end() {
                let run = &self.glyph_runs[index];
                match line.rtl {
                    false => adder.add_ltr(&fonts, index, run, append),
                    true => adder.add_rtl(&fonts, index, run, append),
                }
                append = true;
                index += 1;
            }
        }

        self.required = adder.finish(self.text.len() as u32, self.env.valign, self.env.bounds.1);
        self.wrapped_runs = adder.runs;
        self.lines = adder.lines;
        self.num_glyphs = adder.num_glyphs;
    }
}

impl LineAdder {
    fn add_ltr(&mut self, fonts: &FontLibrary, run_index: usize, run: &GlyphRun, append: bool) {
        let scale_font = fonts.get(run.font_id).scaled(run.font_scale);

        if !append || self.line_is_rtl {
            self.new_line(run.range.start, 0.0);
        }

        let mut line_len = self.caret.0 + run.end_no_space();
        let mut glyph_end = run.glyphs.len() as u32;
        if !self.wrap || (!self.justify && line_len <= self.width_bound) {
            // Short-cut: we can add the entire run.
            self.prep_add(&scale_font, line_len, false, &run);
            self.add_part(run.range.end, run_index, 0..glyph_end);
            self.caret.0 += run.caret;
        } else if run.level.is_rtl() {
            // It makes little sense to wrap a run against its direction.
            // Instead we push the run to the next line, and if it still needs
            // splitting switch the direction of that line.

            line_len = run.end_no_space();
            if line_len < self.width_bound {
                self.new_line(run.range.start, 0.0);
                self.prep_add(&scale_font, line_len, false, &run);
                self.add_part(run.range.end, run_index, 0..glyph_end);
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
                    self.prep_add(&scale_font, line_len, false, &run);
                    let text_end = run
                        .glyphs
                        .get(glyph_end as usize)
                        .map(|g| g.index)
                        .unwrap_or(run.range.end);
                    self.add_part(text_end, run_index, glyph_start..glyph_end);
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
                    self.new_line(glyph.index, -glyph.position.0);
                }
            }
        }
    }

    fn add_rtl(&mut self, fonts: &FontLibrary, run_index: usize, run: &GlyphRun, append: bool) {
        let scale_font = fonts.get(run.font_id).scaled(run.font_scale);

        if !append || !self.line_is_rtl {
            self.new_line(run.range.start, 0.0);
        }

        // In RTL mode, caret.0 starts at zero goes negative. We avoid adding
        // or subtracting width_bound until alignment since it may be infinite
        // during size requirement computations.

        let mut initial_caret = self.caret.0;
        self.caret.0 -= run.caret;
        let mut line_len = run.caret - run.start_no_space() - initial_caret;
        let glyph_end = run.glyphs.len() as u32;
        if !self.wrap || (!self.justify && line_len <= self.width_bound) {
            // Short-cut: we can add the entire run.
            self.prep_add(&scale_font, line_len, true, &run);
            self.add_part(run.range.end, run_index, 0..glyph_end);
        } else if run.level.is_ltr() {
            // It makes little sense to wrap a run against its direction.
            // Instead we push the run to the next line, and if it still needs
            // splitting switch the direction of that line.

            line_len = run.caret - run.start_no_space();
            if line_len < self.width_bound {
                self.new_line(run.range.start, 0.0);
                self.caret.0 -= run.caret;
                self.prep_add(&scale_font, line_len, true, &run);
                self.add_part(run.range.end, run_index, 0..glyph_end);
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
                    self.prep_add(&scale_font, line_len, true, &run);
                    let mut text_end = run.range.end;
                    if glyph_start > 0 {
                        text_end = run.glyphs[glyph_start as usize - 1].index;
                    }
                    self.add_part(text_end, run_index, glyph_start..glyph_end);
                }

                // If we are already at the end of the run, stop. This
                // should not happen on the first iteration.
                if glyph_start == 0 {
                    self.caret.0 = -line_len;
                    break;
                }

                if line_break {
                    // Offset new line since we are not at the start of the run
                    let g = run.glyphs[glyph_start as usize - 1];
                    let pos = g.position.0 + scale_font.h_advance(g.id);
                    self.new_line(g.index, -pos);
                    initial_caret = run.caret - pos;
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
    line_text_start: u32,
    line_text_end: u32,
    line_len: f32,
    line_runs: Vec<(usize, Level, f32)>,
    line_max_level: Option<Level>,
    ascent: f32,
    descent: f32,
    line_gap: f32,
    longest: f32,
    caret: Vec2,
    num_glyphs: u32,
    halign: Align,
    justify: bool,
    wrap: bool,
    line_is_rtl: bool,
    width_bound: f32,
}
impl LineAdder {
    fn new(run_capacity: usize, env: &Environment) -> Self {
        let runs = Vec::with_capacity(run_capacity);
        let justify = env.halign == Align::Stretch;
        LineAdder {
            runs,
            halign: env.halign,
            justify,
            wrap: env.wrap,
            width_bound: env.bounds.0,
            ..Default::default()
        }
    }

    /// Does the current line have any content?
    fn line_is_empty(&self) -> bool {
        self.runs[self.line_start..]
            .iter()
            .all(|run| run.glyph_range.len() == 0)
    }

    fn prep_add<F: Font, SF: ScaleFont<F>>(
        &mut self,
        scale_font: &SF,
        line_len: f32,
        rtl: bool,
        run: &GlyphRun,
    ) {
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

        let part_len = (if rtl { -1.0 } else { 1.0 }) * (line_len - self.line_len);
        self.line_runs.push((self.runs.len(), run.level, part_len));
        self.line_max_level = self
            .line_max_level
            .map(|level| level.max(run.level))
            .or(Some(run.level));

        self.line_len = line_len;
    }

    // Call prep_add first!
    #[inline]
    fn add_part(&mut self, text_end: u32, run_index: usize, glyph_range: std::ops::Range<u32>) {
        self.line_text_end = text_end;
        self.num_glyphs += glyph_range.len() as u32;
        self.runs.push(RunPart {
            text_end,
            glyph_run: run_index as u32,
            glyph_range: glyph_range.into(),
            offset: self.caret,
        });
    }

    fn finish_line(&mut self, text_index: u32) {
        let num_runs = self.runs.len() - self.line_start;

        // Unic TR#9 L2: reverse items on the line
        // This implementation does not correspond directly to the Unicode
        // algorithm, which assumes that shaping happens *after* re-arranging
        // chars (but also *before*, in order to calculate line-wrap points).
        // Our shaper(s) accept both LTR and RTL input; additionally, our line
        // wrapping must explicitly handle both LTR and RTL lines; the missing
        // step is to rearrange non-wrapped runs on the line.
        if let Some(mut level) = self.line_max_level {
            let reverse_first = |mut slice: &mut [(usize, Level, f32)]| loop {
                if slice.len() < 2 {
                    return;
                }
                let len1 = slice.len() - 1;
                let a = slice[0].0;
                slice[0].0 = slice[len1].0;
                slice[len1].0 = a;
                slice = &mut slice[1..len1];
            };
            let line_level = match self.line_is_rtl {
                false => Level::ltr(),
                true => Level::rtl(),
            };
            while level > line_level {
                let mut start = None;
                for i in 0..num_runs {
                    if let Some(s) = start {
                        if self.line_runs[i].1 < level {
                            reverse_first(&mut self.line_runs[s..i]);
                            start = None;
                        }
                    } else {
                        if self.line_runs[i].1 >= level {
                            start = Some(i);
                        }
                    }
                }
                if let Some(s) = start {
                    reverse_first(&mut self.line_runs[s..]);
                }
                level.lower(1).unwrap();
            }

            let mut i = 0;
            while i < self.line_runs.len() {
                let j = self.line_runs[i].0 - self.line_start;
                if j > i {
                    let mut a = self.line_runs[j].2;
                    let b = a - self.line_runs[i].2;
                    for k in (i + 1)..j {
                        a += self.line_runs[k].2;
                        self.runs[self.line_start + k].offset.0 += b;
                    }
                    self.runs[self.line_start + i].offset.0 += a;
                    self.runs[self.line_start + j].offset.0 -= a - b;
                    self.line_runs.swap(i, j);
                    continue;
                }
                i += 1;
            }

            let runs = &mut self.runs[self.line_start..];

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
                    let num_gaps = (num_runs - 1) as f32;
                    let per_gap = (self.width_bound - self.line_len) / num_gaps;

                    if !self.line_is_rtl {
                        let mut offset = per_gap;
                        for run in &mut runs[1..] {
                            run.offset.0 += offset;
                            offset += per_gap;
                        }
                    } else {
                        let mut offset = self.width_bound;
                        for run in &mut runs[..] {
                            run.offset.0 += offset;
                            offset -= per_gap;
                        }
                    }
                    0.0 // do not offset below
                }
            };
            if offset != 0.0 {
                for run in &mut runs[..] {
                    run.offset.0 += offset;
                }
            }
        }

        self.line_runs.clear();
        self.line_max_level = None;

        let top = self.caret.1 - self.ascent;
        self.caret.1 -= self.descent;
        self.longest = self.longest.max(self.line_len);
        self.line_len = 0.0;
        self.lines.push(Line {
            text_range: Range::from(self.line_text_start..self.line_text_end),
            run_range: (self.line_start..self.runs.len()).into(),
            top,
            bottom: self.caret.1,
        });
        self.line_text_start = text_index;
    }

    fn new_line(&mut self, text_index: u32, x: f32) {
        if self.line_start != self.runs.len() {
            self.finish_line(text_index);
        }
        self.line_start = self.runs.len();
        self.caret.0 = x;
        self.caret.1 += self.line_gap;

        self.ascent = 0.0;
        self.descent = 0.0;
        self.line_gap = 0.0;
    }

    // Returns: required dimensions
    fn finish(&mut self, text_index: u32, valign: Align, height_bound: f32) -> Vec2 {
        // If any (even empty) run was added to the line, add v-space
        if self.line_start != self.runs.len() {
            self.finish_line(text_index);
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
