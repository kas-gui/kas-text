// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::Text;
use crate::{Direction, Range};
use smallvec::SmallVec;
use unicode_bidi::{BidiInfo, Level, LTR_LEVEL, RTL_LEVEL};
use xi_unicode::LineBreakIterator;

#[derive(Clone, Debug)]
pub(crate) struct Run {
    /// Range in source text
    pub range: Range,
    /// BIDI level
    pub level: Level,
    /// All soft-break locations within this range (excludes end)
    pub breaks: SmallVec<[u32; 5]>,
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

impl Text {
    /// Bi-directional text and line-break processing
    ///
    /// Prerequisite: self.text is assigned, formatting is assigned
    ///
    /// Result: self.runs and self.breaks are assigned
    ///
    /// This method constructs a list of "hard lines" (the initial line and any
    /// caused by a hard break), each composed of a list of "level runs" (the
    /// result of splitting and reversing according to Unicode TR9 aka
    /// Bidirectional algorithm), plus a list of "soft break" positions
    /// (where wrapping may introduce new lines depending on available space).
    ///
    /// TODO: implement BIDI processing
    pub(crate) fn prepare_runs(&mut self) {
        self.runs.clear();
        self.line_runs.clear();
        if self.text.is_empty() {
            return;
        }

        let bidi = self.env.bidi;
        let default_para_level = match self.env.dir {
            Direction::Auto => None,
            Direction::LR => Some(LTR_LEVEL),
            Direction::RL => Some(RTL_LEVEL),
        };
        let mut levels = vec![];
        let mut level;
        if bidi || default_para_level.is_none() {
            levels = BidiInfo::new(&self.text, default_para_level).levels;
            assert_eq!(self.text.len(), levels.len());
            level = levels[0];
        } else {
            level = default_para_level.unwrap();
        }

        let mut start = 0;
        let mut breaks = Default::default();

        // Iterates over `(pos, hard)` tuples:
        let mut breaks_iter = LineBreakIterator::new(&self.text);
        let mut next_break = breaks_iter.next().unwrap_or((0, false));

        let mut line_start = 0;

        for pos in 1..self.text.len() {
            let is_break = next_break.0 == pos;
            let hard_break = is_break && next_break.1;
            if hard_break || (bidi && levels[pos] != level) {
                let mut range = trim_control(&self.text[start..pos]);
                // trim_control gives us a range within the slice; we need to offset:
                range.start += start as u32;
                range.end += start as u32;

                self.runs.push(Run {
                    range,
                    level,
                    breaks,
                });

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
                breaks.push(pos as u32);
                next_break = breaks_iter.next().unwrap_or((0, false));
            }
        }

        // LineBreakIterator implicitly generates a hard-break at the text end,
        // but if there already is one there it gets omitted. For edit boxes we
        // need to keep this, and to force a run on empty input.
        // TODO: for display-only text (labels), should we not do this or even
        // trim all whitespace? Or is consistent behaviour more important?
        let text_len = self.text.len();
        if start < text_len {
            let mut range = trim_control(&self.text[start..]);
            // trim_control gives us a range within the slice; we need to offset:
            range.start += start as u32;
            range.end += start as u32;
            self.runs.push(Run {
                range,
                level,
                breaks,
            });
        }

        if line_start < self.runs.len() {
            let range = Range::from(line_start..self.runs.len());
            let rtl = self.runs[line_start].level.is_rtl();
            self.line_runs.push(LineRun { range, rtl });
        }
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
