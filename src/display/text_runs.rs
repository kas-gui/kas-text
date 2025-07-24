// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

#![allow(clippy::unnecessary_unwrap)]

use super::TextDisplay;
use crate::conv::{to_u32, to_usize};
use crate::fonts::{self, FontSelector, NoFontMatch};
use crate::format::FormattableText;
use crate::{script_to_fontique, shaper, Direction};
use swash::text::cluster::Boundary;
use swash::text::LineBreak as LB;
use unicode_bidi::{BidiClass, BidiInfo, LTR_LEVEL, RTL_LEVEL};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RunSpecial {
    None,
    /// Run ends with a hard break
    HardBreak,
    /// Run does not end with a break
    NoBreak,
    /// Run is a horizontal tab (run is a single char only)
    HTab,
}

impl TextDisplay {
    /// Update font size
    ///
    /// [Requires status][Self#status-of-preparation]: level runs have been
    /// prepared and are valid in all ways except size (`dpem`).
    ///
    /// This updates the result of [`TextDisplay::prepare_runs`] due to change
    /// in font size.
    pub fn resize_runs<F: FormattableText + ?Sized>(&mut self, text: &F, mut dpem: f32) {
        let mut font_tokens = text.font_tokens(dpem);
        let mut next_fmt = font_tokens.next();

        let text = text.as_str();

        for run in &mut self.runs {
            while let Some(fmt) = next_fmt.as_ref() {
                if fmt.start > run.range.start {
                    break;
                }
                dpem = fmt.dpem;
                next_fmt = font_tokens.next();
            }

            let input = shaper::Input {
                text,
                dpem,
                level: run.level,
                script: run.script,
            };
            let mut breaks = Default::default();
            std::mem::swap(&mut breaks, &mut run.breaks);
            if run.level.is_rtl() {
                breaks.reverse();
            }
            *run = shaper::shape(input, run.range, run.face_id, breaks, run.special);
        }
    }

    /// Break text into level runs
    ///
    /// [Requires status][Self#status-of-preparation]: none.
    ///
    /// Must be called again if any of `text`, `direction` or `font` change.
    /// If only `dpem` changes, [`Self::resize_runs`] may be called instead.
    ///
    /// The text is broken into a set of contiguous "level runs". These runs are
    /// maximal slices of the `text` which do not contain explicit line breaks
    /// and have a single text direction according to the
    /// [Unicode Bidirectional Algorithm](http://www.unicode.org/reports/tr9/).
    pub fn prepare_runs<F: FormattableText + ?Sized>(
        &mut self,
        text: &F,
        direction: Direction,
        mut font: FontSelector,
        mut dpem: f32,
    ) -> Result<(), NoFontMatch> {
        // This method constructs a list of "hard lines" (the initial line and any
        // caused by a hard break), each composed of a list of "level runs" (the
        // result of splitting and reversing according to Unicode TR9 aka
        // Bidirectional algorithm), plus a list of "soft break" positions
        // (where wrapping may introduce new lines depending on available space).

        self.runs.clear();

        let mut font_tokens = text.font_tokens(dpem);
        let mut next_fmt = font_tokens.next();
        if let Some(fmt) = next_fmt.as_ref() {
            if fmt.start == 0 {
                font = fmt.font;
                dpem = fmt.dpem;
                next_fmt = font_tokens.next();
            }
        }

        let fonts = fonts::library();
        let text = text.as_str();

        let default_para_level = match direction {
            Direction::Auto => None,
            Direction::AutoRtl => {
                use unicode_bidi::Direction::*;
                match unicode_bidi::get_base_direction(text) {
                    Ltr | Rtl => None,
                    Mixed => Some(RTL_LEVEL),
                }
            }
            Direction::Ltr => Some(LTR_LEVEL),
            Direction::Rtl => Some(RTL_LEVEL),
        };
        let info = BidiInfo::new(text, default_para_level);
        let levels = info.levels;
        assert_eq!(text.len(), levels.len());
        let classes = info.original_classes;

        let mut input = shaper::Input {
            text,
            dpem,
            level: levels.first().cloned().unwrap_or(LTR_LEVEL),
            script: UNKNOWN_SCRIPT,
        };

        let mut start = 0;
        let mut breaks = Default::default();

        let mut analyzer = swash::text::analyze(text.chars());
        let mut last_props = None;

        let mut face_id = None;

        let mut last_is_control = false;
        let mut last_is_htab = false;
        let mut non_control_end = 0;

        for (pos, c) in text.char_indices() {
            // Handling for control chars
            if !last_is_control {
                non_control_end = pos;
            }
            let is_control = c.is_control();
            let is_htab = c == '\t';
            let control_break = is_htab || (last_is_control && !is_control);

            let (props, boundary) = analyzer.next().unwrap();
            last_props = Some(props);

            // Forcibly end the line?
            let hard_break = boundary == Boundary::Mandatory;
            // Is wrapping allowed at this position?
            let is_break = hard_break || boundary == Boundary::Line;

            // Force end of current run?
            let bidi_break = levels[pos] != input.level;

            if let Some(fmt) = next_fmt.as_ref() {
                if to_usize(fmt.start) == pos {
                    font = fmt.font;
                    dpem = fmt.dpem;
                    next_fmt = font_tokens.next();
                }
            }

            if input.script == UNKNOWN_SCRIPT && props.script().is_real() {
                input.script = script_to_fontique(props.script());
            }

            let opt_last_face = if matches!(
                classes[pos],
                BidiClass::L | BidiClass::R | BidiClass::AL | BidiClass::EN | BidiClass::AN
            ) {
                None
            } else {
                face_id
            };
            let font_id = fonts.select_font(&font, input.script)?;
            let new_face_id = fonts
                .face_for_char(font_id, opt_last_face, c)
                .expect("invalid FontId")
                .or(face_id);
            let font_break = face_id.is_some() && new_face_id != face_id;

            if hard_break || control_break || bidi_break || font_break {
                // TODO: sometimes this results in empty runs immediately
                // following another run. Ideally we would either merge these
                // into the previous run or not simply break in this case.
                // Note: the prior run may end with NoBreak while the latter
                // (and the merge result) do not.
                let range = (start..non_control_end).into();
                let special = match () {
                    _ if hard_break => RunSpecial::HardBreak,
                    _ if last_is_htab => RunSpecial::HTab,
                    _ if last_is_control || is_break => RunSpecial::None,
                    _ => RunSpecial::NoBreak,
                };

                let face = face_id.expect("have a set face_id");
                self.runs
                    .push(shaper::shape(input, range, face, breaks, special));

                start = pos;
                non_control_end = pos;
                input.level = levels[pos];
                breaks = Default::default();
                input.script = UNKNOWN_SCRIPT;
            } else if is_break && !is_control {
                // We do break runs when hitting control chars, but only when
                // encountering the next non-control character.
                breaks.push(shaper::GlyphBreak::new(to_u32(pos)));
            }

            last_is_control = is_control;
            last_is_htab = is_htab;
            face_id = new_face_id;
            input.dpem = dpem;
        }

        debug_assert!(analyzer.next().is_none());
        let hard_break = last_props
            .map(|props| matches!(props.line_break(), LB::BK | LB::CR | LB::LF | LB::NL))
            .unwrap_or(false);

        // Conclude: add last run. This may be empty, but we want it anyway.
        if !last_is_control {
            non_control_end = text.len();
        }
        let range = (start..non_control_end).into();
        let special = match () {
            _ if hard_break => RunSpecial::HardBreak,
            _ if last_is_htab => RunSpecial::HTab,
            _ => RunSpecial::None,
        };

        let font_id = fonts.select_font(&font, input.script)?;
        if let Some(id) = face_id {
            if !fonts.contains_face(font_id, id).expect("invalid FontId") {
                face_id = None;
            }
        }
        let face_id =
            face_id.unwrap_or_else(|| fonts.first_face_for(font_id).expect("invalid FontId"));
        self.runs
            .push(shaper::shape(input, range, face_id, breaks, special));

        // Following a hard break we have an implied empty line.
        if hard_break {
            let range = (text.len()..text.len()).into();
            input.level = default_para_level.unwrap_or(LTR_LEVEL);
            breaks = Default::default();
            self.runs.push(shaper::shape(
                input,
                range,
                face_id,
                breaks,
                RunSpecial::None,
            ));
        }

        /*
        println!("text: {}", text);
        for run in &self.runs {
            let slice = &text[run.range];
            print!(
                "\t{:?}, text[{}..{}]: '{}', ",
                run.level, run.range.start, run.range.end, slice
            );
            match run.special {
                RunSpecial::None => (),
                RunSpecial::HardBreak => print!("HardBreak, "),
                RunSpecial::NoBreak => print!("NoBreak, "),
                RunSpecial::HTab => print!("HTab, "),
            }
            print!("breaks=[");
            let mut iter = run.breaks.iter();
            if let Some(b) = iter.next() {
                print!("{}", b.index);
            }
            for b in iter {
                print!(", {}", b.index);
            }
            println!("]");
        }
        */
        Ok(())
    }
}

trait ScriptExt {
    #[allow(clippy::wrong_self_convention)]
    fn is_real(self) -> bool;
}
impl ScriptExt for swash::text::Script {
    fn is_real(self) -> bool {
        use swash::text::Script::*;
        !matches!(self, Common | Unknown | Inherited)
    }
}

pub(crate) const UNKNOWN_SCRIPT: fontique::Script = fontique::Script(*b"Zzzz");
