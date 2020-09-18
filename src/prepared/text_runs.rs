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
    // TODO: replace this with more `Run` instances?
    pub breaks: SmallVec<[u32; 5]>,
    /// If true, the logical-end of this Run is not a valid break point
    pub no_break: bool,
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
    pub(crate) fn prepare_runs(&mut self) {
        self.runs.clear();
        self.line_runs.clear();

        let bidi = self.env.bidi;
        let default_para_level = match self.env.dir {
            Direction::Auto => None,
            Direction::LR => Some(LTR_LEVEL),
            Direction::RL => Some(RTL_LEVEL),
        };
        let mut levels = vec![];
        let mut level: Level;
        if bidi || default_para_level.is_none() {
            levels = BidiInfo::new(&self.text, default_para_level).levels;
            assert_eq!(self.text.len(), levels.len());
            level = levels.get(0).cloned().unwrap_or(LTR_LEVEL);
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
                    no_break: !is_break,
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

        // The loop above misses the last break at the end of the text.
        // LineBreakIterator always reports a hard break at the text's end
        // regardless of whether the text ends with a line-break char.
        let mut range = trim_control(&self.text[start..]);
        // trim_control gives us a range within the slice; we need to offset:
        range.start += start as u32;
        range.end += start as u32;
        self.runs.push(Run {
            range,
            level,
            breaks,
            no_break: false,
        });
        if line_start < self.runs.len() {
            let range = Range::from(line_start..self.runs.len());
            let rtl = self.runs[line_start].level.is_rtl();
            self.line_runs.push(LineRun { range, rtl });
        }

        // If the text *does* end with a line-break, we add an empty line.
        // TODO: do we want the empty line for labels or only for edit boxes?
        if let Some(c) = self.text.chars().next_back() {
            // Match according to https://www.unicode.org/reports/tr14/#Table1
            if let '\x0C' | '\x0B' | '\u{2028}' | '\u{2029}' | '\r' | '\n' | '\u{85}' = c {
                let text_len = self.text.len();
                let range = Range::from(text_len..text_len);
                level = default_para_level.unwrap_or(LTR_LEVEL);
                breaks = Default::default();
                line_start = self.runs.len();
                self.runs.push(Run {
                    range,
                    level,
                    breaks,
                    no_break: false,
                });

                let range = Range::from(line_start..self.runs.len());
                let rtl = level.is_rtl();
                self.line_runs.push(LineRun { range, rtl });
            }
        }

        println!("text: {}", &self.text);
        for line in self.line_runs.iter() {
            println!("line (rtl={}) runs:", line.rtl);
            for run in &self.runs[line.range.to_std()] {
                let slice = &self.text[run.range];
                println!(
                    "{:?}, text[{}..{}]: '{}', breaks={:?}, no_break={}",
                    run.level, run.range.start, run.range.end, slice, run.breaks, run.no_break,
                );
            }
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
