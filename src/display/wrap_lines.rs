// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: wrapping

use super::{RunSpecial, TextDisplay};
use crate::conv::{to_u32, to_usize};
use crate::fonts::{fonts, FontLibrary};
use crate::shaper::GlyphRun;
use crate::{Action, Align, EnvFlags, Range, Vec2};
use smallvec::SmallVec;
use unicode_bidi::Level;

#[derive(Clone, Debug)]
pub struct RunPart {
    pub text_end: u32,
    pub glyph_run: u32,
    pub glyph_range: Range,
    pub offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub text_range: Range, // range in text
    pub run_range: Range,  // range in wrapped_runs
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
struct PartInfo {
    run: u32,
    offset: f32,
    len: f32,
    len_no_space: f32,
    glyph_range: Range,
    end_space: bool,
}

impl TextDisplay {
    /// Prepare lines ("wrap")
    ///
    /// This does text layout, with wrapping if enabled.
    ///
    /// Prerequisites: prepared runs: panics if action is greater than `Action::Wrap`.  
    /// Post-requirements: none (`Action::None`).  
    /// Parameters: see [`crate::Environment`] documentation.  
    /// Returns: required size, in pixels.
    pub fn prepare_lines(&mut self, bounds: Vec2, flags: EnvFlags, align: (Align, Align)) -> Vec2 {
        assert!(
            self.action <= Action::Wrap,
            "kas-text::TextDisplay: runs not prepared"
        );
        self.action = Action::None;

        let wrap = flags.contains(EnvFlags::WRAP);
        let px_valign = flags.contains(EnvFlags::PX_VALIGN);

        let fonts = fonts();
        let capacity = 0; // TODO(opt): estimate like self.text_len() / 16 ?
        let mut adder = LineAdder::new(capacity, bounds, align);
        let width_bound = adder.width_bound;
        let justify = align.0 == Align::Stretch;
        let mut parts = Vec::with_capacity(16);

        // Almost everything in "this" method depends on the line direction, so
        // we determine that then call the appropriate implementation.
        for line in self.line_runs.iter() {
            // Each LineRun contains at least one Run, though a Run may be empty
            let end_index = line.range.end();
            debug_assert!(line.range.start < line.range.end);
            assert!(end_index <= self.runs.len());
            let level = match line.rtl {
                false => Level::ltr(),
                true => Level::rtl(),
            };

            // Tuples: (index, part)
            let mut start = (line.range.start(), 0, 0);
            let mut end = start;
            parts.clear();

            let mut caret = 0.0;
            let mut index = start.0;
            'a: while index < end_index {
                let run = &self.runs[index];
                let num_parts = run.num_parts();
                let (allow_break, tab) = match run.special {
                    RunSpecial::None => (true, false),
                    RunSpecial::NoBreak => (false, false),
                    RunSpecial::HTab => (true, true),
                };

                let mut last_part = start.1;
                let mut part = last_part + 1;
                while part <= num_parts {
                    let (part_offset, part_len_no_space, mut part_len) =
                        run.part_lengths(last_part..part);
                    if tab {
                        // Tab runs have no glyph; instead we calculate part_len
                        // based on the current line length.

                        // TODO(bidi): we should really calculate this after
                        // re-ordering the line based on full line contents,
                        // then use a checkpoint reset if too long.

                        let sf = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                        // TODO: custom tab sizes?
                        let tab_size = sf.h_advance(sf.face().glyph_index(' ')) * 8.0;
                        let stops = (caret / tab_size).floor() + 1.0;
                        part_len = tab_size * stops - caret;
                    }

                    let line_len = caret + part_len_no_space;
                    if wrap && line_len > width_bound && end.2 > 0 {
                        // Add up to last valid break point then wrap and reset
                        let slice = &mut parts[0..end.2];
                        adder.add_line(fonts, level, &self.runs, slice, true, px_valign);

                        end.2 = 0;
                        start = end;
                        parts.clear();
                        caret = 0.0;
                        index = start.0;
                        continue 'a;
                    }
                    caret += part_len;
                    let glyph_range = run.to_glyph_range(last_part..part);
                    let checkpoint = part < num_parts || allow_break;
                    if !checkpoint
                        || (justify && part_len_no_space > 0.0)
                        || parts
                            .last()
                            .map(|part| to_usize(part.run) < index)
                            .unwrap_or(true)
                    {
                        parts.push(PartInfo {
                            run: to_u32(index),
                            offset: part_offset,
                            len: part_len,
                            len_no_space: part_len_no_space,
                            glyph_range,
                            end_space: false, // set later
                        });
                    } else {
                        // Combine with last part (not strictly necessary)
                        if let Some(part) = parts.last_mut() {
                            if run.level.is_ltr() {
                                part.glyph_range.end = glyph_range.end;
                            } else {
                                part.offset = part_offset;
                                part.glyph_range.start = glyph_range.start;
                            }
                            debug_assert!(part.glyph_range.start <= part.glyph_range.end);
                            if part_len_no_space > 0.0 {
                                part.len_no_space = part.len + part_len_no_space;
                            }
                            part.len += part_len;
                        }
                    }
                    if checkpoint {
                        end = (index, part, parts.len());
                    }
                    last_part = part;
                    part += 1;
                }

                index += 1;
                start.1 = 0;
            }

            if parts.len() > 0 {
                // It should not be possible for a line to end with a no-break, so:
                debug_assert_eq!(parts.len(), end.2);
                adder.add_line(fonts, level, &self.runs, &mut parts, false, px_valign);
            }
        }

        let required = adder.finish(bounds, align);
        self.wrapped_runs = adder.runs;
        self.lines = adder.lines;
        self.num_glyphs = adder.num_glyphs;
        self.width = bounds.0;
        required
    }
}

#[derive(Default)]
struct LineAdder {
    runs: Vec<RunPart>,
    lines: Vec<Line>,
    line_gap: f32,
    longest: f32,
    vcaret: f32,
    num_glyphs: u32,
    halign: Align,
    width_bound: f32,
}
impl LineAdder {
    fn new(run_capacity: usize, bounds: Vec2, align: (Align, Align)) -> Self {
        let runs = Vec::with_capacity(run_capacity);
        LineAdder {
            runs,
            halign: align.0,
            width_bound: bounds.0,
            ..Default::default()
        }
    }

    fn add_line(
        &mut self,
        fonts: &FontLibrary,
        line_level: Level,
        runs: &[GlyphRun],
        parts: &mut [PartInfo],
        is_wrap: bool,
        px_valign: bool,
    ) {
        assert!(parts.len() > 0);
        let line_start = self.runs.len();
        let line_is_rtl = line_level.is_rtl();

        // Iterate runs to determine max ascent, level, etc.
        let mut last_run = u32::MAX;
        let (mut ascent, mut descent, mut line_gap) = (0f32, 0f32, 0f32);
        let mut max_level = line_level;
        for part in parts.iter() {
            if last_run == part.run {
                continue;
            }
            last_run = part.run;
            let run = &runs[to_usize(last_run)];

            let scale_font = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
            ascent = ascent.max(scale_font.ascent());
            descent = descent.min(scale_font.descent());
            line_gap = line_gap.max(scale_font.line_gap());

            debug_assert!(run.level >= line_level);
            max_level = max_level.max(run.level);
        }

        self.vcaret += line_gap.max(self.line_gap) + ascent;
        self.line_gap = line_gap;

        let line_text_start = {
            let part = &parts[0];
            let run = &runs[to_usize(part.run)];
            if run.level.is_ltr() {
                if part.glyph_range.start() < run.glyphs.len() {
                    run.glyphs[part.glyph_range.start()].index
                } else {
                    run.range.start
                }
            } else {
                let end = part.glyph_range.end();
                if 0 < end && end < run.glyphs.len() {
                    run.glyphs[end - 1].index
                } else {
                    run.range.start
                }
            }
        };

        // Adjust the (logical) tail: optionally exclude last glyph, calculate
        // line_text_end, trim whitespace for the purposes of layout.
        let mut line_text_end;
        let mut line_len = 0.0;
        {
            let part = &mut parts[parts.len() - 1];
            let run = &runs[to_usize(part.run)];

            if is_wrap && part.len > part.len_no_space {
                // When wrapping on whitespace: exclude the last glyph
                // This excludes only one glyph; the main aim is to make the
                // 'End' key exclude the wrapping position (which is also the
                // next line's start). It also avoids highlighting whitespace
                // when selecting wrapped bidi text, for a single space.
                if part.glyph_range.start < part.glyph_range.end {
                    if run.level.is_ltr() {
                        part.glyph_range.end -= 1;
                    } else {
                        part.glyph_range.start += 1;
                    }
                }
            }

            line_text_end = run.range.end;
            if run.level.is_ltr() {
                if part.glyph_range.end() < run.glyphs.len() {
                    line_text_end = run.glyphs[part.glyph_range.end()].index;
                }
            } else {
                let start = part.glyph_range.start();
                if 0 < start && start < run.glyphs.len() {
                    line_text_end = run.glyphs[start - 1].index
                }
            }

            // With bidi text, the logical end may not actually be at the end;
            // we must not allow spaces here to move other content.
            for part in parts.iter_mut().rev() {
                part.end_space = true;

                if part.len_no_space > 0.0 {
                    break;
                }
            }
            for part in parts.iter() {
                line_len += if part.end_space {
                    part.len_no_space
                } else {
                    part.len
                };
            }
        }

        // Unic TR#9 L2: reverse items on the line
        // This implementation does not correspond directly to the Unicode
        // algorithm, which assumes that shaping happens *after* re-arranging
        // chars (but also *before*, in order to calculate line-wrap points).
        // Our shaper(s) accept both LTR and RTL input; additionally, our line
        // wrapping must explicitly handle both LTR and RTL lines; the missing
        // step is to rearrange non-wrapped runs on the line.

        {
            let mut level = max_level;
            while level > Level::ltr() {
                let mut start = None;
                for i in 0..parts.len() {
                    let part_level = runs[to_usize(parts[i].run)].level;
                    if let Some(s) = start {
                        if part_level < level {
                            parts[s..i].reverse();
                            start = None;
                        }
                    } else {
                        if part_level >= level {
                            start = Some(i);
                        }
                    }
                }
                if let Some(s) = start {
                    parts[s..].reverse();
                }
                level.lower(1).unwrap();
            }
        }

        let spare = self.width_bound - line_len;
        let mut is_gap = SmallVec::<[bool; 16]>::new();
        let mut per_gap = 0.0;
        let mut caret = match self.halign {
            Align::Default => match line_is_rtl {
                false => 0.0,
                true => spare,
            },
            Align::TL => 0.0,
            Align::Center => 0.5 * spare,
            Align::BR => spare,
            Align::Stretch if !is_wrap => match line_is_rtl {
                false => 0.0,
                true => spare,
            },
            Align::Stretch => {
                let len = parts.len();
                is_gap.resize(len, false);
                let mut num_gaps = 0;
                for (i, part) in parts[..len - 1].iter().enumerate() {
                    let run = &runs[to_usize(part.run)];
                    let not_at_end = if run.level.is_ltr() {
                        part.glyph_range.end() < run.glyphs.len()
                    } else {
                        part.glyph_range.start > 0
                    };
                    if not_at_end || run.special != RunSpecial::NoBreak {
                        is_gap[i] = true;
                        num_gaps += 1;
                    }
                }

                // Apply per-level reversing to is_gap
                let mut start = 0;
                let mut level = runs[to_usize(parts[0].run)].level;
                for i in 1..len {
                    let new_level = runs[to_usize(parts[i].run)].level;
                    if level != new_level {
                        if level > line_level {
                            is_gap[start..i].reverse();
                        }
                        start = i;
                        level = new_level;
                    }
                }
                if level > line_level {
                    is_gap[start..len - 1].reverse();
                }

                // Shift left 1 part for RTL lines
                if line_level.is_rtl() {
                    is_gap.copy_within(1..len, 0);
                }

                per_gap = spare / (num_gaps as f32);
                0.0
            }
        };

        let mut end_caret = caret;
        for (i, part) in parts.iter().enumerate() {
            let run = &runs[to_usize(part.run)];

            self.num_glyphs += to_u32(part.glyph_range.len());

            let mut text_end = run.range.end;
            let mut offset = 0.0;
            if run.level.is_ltr() {
                if part.glyph_range.end() < run.glyphs.len() {
                    text_end = run.glyphs[part.glyph_range.end()].index;
                }
            } else {
                let start = part.glyph_range.start();
                if 0 < start && start < run.glyphs.len() {
                    text_end = run.glyphs[start - 1].index
                }
                offset = part.len_no_space - part.len;
            }

            let xoffset = if part.end_space {
                end_caret - part.offset + offset
            } else {
                caret - part.offset
            };
            self.runs.push(RunPart {
                text_end,
                glyph_run: part.run,
                glyph_range: part.glyph_range.into(),
                offset: Vec2(xoffset, self.vcaret),
            });
            if part.end_space {
                caret += part.len_no_space;
                end_caret += part.len;
            } else {
                caret += part.len;
                end_caret = caret;
            }

            if is_gap.len() > 0 && is_gap[i] {
                caret += per_gap;
                end_caret += per_gap;
            }
        }

        // Other parts of this library expect runs to be in logical order, so
        // we re-order now (does not affect display position).
        // TODO: should we change this, e.g. for visual-order navigation?
        self.runs[line_start..].sort_by_key(|run| run.text_end);

        let top = self.vcaret - ascent;
        self.vcaret -= descent;
        if px_valign {
            self.vcaret = self.vcaret.round();
        }
        self.longest = self.longest.max(line_len);
        self.lines.push(Line {
            text_range: Range::from(line_text_start..line_text_end),
            run_range: (line_start..self.runs.len()).into(),
            top,
            bottom: self.vcaret,
        });
    }

    // Returns: required dimensions
    fn finish(&mut self, bounds: Vec2, align: (Align, Align)) -> Vec2 {
        let height = self.vcaret;
        let offset = match align.1 {
            Align::Default | Align::TL | Align::Stretch => 0.0, // nothing to do
            Align::Center => 0.5 * (bounds.1 - height),
            Align::BR => bounds.1 - height,
        };
        if offset != 0.0 {
            for run in &mut self.runs {
                run.offset.1 += offset;
            }
        }

        Vec2(self.longest, height)
    }
}
