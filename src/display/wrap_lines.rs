// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: wrapping

use super::{NotReady, RunSpecial, TextDisplay};
use crate::conv::{to_u32, to_usize};
use crate::fonts::{fonts, FontLibrary};
use crate::shaper::GlyphRun;
use crate::{Action, Align, Range, Vec2};
use smallvec::SmallVec;
use unicode_bidi::{Level, LTR_LEVEL};

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
    /// Measure required width, up to some `max_width`
    ///
    /// This method allows calculation of the width requirement of a text object
    /// without full wrapping and glyph placement. Whenever the requirement
    /// exceeds `max_width`, the algorithm stops early, returning `max_width`.
    ///
    /// The return value is unaffected by alignment and wrap configuration.
    pub fn measure_width(&self, max_width: f32) -> Result<f32, NotReady> {
        if self.action > Action::Wrap {
            return Err(NotReady);
        }

        let mut max_line_len = 0.0f32;
        let mut caret = 0.0;
        let mut line_len = 0.0;

        for run in self.runs.iter() {
            let num_parts = run.num_parts();
            let (_, part_len_no_space, part_len) = run.part_lengths(0..num_parts);

            if part_len_no_space > 0.0 {
                line_len = caret + part_len_no_space;
                if line_len >= max_width {
                    return Ok(max_width);
                }
            }
            caret += part_len;

            if run.special == RunSpecial::HardBreak {
                max_line_len = max_line_len.max(line_len);
                caret = 0.0;
                line_len = 0.0;
            }
        }

        Ok(max_line_len.max(line_len))
    }

    /// Measure required vertical height, wrapping as configured
    ///
    /// This method performs most required preparation steps of the
    /// [`TextDisplay`]. Remaining prepartion should be fast.
    pub fn measure_height(
        &mut self,
        width_bound: f32,
        wrap: bool,
        h_align: Align,
    ) -> Result<f32, NotReady> {
        if self.action > Action::Wrap {
            return Err(NotReady);
        }

        if self.action == Action::Wrap {
            return self.prepare_lines(width_bound, wrap, h_align).map(|v| v.1);
        }

        self.bounding_box().map(|(tl, br)| br.1 - tl.1)
    }

    /// Prepare lines ("wrap")
    ///
    /// This does text layout, including wrapping and horizontal alignment but
    /// excluding vertical alignment.
    ///
    /// Returns:
    ///
    /// -   `Err(NotReady)` if required action is greater than [`Action::Wrap`]
    /// -   `Ok(bounding_corner)` on success: the vertical component is the
    ///     required height while the horizontal component depends on alignment
    pub fn prepare_lines(
        &mut self,
        width_bound: f32,
        wrap: bool,
        h_align: Align,
    ) -> Result<Vec2, NotReady> {
        if self.action > Action::Wrap {
            return Err(NotReady);
        }
        self.action = Action::VAlign;

        let fonts = fonts();
        let mut adder = LineAdder::new(width_bound, h_align);
        let width_bound = adder.width_bound;
        let justify = h_align == Align::Stretch;
        let mut parts = Vec::with_capacity(16);

        // Tuples: (index, part, num_parts)
        let mut start = (0, 0, 0);
        let mut end = start;

        let mut caret = 0.0;
        let mut index = start.0;

        let end_index = self.runs.len();
        'a: while index < end_index {
            let run = &self.runs[index];
            let num_parts = run.num_parts();

            let hard_break = run.special == RunSpecial::HardBreak;
            let allow_break = run.special != RunSpecial::NoBreak;
            let tab = run.special == RunSpecial::HTab;

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
                    adder.add_line(fonts, &self.runs, slice, true);

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

            if hard_break || index == end_index {
                if parts.len() > 0 {
                    // It should not be possible for a line to end with a no-break, so:
                    debug_assert_eq!(parts.len(), end.2);
                    adder.add_line(fonts, &self.runs, &mut parts, false);
                }

                start = (index, 0, 0);
                end = start;
                parts.clear();

                caret = 0.0;
                index = start.0;
            }
        }

        self.wrapped_runs = adder.runs;
        self.lines = adder.lines;
        #[cfg(feature = "num_glyphs")]
        {
            self.num_glyphs = adder.num_glyphs;
        }
        self.l_bound = adder.l_bound.min(adder.r_bound);
        self.r_bound = adder.r_bound;
        Ok(Vec2(adder.r_bound, adder.vcaret))
    }

    /// Vertically align lines
    ///
    /// Returns the bottom-right bounding corner.
    pub fn vertically_align(&mut self, bound: f32, v_align: Align) -> Result<Vec2, NotReady> {
        if self.action > Action::VAlign {
            return Err(NotReady);
        }
        self.action = Action::None;

        if self.lines.is_empty() {
            return Ok(Vec2(0.0, 0.0));
        }

        let top = self.lines.first().unwrap().top;
        let bottom = self.lines.last().unwrap().bottom;
        let height = bottom - top;
        let new_offset = match v_align {
            _ if height >= bound => 0.0,
            Align::Default | Align::TL | Align::Stretch => 0.0,
            Align::Center => 0.5 * (bound - height),
            Align::BR => bound - height,
        };
        let offset = new_offset - top;

        if offset != 0.0 {
            for run in &mut self.wrapped_runs {
                run.offset.1 += offset;
            }
            for line in &mut self.lines {
                line.top += offset;
                line.bottom += offset;
            }
        }

        Ok(Vec2(self.r_bound, bottom))
    }
}

#[derive(Default)]
struct LineAdder {
    runs: SmallVec<[RunPart; 1]>,
    lines: SmallVec<[Line; 1]>,
    line_gap: f32,
    l_bound: f32,
    r_bound: f32,
    vcaret: f32,
    #[cfg(feature = "num_glyphs")]
    num_glyphs: u32,
    h_align: Align,
    width_bound: f32,
}
impl LineAdder {
    fn new(width_bound: f32, h_align: Align) -> Self {
        LineAdder {
            l_bound: width_bound,
            h_align,
            width_bound,
            ..Default::default()
        }
    }

    fn add_line(
        &mut self,
        fonts: &FontLibrary,
        runs: &[GlyphRun],
        parts: &mut [PartInfo],
        is_wrap: bool,
    ) {
        assert!(parts.len() > 0);
        let line_start = self.runs.len();

        // Iterate runs to determine max ascent, level, etc.
        let mut last_run = u32::MAX;
        let (mut ascent, mut descent, mut line_gap) = (0f32, 0f32, 0f32);
        let mut line_level = Level::new(Level::max_implicit_depth()).unwrap();
        let mut max_level = LTR_LEVEL;
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

            line_level = line_level.min(run.level);
            max_level = max_level.max(run.level);
        }
        let line_is_rtl = line_level.is_rtl();

        if !self.lines.is_empty() {
            self.vcaret += line_gap.max(self.line_gap);
        }
        self.vcaret += ascent;
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
                    } else if part_level >= level {
                        start = Some(i);
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
        let mut caret = match self.h_align {
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

        self.l_bound = self.l_bound.min(caret);
        let mut end_caret = caret;

        for (i, part) in parts.iter().enumerate() {
            let run = &runs[to_usize(part.run)];

            #[cfg(feature = "num_glyphs")]
            {
                self.num_glyphs += to_u32(part.glyph_range.len());
            }

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
                glyph_range: part.glyph_range,
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

        self.r_bound = self.r_bound.max(caret);

        // Other parts of this library expect runs to be in logical order, so
        // we re-order now (does not affect display position).
        // TODO: should we change this, e.g. for visual-order navigation?
        self.runs[line_start..].sort_by_key(|run| run.text_end);

        let top = self.vcaret - ascent;
        self.vcaret -= descent;
        // Vertically align lines to the nearest pixel (improves rendering):
        self.vcaret = self.vcaret.round();

        self.lines.push(Line {
            text_range: Range::from(line_text_start..line_text_end),
            run_range: (line_start..self.runs.len()).into(),
            top,
            bottom: self.vcaret,
        });
    }
}
