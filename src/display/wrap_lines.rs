// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: wrapping

use super::{RunSpecial, TextDisplay};
use crate::conv::{to_u32, to_usize};
use crate::fonts::{self, FontLibrary};
use crate::shaper::{GlyphRun, PartMetrics};
use crate::{Align, Range, Vec2};
use smallvec::SmallVec;
use std::num::NonZeroUsize;
use tinyvec::TinyVec;
use unicode_bidi::{LTR_LEVEL, Level};

#[derive(Clone, Debug, Default)]
pub struct RunPart {
    pub text_end: u32,
    pub glyph_run: u32,
    pub glyph_range: Range,
    pub offset: Vec2,
}

/// Per-line data (post wrapping)
#[derive(Clone, Debug, Default)]
pub struct Line {
    pub(crate) text_range: Range, // range in text
    pub(crate) run_range: Range,  // range in wrapped_runs
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl Line {
    /// Get the text range of line contents
    #[inline]
    pub fn text_range(&self) -> std::ops::Range<usize> {
        self.text_range.to_std()
    }

    /// Get the upper bound of the line
    #[inline]
    pub fn top(&self) -> f32 {
        self.top
    }

    /// Get the lower bound of the line
    #[inline]
    pub fn bottom(&self) -> f32 {
        self.bottom
    }
}

impl TextDisplay {
    /// Measure required width, up to some `max_width`
    ///
    /// [Requires status][Self#status-of-preparation]: level runs have been
    /// prepared.
    ///
    /// This method allows calculation of the width requirement of a text object
    /// without full wrapping and glyph placement. Whenever the requirement
    /// exceeds `max_width`, the algorithm stops early, returning `max_width`.
    ///
    /// The return value is unaffected by alignment and wrap configuration.
    pub fn measure_width(&self, max_width: f32) -> f32 {
        let mut max_line_len = 0.0f32;
        let mut caret = 0.0;
        let mut line_len = 0.0;

        for run in self.runs.iter() {
            let num_parts = run.num_parts();
            let part = run.part_lengths(0..num_parts);

            if part.len_no_space > 0.0 {
                line_len = caret + part.len_no_space;
                if line_len >= max_width {
                    return max_width;
                }
            }
            caret += part.len;

            if run.special == RunSpecial::HardBreak {
                max_line_len = max_line_len.max(line_len);
                caret = 0.0;
                line_len = 0.0;
            }
        }

        max_line_len.max(line_len)
    }

    /// Measure required vertical height, wrapping as configured
    ///
    /// Stops after `max_lines`, if provided.
    ///
    /// [Requires status][Self#status-of-preparation]: level runs have been
    /// prepared.
    pub fn measure_height(&self, wrap_width: f32, max_lines: Option<NonZeroUsize>) -> f32 {
        #[derive(Default)]
        struct MeasureAdder {
            parts: Vec<usize>, // run index for each part
            lines: usize,
            line_gap: f32,
            vcaret: f32,
        }

        impl PartAccumulator for MeasureAdder {
            fn num_parts(&self) -> usize {
                self.parts.len()
            }

            fn num_lines(&self) -> usize {
                self.lines
            }

            fn add_part(
                &mut self,
                _: &[GlyphRun],
                run_index: usize,
                _: std::ops::Range<usize>,
                _: PartMetrics,
                checkpoint: bool,
            ) {
                if checkpoint
                    && let Some(index) = self.parts.last().cloned()
                    && index == run_index
                {
                    return;
                }

                self.parts.push(run_index);
            }

            fn add_line(
                &mut self,
                fonts: &FontLibrary,
                runs: &[GlyphRun],
                parts_end: usize,
                _: bool,
            ) {
                debug_assert!(parts_end > 0);
                let parts = &mut self.parts[..parts_end];

                // Iterate runs to determine max ascent, level, etc.
                let mut last_run = usize::MAX;
                let (mut ascent, mut descent, mut line_gap) = (0f32, 0f32, 0f32);
                for run_index in parts.iter().cloned() {
                    if last_run == run_index {
                        continue;
                    }
                    last_run = run_index;
                    let run = &runs[last_run];

                    let scale_font = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                    ascent = ascent.max(scale_font.ascent());
                    descent = descent.min(scale_font.descent());
                    line_gap = line_gap.max(scale_font.line_gap());
                }

                if self.lines > 0 {
                    self.vcaret += line_gap.max(self.line_gap);
                }
                self.vcaret += ascent - descent;
                self.line_gap = line_gap;

                // Vertically align lines to the nearest pixel (improves rendering):
                self.vcaret = self.vcaret.round();

                self.lines += 1;
                self.parts.clear();
            }
        }

        let mut adder = MeasureAdder::default();
        let max_lines = max_lines.map(|n| n.get()).unwrap_or(0);
        self.wrap_lines(&mut adder, wrap_width, max_lines);
        adder.vcaret
    }

    /// Prepare lines ("wrap")
    ///
    /// [Requires status][Self#status-of-preparation]: level runs have been
    /// prepared.
    ///
    /// This does text layout, including wrapping and horizontal alignment but
    /// excluding vertical alignment.
    ///
    /// `wrap_width` causes line wrapping at the given width. Use
    /// `f32::INFINITY` to disable wrapping.
    ///
    /// Text will be aligned within `0..width_bound` (see below); as such
    /// `width_bound` must be finite. When `wrap_width` is finite it will often
    /// make sense to use the same value; when not it might make sense to use
    /// `width_bound = 0` then offset the result using [`Self::apply_offset`].
    /// If `width_bound` is too small, text will escape `0..width_bound`
    /// according to alignment.
    ///
    /// Alignment is applied according to `h_align`:
    ///
    /// -   [`Align::Default`]: LTR text is left-aligned, RTL text is right-aligned
    /// -   [`Align::TL`]: text is left-aligned (left at `0`)
    /// -   [`Align::BR`]: text is right-aligned (right at `width_bound`)
    /// -   [`Align::Center`]: text is center-aligned (center at `width_bound / 2`)
    /// -   [`Align::Stretch`]: this is the most complex mode. For lines which
    ///     wrap and where the line length does not exceed `width_bound`, the
    ///     text is aligned to `0` *and* `width_bound` (if possible) by
    ///     stretching spaces within the text. Other lines are aligned in the
    ///     same way as [`Align::Default`].
    ///
    /// Returns the required height.
    pub fn prepare_lines(&mut self, wrap_width: f32, width_bound: f32, h_align: Align) -> f32 {
        debug_assert!(width_bound.is_finite());
        let mut adder = LineAdder::new(width_bound, h_align);

        self.wrap_lines(&mut adder, wrap_width, 0);

        self.wrapped_runs = adder.wrapped_runs;
        self.lines = adder.lines;
        self.l_bound = adder.l_bound.min(adder.r_bound);
        self.r_bound = adder.r_bound;
        adder.vcaret
    }

    fn wrap_lines(
        &self,
        accumulator: &mut impl PartAccumulator,
        wrap_width: f32,
        max_lines: usize,
    ) {
        let fonts = fonts::library();

        // Tuples: (index, part_index, num_parts)
        let mut start = (0, 0, 0);
        let mut end = start;

        let mut caret = 0.0;
        let mut run_index = start.0;

        let end_index = self.runs.len();
        'a: while run_index < end_index {
            let run = &self.runs[run_index];
            let num_parts = run.num_parts();

            let hard_break = run.special == RunSpecial::HardBreak;
            let allow_break = run.special != RunSpecial::NoBreak;
            let tab = run.special == RunSpecial::HTab;

            let mut last_part = start.1;
            let mut part_index = last_part + 1;
            while part_index <= num_parts {
                let mut part = run.part_lengths(last_part..part_index);
                if tab {
                    // Tab runs have no glyph; instead we calculate part.len
                    // based on the current line length.

                    // TODO(bidi): we should really calculate this after
                    // re-ordering the line based on full line contents,
                    // then use a checkpoint reset if too long.

                    let sf = fonts.get_face(run.face_id).scale_by_dpu(run.dpu);
                    // TODO: custom tab sizes?
                    let tab_size = sf.h_advance(sf.face().glyph_index(' ')) * 8.0;
                    let stops = (caret / tab_size).floor() + 1.0;
                    part.len = tab_size * stops - caret;
                }

                let line_len = caret + part.len_no_space;
                if line_len > wrap_width && end.2 > 0 {
                    // Add up to last valid break point then wrap and reset
                    accumulator.add_line(fonts, &self.runs, end.2, true);

                    if accumulator.num_lines() == max_lines {
                        return;
                    }

                    end.2 = 0;
                    start = end;
                    caret = 0.0;
                    run_index = start.0;
                    continue 'a;
                }
                caret += part.len;
                let checkpoint = part_index < num_parts || allow_break;
                accumulator.add_part(
                    &self.runs,
                    run_index,
                    last_part..part_index,
                    part,
                    checkpoint,
                );
                if checkpoint {
                    end = (run_index, part_index, accumulator.num_parts());
                }
                last_part = part_index;
                part_index += 1;
            }

            run_index += 1;
            start.1 = 0;

            if hard_break || run_index == end_index {
                let num_parts = accumulator.num_parts();
                if num_parts > 0 {
                    // It should not be possible for a line to end with a no-break, so:
                    debug_assert_eq!(num_parts, end.2);

                    accumulator.add_line(fonts, &self.runs, num_parts, false);

                    if accumulator.num_lines() == max_lines {
                        return;
                    }
                }

                start = (run_index, 0, 0);
                end = start;

                caret = 0.0;
                run_index = start.0;
            }
        }
    }

    /// Add `offset` to all positioned content
    ///
    /// This is a low-level method which can be used for alignment in some
    /// cases. Cost is `O(w)` where `w` is the number of wrapped runs.
    pub fn apply_offset(&mut self, offset: Vec2) {
        for run in &mut self.wrapped_runs {
            run.offset += offset;
        }
        for line in &mut self.lines {
            line.top += offset.1;
            line.bottom += offset.1;
        }
        self.l_bound += offset.0;
        self.r_bound += offset.0;
    }

    /// Vertically align lines
    ///
    /// [Requires status][Self#status-of-preparation]: lines have been wrapped.
    pub fn vertically_align(&mut self, bound: f32, v_align: Align) {
        debug_assert!(bound.is_finite());

        if self.lines.is_empty() {
            return;
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
            self.apply_offset(Vec2(0.0, offset));
        }
    }
}

trait PartAccumulator {
    fn num_parts(&self) -> usize;
    fn num_lines(&self) -> usize;

    fn add_part(
        &mut self,
        runs: &[GlyphRun],
        run_index: usize,
        part_range: std::ops::Range<usize>,
        part: PartMetrics,
        checkpoint: bool,
    );

    fn add_line(&mut self, fonts: &FontLibrary, runs: &[GlyphRun], parts_end: usize, is_wrap: bool);
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

#[derive(Default)]
struct LineAdder {
    wrapped_runs: TinyVec<[RunPart; 1]>,
    parts: Vec<PartInfo>,
    lines: TinyVec<[Line; 1]>,
    line_gap: f32,
    l_bound: f32,
    r_bound: f32,
    vcaret: f32,
    h_align: Align,
    width_bound: f32,
}
impl LineAdder {
    fn new(width_bound: f32, h_align: Align) -> Self {
        LineAdder {
            parts: Vec::with_capacity(16),
            l_bound: width_bound,
            h_align,
            width_bound,
            ..Default::default()
        }
    }
}

impl PartAccumulator for LineAdder {
    fn num_parts(&self) -> usize {
        self.parts.len()
    }

    fn num_lines(&self) -> usize {
        self.lines.len()
    }

    fn add_part(
        &mut self,
        runs: &[GlyphRun],
        run_index: usize,
        part_range: std::ops::Range<usize>,
        part: PartMetrics,
        checkpoint: bool,
    ) {
        let glyph_run = &runs[run_index];
        let run = to_u32(run_index);
        let glyph_range = glyph_run.to_glyph_range(part_range);

        let justify = self.h_align == Align::Stretch && part.len_no_space > 0.0;
        if checkpoint
            && !justify
            && let Some(info) = self.parts.last_mut()
            && info.run == run
        {
            // Merge into last part info (not strictly necessary)

            if glyph_run.level.is_rtl() {
                info.offset = part.offset;
            }
            info.glyph_range.end = glyph_range.end;
            debug_assert!(info.glyph_range.start <= info.glyph_range.end);

            if part.len_no_space > 0.0 {
                info.len_no_space = info.len + part.len_no_space;
            }
            info.len += part.len;

            return;
        }

        self.parts.push(PartInfo {
            run,
            offset: part.offset,
            len: part.len,
            len_no_space: part.len_no_space,
            glyph_range,
            end_space: false, // set later
        });
    }

    fn add_line(
        &mut self,
        fonts: &FontLibrary,
        runs: &[GlyphRun],
        parts_end: usize,
        is_wrap: bool,
    ) {
        debug_assert!(parts_end > 0);
        let line_start = self.wrapped_runs.len();
        let parts = &mut self.parts[..parts_end];

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
            if part.glyph_range.start() < run.glyphs.len() {
                run.glyphs[part.glyph_range.start()].index
            } else {
                run.range.start
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
                    part.glyph_range.end -= 1;
                }
            }

            line_text_end = run.range.end;
            if part.glyph_range.end() < run.glyphs.len() {
                line_text_end = run.glyphs[part.glyph_range.end()].index;
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
            Align::Default if line_is_rtl => spare,
            Align::Default => 0.0,
            Align::TL => 0.0,
            Align::Center => 0.5 * spare,
            Align::BR => spare,
            Align::Stretch if is_wrap && spare > 0.0 => {
                let len = parts.len();
                is_gap.resize(len, false);
                let mut num_gaps = 0;
                for (i, part) in parts[..len - 1].iter().enumerate() {
                    let run = &runs[to_usize(part.run)];
                    let not_at_end = part.glyph_range.end() < run.glyphs.len();
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

                if num_gaps == 0 {
                    match line_is_rtl {
                        false => 0.0,
                        true => spare,
                    }
                } else {
                    per_gap = spare / (num_gaps as f32);
                    0.0
                }
            }
            Align::Stretch if line_is_rtl => spare,
            Align::Stretch => 0.0,
        };

        self.l_bound = self.l_bound.min(caret);
        let mut end_caret = caret;

        for (i, part) in parts.iter().enumerate() {
            let run = &runs[to_usize(part.run)];

            let mut text_end = run.range.end;
            if part.glyph_range.end() < run.glyphs.len() {
                text_end = run.glyphs[part.glyph_range.end()].index;
            }
            debug_assert!(text_end <= line_text_end);

            let mut offset = 0.0;
            if run.level.is_rtl() {
                offset = part.len_no_space - part.len;
            }

            let xoffset = if part.end_space {
                end_caret - part.offset + offset
            } else {
                caret - part.offset
            };

            self.wrapped_runs.push(RunPart {
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
        self.wrapped_runs[line_start..].sort_by_key(|run| run.text_end);

        let top = self.vcaret - ascent;
        self.vcaret -= descent;
        // Vertically align lines to the nearest pixel (improves rendering):
        self.vcaret = self.vcaret.round();

        self.lines.push(Line {
            text_range: Range::from(line_text_start..line_text_end),
            run_range: (line_start..self.wrapped_runs.len()).into(),
            top,
            bottom: self.vcaret,
        });
        self.parts.clear();
    }
}
