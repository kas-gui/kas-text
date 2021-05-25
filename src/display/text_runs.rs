// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::TextDisplay;
use crate::conv::{to_u32, to_usize};
use crate::fonts::{fonts, FontId};
use crate::format::FormattableText;
use crate::{shaper, Action, Direction, Range};
use unicode_bidi::{BidiInfo, Level, LTR_LEVEL, RTL_LEVEL};
use xi_unicode::LineBreakIterator;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RunSpecial {
    None,
    /// Forbid a break here
    NoBreak,
    /// Horizontal tab
    HTab,
}

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
            let mut breaks = Default::default();
            std::mem::swap(&mut breaks, &mut run.breaks);
            if run.level.is_rtl() {
                breaks.reverse();
            }
            *run = shaper::shape(
                text.as_str(),
                run.range,
                dpem,
                run.face_id,
                breaks,
                run.special,
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

        let fonts = fonts();
        // TODO: resolve face per character
        let mut face_id = fonts.first_face_for(font_id);

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
        let mut last_is_control = false;
        let mut last_is_htab = false;
        let mut non_control_end = 0;

        for (pos, c) in text.as_str().char_indices() {
            // Handling for control chars
            if !last_is_control {
                non_control_end = pos;
            }
            let is_control = c.is_control();
            let is_htab = c == '\t';
            let control_break = is_htab || (last_is_control && !is_control);

            // Is wrapping allowed at this position?
            let is_break = next_break.0 == pos;
            // Forcibly end the line?
            let hard_break = is_break && next_break.1;

            // Force end of current run?
            let bidi_break = bidi && levels[pos] != level;
            let fmt_break = next_fmt
                .as_ref()
                .map(|fmt| to_usize(fmt.start) == pos)
                .unwrap_or(false);

            if hard_break || control_break || bidi_break || fmt_break {
                let range = (start..non_control_end).into();
                let special = match (last_is_htab, last_is_control || is_break) {
                    (true, _) => RunSpecial::HTab,
                    (false, true) => RunSpecial::None,
                    (false, false) => RunSpecial::NoBreak,
                };
                self.runs.push(shaper::shape(
                    text.as_str(),
                    range,
                    dpem,
                    face_id,
                    breaks,
                    special,
                    level,
                ));

                if let Some(fmt) = next_fmt.as_ref() {
                    if to_usize(fmt.start) == pos {
                        font_id = fmt.font_id;
                        dpem = fmt.dpem;
                        next_fmt = font_tokens.next();
                    }
                }
                face_id = fonts.first_face_for(font_id);

                if hard_break {
                    let range = Range::from(line_start..self.runs.len());
                    let rtl = self.runs[line_start].level.is_rtl();
                    self.line_runs.push(LineRun { range, rtl });
                    line_start = self.runs.len();
                }

                start = pos;
                non_control_end = pos;
                if bidi {
                    level = levels[pos];
                }
                breaks = Default::default();
                if is_break {
                    next_break = breaks_iter.next().unwrap_or((0, false));
                }
            } else if is_break {
                if !is_control {
                    // We break runs when hitting control chars, but do so
                    // later; we do not want a "soft break" here.
                    breaks.push(shaper::GlyphBreak::new(to_u32(pos)));
                }
                next_break = breaks_iter.next().unwrap_or((0, false));
            }

            last_is_control = is_control;
            last_is_htab = is_htab;
        }

        // Conclude: add last run. This may be empty, but we want it anyway.
        if !last_is_control {
            non_control_end = text.str_len();
        }
        let range = (start..non_control_end).into();
        let special = match last_is_htab {
            true => RunSpecial::HTab,
            false => RunSpecial::None,
        };
        self.runs.push(shaper::shape(
            text.as_str(),
            range,
            dpem,
            face_id,
            breaks,
            special,
            level,
        ));

        debug_assert!(line_start < self.runs.len());
        let range = Range::from(line_start..self.runs.len());
        let rtl = self.runs[line_start].level.is_rtl();
        self.line_runs.push(LineRun { range, rtl });

        // The LineBreakIterator finishes with a break (unless the string is empty).
        // This is a hard break when the string finishes with an explicit line-break,
        // in which case we have an implied new line (empty).
        debug_assert_eq!(next_break.0, text.str_len());
        if next_break.1 {
            let text_len = text.str_len();
            let range = Range::from(text_len..text_len);
            level = default_para_level.unwrap_or(LTR_LEVEL);
            breaks = Default::default();
            line_start = self.runs.len();
            self.runs.push(shaper::shape(
                text.as_str(),
                range,
                dpem,
                face_id,
                breaks,
                RunSpecial::None,
                level,
            ));

            let range = Range::from(line_start..self.runs.len());
            let rtl = level.is_rtl();
            self.line_runs.push(LineRun { range, rtl });
        }

        /*
        println!("text: {}", text.as_str());
        for line in self.line_runs.iter() {
            println!("line (rtl={}) runs:", line.rtl);
            for run in &self.runs[line.range.to_std()] {
                let slice = &text.as_str()[run.range];
                println!(
                    "\t{:?}, text.as_str()[{}..{}]: '{}', breaks={:?}",
                    run.level, run.range.start, run.range.end, slice, run.breaks,
                );
            }
        }
        */
    }
}
