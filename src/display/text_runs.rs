// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

#![allow(clippy::unnecessary_unwrap)]

use super::TextDisplay;
use crate::conv::{to_u32, to_usize};
use crate::fonts::{self, FaceId, FontSelector, NoFontMatch};
use crate::format::FormattableText;
use crate::util::ends_with_hard_break;
use crate::{Direction, Range, shaper};
use icu_properties::props::{Emoji, EmojiModifier, RegionalIndicator, Script};
use icu_properties::{CodePointMapData, CodePointSetData};
use icu_segmenter::LineSegmenter;
use std::sync::OnceLock;
use unicode_bidi::{BidiInfo, LTR_LEVEL, RTL_LEVEL};

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

    /// Resolve font face and shape run
    ///
    /// This may sub-divide text as required to find matching fonts.
    fn push_run(
        &mut self,
        font: FontSelector,
        input: shaper::Input,
        range: Range,
        mut breaks: tinyvec::TinyVec<[shaper::GlyphBreak; 4]>,
        special: RunSpecial,
        first_real: Option<char>,
    ) -> Result<(), NoFontMatch> {
        let fonts = fonts::library();
        let font_id = fonts.select_font(&font, input.script.into())?;
        let text = &input.text[range.to_std()];

        // Find a font face
        let mut face_id = None;
        if let Some(c) = first_real {
            face_id = fonts
                .face_for_char(font_id, None, c)
                .expect("invalid FontId");
        }

        let mut face = match face_id {
            Some(id) => id,
            None => {
                // We failed to find a font face for the run
                fonts.first_face_for(font_id).expect("invalid FontId")
            }
        };

        let mut start = 0;
        for (index, c) in text.char_indices() {
            let index = to_u32(index);
            if let Some(new_face) = fonts
                .face_for_char(font_id, Some(face), c)
                .expect("invalid FontId")
                && new_face != face
            {
                if index > start {
                    let sub_range = Range {
                        start: range.start + start,
                        end: range.start + index,
                    };
                    let mut j = 0;
                    for i in 0..breaks.len() {
                        if breaks[i].index < sub_range.end {
                            j = i + 1;
                        }
                    }
                    let rest = breaks.split_off(j);

                    self.runs.push(shaper::shape(
                        input,
                        sub_range,
                        face,
                        breaks,
                        RunSpecial::NoBreak,
                    ));
                    breaks = rest;
                    start = index;
                }

                face = new_face;
            }
        }

        let sub_range = Range {
            start: range.start + start,
            end: range.end,
        };
        self.runs
            .push(shaper::shape(input, sub_range, face, breaks, special));
        Ok(())
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
        if let Some(fmt) = next_fmt.as_ref()
            && fmt.start == 0
        {
            font = fmt.font;
            dpem = fmt.dpem;
            next_fmt = font_tokens.next();
        }

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

        let mut input = shaper::Input {
            text,
            dpem,
            level: levels.first().cloned().unwrap_or(LTR_LEVEL),
            script: Script::Unknown,
        };

        let mut start = 0;
        let mut breaks = Default::default();

        // TODO: allow segmenter configuration
        let segmenter = LineSegmenter::new_auto(Default::default());
        let mut break_iter = segmenter.segment_str(text);
        let mut next_break = break_iter.next();

        let mut first_real = None;
        let mut emoji_state = EmojiState::None;
        let mut emoji_start = 0;
        let mut emoji_end = 0;

        let mut last_is_control = false;
        let mut last_is_htab = false;
        let mut non_control_end = 0;

        for (index, c) in text
            .char_indices()
            .chain(std::iter::once((text.len(), '\0')))
        {
            // Handling for control chars
            if !last_is_control {
                non_control_end = index;
            }
            let is_control = c.is_control();
            let is_htab = c == '\t';
            let mut require_break = is_htab;

            // Is wrapping allowed at this position?
            let is_break = next_break == Some(index);
            let hard_break = is_break && ends_with_hard_break(&text[..index]);
            if is_break {
                next_break = break_iter.next();
            }

            let script = CodePointMapData::<Script>::new().get(c);

            let emoji_break = emoji_state.advance(c);
            let mut new_emoji_start = emoji_start;
            let mut is_emoji = false;
            let prohibit_break = match emoji_break {
                EmojiBreak::None => false,
                EmojiBreak::Start => {
                    require_break = true;
                    new_emoji_start = index;
                    false
                }
                EmojiBreak::Prohibit => {
                    emoji_end = index;
                    true
                }
                EmojiBreak::End => {
                    require_break = true;
                    emoji_end = index;
                    debug_assert!(emoji_end > emoji_start);
                    is_emoji = true;
                    false
                }
                EmojiBreak::Restart => {
                    require_break = true;
                    emoji_end = index;
                    new_emoji_start = index;
                    debug_assert!(emoji_end > emoji_start);
                    is_emoji = true;
                    false
                }
                EmojiBreak::Error => {
                    is_emoji = emoji_end > emoji_start;
                    require_break = is_emoji;
                    false
                }
            };

            // Force end of current run?
            require_break |= levels
                .get(index)
                .map(|level| *level != input.level)
                .unwrap_or(true);

            if let Some(fmt) = next_fmt.as_ref()
                && to_usize(fmt.start) == index
            {
                font = fmt.font;
                dpem = fmt.dpem;
                next_fmt = font_tokens.next();
            }

            let mut new_script = None;
            if is_real(script) {
                if first_real.is_none() {
                    first_real = Some(c);
                }
                if script != input.script {
                    new_script = Some(script);
                    require_break |= is_real(input.script);
                }
            }

            if !prohibit_break && non_control_end > start && (hard_break || require_break) {
                let range = (start..non_control_end).into();
                let special = match () {
                    _ if hard_break => RunSpecial::HardBreak,
                    _ if last_is_htab => RunSpecial::HTab,
                    _ if last_is_control || is_break => RunSpecial::None,
                    _ => RunSpecial::NoBreak,
                };

                if is_emoji {
                    let range = (emoji_start..emoji_end).into();
                    let face = emoji_face_id()?;
                    self.runs
                        .push(shaper::shape(input, range, face, breaks, special));
                } else {
                    self.push_run(font, input, range, breaks, special, first_real)?;
                };
                first_real = None;

                start = index;
                non_control_end = index;
                if let Some(level) = levels.get(index) {
                    input.level = *level;
                }
                input.script = script;
                breaks = Default::default();
            } else if is_break && !is_control {
                // We do break runs when hitting control chars, but only when
                // encountering the next non-control character.
                breaks.push(shaper::GlyphBreak::new(to_u32(index)));
            }

            last_is_control = is_control;
            last_is_htab = is_htab;
            emoji_start = new_emoji_start;
            input.dpem = dpem;
            if let Some(script) = new_script {
                input.script = script;
            }
        }

        let hard_break = ends_with_hard_break(&text);

        // Following a hard break we have an implied empty line.
        if hard_break {
            let range = (text.len()..text.len()).into();
            input.level = default_para_level.unwrap_or(LTR_LEVEL);
            breaks = Default::default();
            self.push_run(font, input, range, breaks, RunSpecial::None, None)?;
        }

        /*
        println!("text: {}", text);
        let fonts = fonts::library();
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
            print!("]");
            if let Some(name) = fonts.get_face_store(run.face_id).name_full() {
                print!(", {name}");
            }
            println!();
        }
        */
        Ok(())
    }
}

fn is_real(script: Script) -> bool {
    !matches!(script, Script::Common | Script::Unknown | Script::Inherited)
}

fn emoji_face_id() -> Result<FaceId, NoFontMatch> {
    static ONCE: OnceLock<Result<FaceId, NoFontMatch>> = OnceLock::new();
    *ONCE.get_or_init(|| {
        let fonts = fonts::library();
        let font = fonts.select_font(&FontSelector::EMOJI, Script::Common.into());
        font.map(|font_id| fonts.first_face_for(font_id).expect("invalid FontId"))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmojiBreak {
    /// Not an Emoji
    None,
    /// Start of an Emoji sequence
    Start,
    /// Mid Emoji sequence, valid
    Prohibit,
    /// End of a valid Emoji sequence
    End,
    /// End of one Emoji and start of another
    Restart,
    /// Error; revert to last known good index
    Error,
}

enum EmojiState {
    None,
    RI1,
    RI2,
    Emoji,
    EMod,
    VarSelector,
    TagModifier,
    ZWJ,
}

impl EmojiState {
    /// Advance the emoji state machine
    ///
    /// Returns whether a break should occur before `c`.
    fn advance(&mut self, c: char) -> EmojiBreak {
        // Reference: https://unicode.org/reports/tr51/#EBNF_and_Regex
        #[allow(non_snake_case)]
        fn end_unless_ZWJ(c: char, b: &mut EmojiBreak) -> EmojiState {
            if c == '\u{200D}' {
                EmojiState::ZWJ
            } else {
                *b = EmojiBreak::End;
                EmojiState::None
            }
        }
        let mut b = EmojiBreak::None;
        *self = match *self {
            EmojiState::None => {
                if CodePointSetData::new::<RegionalIndicator>().contains(c) {
                    b = EmojiBreak::Start;
                    EmojiState::RI1
                } else if CodePointSetData::new::<Emoji>().contains(c) {
                    b = EmojiBreak::Start;
                    EmojiState::Emoji
                } else {
                    EmojiState::None
                }
            }
            EmojiState::RI1 => {
                if CodePointSetData::new::<RegionalIndicator>().contains(c) {
                    b = EmojiBreak::Prohibit;
                    EmojiState::RI2
                } else {
                    b = EmojiBreak::Error;
                    EmojiState::None
                }
            }
            EmojiState::RI2 => end_unless_ZWJ(c, &mut b),
            EmojiState::Emoji => {
                if CodePointSetData::new::<EmojiModifier>().contains(c) {
                    EmojiState::EMod
                } else if c == '\u{FE0F}' {
                    EmojiState::VarSelector
                } else if ('\u{E0020}'..='\u{E007E}').contains(&c) {
                    EmojiState::TagModifier
                } else if c == '\u{200D}' {
                    EmojiState::ZWJ
                } else {
                    b = EmojiBreak::End;
                    EmojiState::None
                }
            }
            EmojiState::EMod => end_unless_ZWJ(c, &mut b),
            EmojiState::VarSelector => {
                if c == '\u{20E3}' {
                    end_unless_ZWJ(c, &mut b)
                } else {
                    b = EmojiBreak::End;
                    EmojiState::None
                }
            }
            EmojiState::TagModifier => {
                if ('\u{E0020}'..='\u{E007E}').contains(&c) {
                    EmojiState::TagModifier
                } else if c == '\u{E007F}' {
                    end_unless_ZWJ(c, &mut b)
                } else {
                    b = EmojiBreak::Error;
                    EmojiState::None
                }
            }
            EmojiState::ZWJ => {
                if CodePointSetData::new::<RegionalIndicator>().contains(c) {
                    EmojiState::RI1
                } else if CodePointSetData::new::<Emoji>().contains(c) {
                    EmojiState::Emoji
                } else {
                    b = EmojiBreak::Error;
                    EmojiState::None
                }
            }
        };
        if b == EmojiBreak::End {
            *self = if CodePointSetData::new::<RegionalIndicator>().contains(c) {
                b = EmojiBreak::Restart;
                EmojiState::RI1
            } else if CodePointSetData::new::<Emoji>().contains(c) {
                b = EmojiBreak::Restart;
                EmojiState::Emoji
            } else {
                EmojiState::None
            };
        }
        b
    }
}
