// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::Text;
use crate::Range;
use smallvec::SmallVec;
use xi_unicode::LineBreakIterator;

#[derive(Clone, Debug)]
pub(crate) struct Run {
    // TODO: add append-to-previous-line property (for now always false)
    // TODO: support reversed texts
    /// Range in source text
    pub range: Range,
    /// All soft-break locations within this range (excludes end)
    pub breaks: SmallVec<[u32; 5]>,
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

        let mut start = 0;
        let mut breaks = Default::default();

        for (pos, hard) in LineBreakIterator::new(&self.text) {
            if hard && start < pos {
                let range = trim_control(&self.text[start..pos]);
                self.runs.push(Run { range, breaks });
                start = pos;
                breaks = Default::default();
            }
            if !hard {
                breaks.push(pos as u32);
            }
        }

        assert_eq!(start, self.text.len()); // iterator always generates a break at the end
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
