// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::TextDisplay;
use crate::conv::{to_u32, to_usize};
use crate::fonts::FontId;
use crate::format::FormattableText;
use crate::{shaper, Action, Direction, Range};
use unicode_bidi::{BidiInfo, Level, LTR_LEVEL, RTL_LEVEL};
use xi_unicode::LineBreakIterator;

#[derive(Copy, Clone, Debug)]
pub(crate) struct LineRun {
    /// Range within runs
    ///
    /// Runs within this range occur in visual order, from the line's start
    /// (left or right depending on direction).
    pub range: Range,
    /// If true, line is right-to-left
    pub rtl: bool,
}

impl TextDisplay {
    /// Update font size
    ///
    /// This updates the result of [`TextDisplay::prepare_runs`] due to change
    /// in font size.
    ///
    /// Prerequisites: prepared runs: panics if action is greater than `Action::Wrap`.  
    /// Post-requirements: prepare lines (requires action `Action::Wrap`).  
    /// Parameters: see [`crate::Environment`] documentation.
    pub fn resize_runs<F: FormattableText>(&mut self, text: &F, dpp: f32, pt_size: f32) {
        assert!(
            self.action <= Action::Wrap,
            "kas-text::TextDisplay: runs not prepared"
        );
        self.action = Action::Wrap;
        let mut dpem = dpp * pt_size;

        let mut font_tokens = text.font_tokens(dpp, pt_size);
        let mut next_fmt = font_tokens.next();

        for run in &mut self.runs {
            while let Some(fmt) = next_fmt.as_ref() {
                if fmt.start > run.range.start {
                    break;
                }
                dpem = fmt.dpem;
                next_fmt = font_tokens.next();
            }

            // This is hacky, but should suffice!
            let mut soft_breaks = Default::default();
            std::mem::swap(&mut soft_breaks, &mut run.soft_breaks);
            *run = shaper::shape(
                text.as_str(),
                run.range,
                dpem,
                run.font_id,
                soft_breaks,
                run.no_break,
                run.level,
            );
        }
    }

    /// Prepare text runs
    ///
    /// This is the first step of preparation: breaking text into runs according
    /// to font properties, bidi-levels and line-wrap points.
    ///
    /// Prerequisites: none (ignores current `action` state).  
    /// Post-requirements: prepare lines (requires action `Action::Wrap`).  
    /// Parameters: see [`crate::Environment`] documentation.
    pub fn prepare_runs<F: FormattableText>(
        &mut self,
        text: &F,
        bidi: bool,
        dir: Direction,
        dpp: f32,
        pt_size: f32,
    ) {
        // This method constructs a list of "hard lines" (the initial line and any
        // caused by a hard break), each composed of a list of "level runs" (the
        // result of splitting and reversing according to Unicode TR9 aka
        // Bidirectional algorithm), plus a list of "soft break" positions
        // (where wrapping may introduce new lines depending on available space).

        self.runs.clear();
        self.line_runs.clear();
        self.action = Action::Wrap;

        let mut font_id = FontId::default();
        let mut dpem = dpp * pt_size;

        let mut font_tokens = text.font_tokens(dpp, pt_size);
        let mut next_fmt = font_tokens.next();
        if let Some(fmt) = next_fmt.as_ref() {
            if fmt.start == 0 {
                font_id = fmt.font_id;
                dpem = fmt.dpem;
                next_fmt = font_tokens.next();
            }
        }

        let bidi = bidi;
        let default_para_level = match dir {
            Direction::Auto => None,
            Direction::LR => Some(LTR_LEVEL),
            Direction::RL => Some(RTL_LEVEL),
        };
        let mut levels = vec![];
        let mut level: Level;
        if bidi || default_para_level.is_none() {
            levels = BidiInfo::new(text.as_str(), default_para_level).levels;
            assert_eq!(text.str_len(), levels.len());
            level = levels.get(0).cloned().unwrap_or(LTR_LEVEL);
        } else {
            level = default_para_level.unwrap();
        }

        let mut start = 0;
        let mut breaks = Default::default();

        // Iterates over `(pos, hard)` tuples:
        let mut breaks_iter = LineBreakIterator::new(text.as_str());
        let mut next_break = breaks_iter.next().unwrap_or((0, false));

        let mut line_start = 0;

        for pos in 1..text.str_len() {
            let is_break = next_break.0 == pos;
            let hard_break = is_break && next_break.1;
            let bidi_break = bidi && levels[pos] != level;

            let fmt_break = next_fmt
                .as_ref()
                .map(|fmt| to_usize(fmt.start) == pos)
                .unwrap_or(false);

            if hard_break || bidi_break || fmt_break {
                let mut range = trim_control(&text.as_str()[start..pos]);
                // trim_control gives us a range within the slice; we need to offset:
                range.start += to_u32(start);
                range.end += to_u32(start);

                self.runs.push(shaper::shape(
                    text.as_str(),
                    range,
                    dpem,
                    font_id,
                    breaks,
                    !is_break,
                    level,
                ));

                if let Some(fmt) = next_fmt.as_ref() {
                    if to_usize(fmt.start) == pos {
                        font_id = fmt.font_id;
                        dpem = fmt.dpem;
                        next_fmt = font_tokens.next();
                    }
                }

                if hard_break {
                    let range = Range::from(line_start..self.runs.len());
                    let rtl = self.runs[line_start].level.is_rtl();
                    self.line_runs.push(LineRun { range, rtl });
                    line_start = self.runs.len();
                }

                start = pos;
                if bidi {
                    level = levels[pos];
                }
                breaks = Default::default();
                if is_break {
                    next_break = breaks_iter.next().unwrap_or((0, false));
                }
            } else if is_break {
                breaks.push(to_u32(pos));
                next_break = breaks_iter.next().unwrap_or((0, false));
            }
        }

        // The loop above misses the last break at the end of the text.
        // LineBreakIterator always reports a hard break at the text's end
        // regardless of whether the text ends with a line-break char.
        let mut range = trim_control(&text.as_str()[start..]);
        // trim_control gives us a range within the slice; we need to offset:
        range.start += to_u32(start);
        range.end += to_u32(start);
        self.runs.push(shaper::shape(
            text.as_str(),
            range,
            dpem,
            font_id,
            breaks,
            false,
            level,
        ));
        if line_start < self.runs.len() {
            let range = Range::from(line_start..self.runs.len());
            let rtl = self.runs[line_start].level.is_rtl();
            self.line_runs.push(LineRun { range, rtl });
        }

        // If the text *does* end with a line-break, we add an empty line.
        // TODO: do we want the empty line for labels or only for edit boxes?
        if let Some(c) = text.as_str().chars().next_back() {
            // Match according to https://www.unicode.org/reports/tr14/#Table1
            if let '\x0C' | '\x0B' | '\u{2028}' | '\u{2029}' | '\r' | '\n' | '\u{85}' = c {
                let text_len = text.str_len();
                let range = Range::from(text_len..text_len);
                level = default_para_level.unwrap_or(LTR_LEVEL);
                breaks = Default::default();
                line_start = self.runs.len();
                self.runs.push(shaper::shape(
                    text.as_str(),
                    range,
                    dpem,
                    font_id,
                    breaks,
                    false,
                    level,
                ));

                let range = Range::from(line_start..self.runs.len());
                let rtl = level.is_rtl();
                self.line_runs.push(LineRun { range, rtl });
            }
        }

        /*
        println!("text: {}", text.as_str());
        for line in self.line_runs.iter() {
            println!("line (rtl={}) runs:", line.rtl);
            for run in &self.runs[line.range.to_std()] {
                let slice = &text.as_str()[run.range];
                println!(
                    "{:?}, text.as_str()[{}..{}]: '{}', breaks={:?}, no_break={}",
                    run.level, run.range.start, run.range.end, slice, run.breaks, run.no_break,
                );
            }
        }
        */
    }
}

fn trim_control(slice: &str) -> Range {
    let (mut a, mut b) = (0, 0);
    let mut char_indices = slice.char_indices();

    loop {
        let pre_iter_len = char_indices.as_str().len();
        if let Some((i, c)) = char_indices.next() {
            if char::is_control(c) {
                continue;
            } else {
                // First non-control char. It may also be the last and we have
                // now removed it from the iter, so we must record the location.
                let char_len = pre_iter_len - char_indices.as_str().len();
                a = i;
                b = i + char_len;
                break;
            }
        }
        break;
    }

    loop {
        let pre_iter_len = char_indices.as_str().len();
        if let Some((i, c)) = char_indices.next_back() {
            if char::is_control(c) {
                continue;
            } else {
                let char_len = pre_iter_len - char_indices.as_str().len();
                b = i + char_len;
            }
        }
        break;
    }

    (a..b).into()
}
