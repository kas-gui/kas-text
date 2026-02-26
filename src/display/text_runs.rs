// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::TextDisplay;
use crate::conv::{to_u32, to_usize};
use crate::fonts::{self, FaceId, FontSelector, NoFontMatch};
use crate::format::FontToken;
use crate::util::ends_with_hard_break;
use crate::{Direction, Range, shaper};
use icu_properties::CodePointMapData;
use icu_properties::props::{
    BinaryProperty, DefaultIgnorableCodePoint, EmojiModifier, EmojiPresentation, RegionalIndicator,
    Script,
};
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
    /// Parameters must match those passed to [`Self::prepare_runs`] for the
    /// result to be valid.
    ///
    /// This updates the result of [`TextDisplay::prepare_runs`] due to change
    /// in font size.
    pub fn resize_runs(&mut self, text: &str, mut font_tokens: impl Iterator<Item = FontToken>) {
        let (mut dpem, _) = read_initial_token(&mut font_tokens);
        let mut next_token = font_tokens.next();

        for run in &mut self.runs {
            while let Some(token) = next_token.as_ref() {
                if token.start > run.range.start {
                    break;
                }
                dpem = token.dpem;
                next_token = font_tokens.next();
            }

            let input = shaper::Input {
                text,
                dpem,
                level: run.level,
                script: run.script,
            };
            let mut breaks = Default::default();
            std::mem::swap(&mut breaks, &mut run.breaks);
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

        let preferred_face = match face_id {
            Some(id) => id,
            None => {
                // We failed to find a font face for the run
                fonts.first_face_for(font_id).expect("invalid FontId")
            }
        };
        let mut face = preferred_face;

        let mut start = 0;
        for (index, c) in text.char_indices() {
            if DefaultIgnorableCodePoint::for_char(c) {
                continue;
            }

            if let Some(new_face) = fonts
                .face_for_char(font_id, Some(preferred_face), c)
                .expect("invalid FontId")
                && new_face != face
            {
                let index = to_u32(index);
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
                    let mut rest = breaks.split_off(j);
                    if rest
                        .first()
                        .map(|b| b.index == sub_range.end)
                        .unwrap_or(false)
                    {
                        rest.remove(0);
                    }

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
    /// The `font_tokens` iterator must not be empty and the first token yielded
    /// must have [`FontToken::start`] == 0. (Failure to do so will result in an
    /// error on debug builds and usage of default values on release builds.)
    ///
    /// Must be called again if any of `text`, `direction` or `font_tokens`
    /// change.
    /// If only `dpem` changes, [`Self::resize_runs`] may be called instead.
    ///
    /// The text is broken into a set of contiguous "level runs". These runs are
    /// maximal slices of the `text` which do not contain explicit line breaks
    /// and have a single text direction according to the
    /// [Unicode Bidirectional Algorithm](http://www.unicode.org/reports/tr9/).
    pub fn prepare_runs(
        &mut self,
        text: &str,
        direction: Direction,
        mut font_tokens: impl Iterator<Item = FontToken>,
    ) -> Result<(), NoFontMatch> {
        // This method constructs a list of "hard lines" (the initial line and any
        // caused by a hard break), each composed of a list of "level runs" (the
        // result of splitting and reversing according to Unicode TR9 aka
        // Bidirectional algorithm), plus a list of "soft break" positions
        // (where wrapping may introduce new lines depending on available space).

        self.runs.clear();

        let (mut dpem, mut font) = read_initial_token(&mut font_tokens);
        let mut next_token = font_tokens.next();
        if let Some(token) = next_token.as_ref()
            && token.start == 0
        {
            font = token.font;
            dpem = token.dpem;
            next_token = font_tokens.next();
        }

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
            let is_htab = c == '\t';
            let mut require_break = last_is_htab;
            let is_control = c.is_control();

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

            if let Some(token) = next_token.as_ref()
                && to_usize(token.start) == index
            {
                require_break = true;
            }

            if is_real(script) && script != input.script {
                require_break |= is_real(input.script);
            }

            if !prohibit_break && (hard_break || require_break) {
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
                    // NOTE: the range may be empty; we need it anyway (unless
                    // we modify the last run's special property).
                    let range = (start..non_control_end).into();
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
            } else {
                if is_break && !is_control && index > start {
                    breaks.push(shaper::GlyphBreak::new(to_u32(index)));
                }

                if input.script == Script::Unknown
                    || matches!(input.script, Script::Common | Script::Inherited) && is_real(script)
                {
                    input.script = script;
                }
            }

            if is_real(script) && first_real.is_none() {
                first_real = Some(c);
            }

            if let Some(token) = next_token.as_ref()
                && to_usize(token.start) == index
            {
                font = token.font;
                input.dpem = token.dpem;
                next_token = font_tokens.next();
                debug_assert!(
                    next_token
                        .as_ref()
                        .map(|token| to_usize(token.start) > index)
                        .unwrap_or(true)
                );
            }

            last_is_control = is_control;
            last_is_htab = is_htab;
            emoji_start = new_emoji_start;
        }

        let hard_break = ends_with_hard_break(text);

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
                "\t{:?}, text[{}..{}], script {:?}, {} glyphs: '{}', ",
                run.level,
                run.range.start,
                run.range.end,
                run.script,
                run.glyphs.len(),
                slice
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

fn read_initial_token(iter: &mut impl Iterator<Item = FontToken>) -> (f32, FontSelector) {
    let Some(FontToken { start, dpem, font }) = iter.next() else {
        debug_assert!(false, "iterator font_tokens is empty");
        return (16.0, FontSelector::default());
    };
    debug_assert_eq!(start, 0, "iterator font_tokens does not start at 0");
    (dpem, font)
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

#[allow(clippy::upper_case_acronyms)]
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
                if RegionalIndicator::for_char(c) {
                    b = EmojiBreak::Start;
                    EmojiState::RI1
                } else if EmojiPresentation::for_char(c) {
                    b = EmojiBreak::Start;
                    EmojiState::Emoji
                } else {
                    EmojiState::None
                }
            }
            EmojiState::RI1 => {
                if RegionalIndicator::for_char(c) {
                    b = EmojiBreak::Prohibit;
                    EmojiState::RI2
                } else {
                    b = EmojiBreak::Error;
                    EmojiState::None
                }
            }
            EmojiState::RI2 => end_unless_ZWJ(c, &mut b),
            EmojiState::Emoji => {
                if EmojiModifier::for_char(c) {
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
                if RegionalIndicator::for_char(c) {
                    EmojiState::RI1
                } else if EmojiPresentation::for_char(c) {
                    EmojiState::Emoji
                } else {
                    b = EmojiBreak::Error;
                    EmojiState::None
                }
            }
        };
        if b == EmojiBreak::End {
            *self = if RegionalIndicator::for_char(c) {
                b = EmojiBreak::Restart;
                EmojiState::RI1
            } else if EmojiPresentation::for_char(c) {
                b = EmojiBreak::Restart;
                EmojiState::Emoji
            } else {
                EmojiState::None
            };
        }
        b
    }
}

/// Warning: test results may depend on system fonts
#[cfg(test)]
mod test {
    use super::*;
    use std::iter;
    use std::ops::Range;
    use unicode_bidi::Level;

    type Expected<'a> = &'a [(Range<usize>, RunSpecial, Level, Script, &'a [u32])];

    fn test_breaking(text: &str, dir: Direction, expected: Expected) {
        let fonts = iter::once(FontToken {
            start: 0,
            dpem: 16.0,
            font: Default::default(),
        });

        let mut display = TextDisplay::default();
        assert!(display.prepare_runs(text, dir, fonts).is_ok());

        for (i, (run, expected)) in display.runs.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                run.range.to_std(),
                expected.0,
                "text range for text \"{text}\", run {i}"
            );
            assert_eq!(
                run.special, expected.1,
                "RunSpecial for text \"{text}\", run {i}"
            );
            assert_eq!(run.level, expected.2, "for text \"{text}\", run {i}");
            assert_eq!(run.script, expected.3, "for text \"{text}\", run {i}");
            assert_eq!(
                run.breaks.iter().map(|b| b.index).collect::<Vec<_>>(),
                expected.4,
                "wrap-points for text \"{text}\", run {i}"
            );
        }
        assert_eq!(display.runs.len(), expected.len(), "number of runs");
    }

    #[test]
    fn test_breaking_simple() {
        let sample = "Layout demo 123";
        test_breaking(
            sample,
            Direction::Auto,
            &[(
                0..sample.len(),
                RunSpecial::None,
                Level::ltr(),
                Script::Latin,
                &[7, 12],
            )],
        );
    }

    #[test]
    fn test_breaking_bidi() {
        let sample = "one אחת שתיים two";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (0..4, RunSpecial::None, Level::ltr(), Script::Latin, &[]),
                (
                    4..21,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Hebrew,
                    &[11],
                ),
                (21..25, RunSpecial::None, Level::ltr(), Script::Latin, &[22]),
            ],
        );

        let sample = "אחת one two שתיים";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (0..7, RunSpecial::None, Level::rtl(), Script::Hebrew, &[]),
                (
                    7..14,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Latin,
                    &[11],
                ),
                (
                    14..25,
                    RunSpecial::None,
                    Level::rtl(),
                    Script::Hebrew,
                    &[15],
                ),
            ],
        );
    }

    #[test]
    fn test_breaking_weak_bidi() {
        let sample = "123 (1-2)";

        let expected_ltr: Expected =
            &[(0..9, RunSpecial::None, Level::ltr(), Script::Common, &[4])];
        test_breaking(sample, Direction::Auto, &expected_ltr[..]);
        test_breaking(sample, Direction::Ltr, &expected_ltr[..]);

        let expected_rtl: Expected = &[
            (
                0..3,
                RunSpecial::NoBreak,
                Level::new(2).unwrap(),
                Script::Common,
                &[],
            ),
            (
                3..5,
                RunSpecial::NoBreak,
                Level::rtl(),
                Script::Common,
                &[4],
            ),
            (
                5..8,
                RunSpecial::NoBreak,
                Level::new(2).unwrap(),
                Script::Common,
                &[],
            ),
            (8..9, RunSpecial::None, Level::rtl(), Script::Common, &[]),
        ];
        test_breaking(sample, Direction::AutoRtl, &expected_rtl[..]);
        test_breaking(sample, Direction::Rtl, &expected_rtl[..]);
    }

    #[test]
    fn test_breaking_rtl() {
        // Additional tests for right-to-left languages: Hebrew, Arabic.
        // Samples are translations of the first article of the UDHR from https://r12a.github.io/

        let sample = "סעיף א. כל בני אדם נולדו בני חורין ושווים בערכם ובזכויותיהם. כולם חוננו בתבונה ובמצפון, לפיכך חובה עליהם לנהוג איש ברעהו ברוח של אחוה.";
        test_breaking(
            sample,
            Direction::Auto,
            &[(
                0..sample.len(),
                RunSpecial::None,
                Level::rtl(),
                Script::Hebrew,
                &[
                    9, 13, 18, 25, 32, 43, 50, 61, 74, 85, 109, 118, 129, 142, 158, 169, 178, 189,
                    200, 207, 218, 227, 232,
                ],
            )],
        );

        let sample = "المادة 1 يولد جميع الناس أحرارًا متساوين في الكرامة والحقوق. وقد وهبوا عقلاً وضميرًا وعليهم أن يعامل بعضهم بعضًا بروح الإخاء.";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (0..13, RunSpecial::None, Level::rtl(), Script::Arabic, &[]),
                (
                    13..14,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    14..sample.len(),
                    RunSpecial::None,
                    Level::rtl(),
                    Script::Arabic,
                    &[
                        15, 24, 33, 44, 59, 74, 79, 94, 110, 117, 128, 139, 154, 167, 172, 183,
                        194, 205, 214,
                    ],
                ),
            ],
        );
    }

    // TODO: find a way to enable this test which is permissive of font
    // variability (namely whether or not font fallbacks cause extra breaks).
    #[cfg(false)]
    #[test]
    fn test_breaking_complex_arabic() {
        // Another, more complex, Arabic sample. Source: https://r12a.github.io/scripts/tutorial/summaries/arabic
        let sample = " عندما يريد العالم أن ‪يتكلّم ‬ ، فهو يتحدّث بلغة يونيكود. تسجّل الآن لحضور المؤتمر الدولي العاشر ليونيكود (Unicode Conference)، الذي سيعقد في 10-12 آذار 1997 بمدينة مَايِنْتْس، ألمانيا. و سيجمع المؤتمر بين خبراء من كافة قطاعات الصناعة على الشبكة العالمية انترنيت ويونيكود، حيث ستتم، على الصعيدين الدولي والمحلي على حد سواء مناقشة سبل استخدام يونكود في النظم القائمة وفيما يخص التطبيقات الحاسوبية، الخطوط، تصميم النصوص والحوسبة متعددة اللغات.";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (
                    0..42,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Arabic,
                    &[1, 12, 21, 34, 39],
                ),
                (
                    42..54,
                    RunSpecial::NoBreak,
                    Level::new(3).unwrap(),
                    Script::Arabic,
                    &[],
                ),
                (
                    54..58,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[55],
                ),
                (
                    58..196,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Arabic,
                    &[62, 69, 82, 91, 107, 118, 127, 138, 153, 166, 179],
                ),
                (
                    // Note that this break occurs due to font fallback where
                    // the Arabic font doesn't contain a (round) bracket.
                    196..197,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Arabic,
                    &[],
                ),
                (
                    197..215,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Latin,
                    &[205],
                ),
                (
                    // Note that this break occurs due to font fallback where
                    // the Arabic font doesn't contain a (round) bracket.
                    215..216,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Arabic,
                    &[],
                ),
                (
                    216..244,
                    RunSpecial::None,
                    Level::rtl(),
                    Script::Arabic,
                    &[219, 228, 239],
                ),
                (
                    244..246,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    246..247,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Script::Common,
                    &[],
                ),
                (
                    247..249,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    249..259,
                    RunSpecial::None,
                    Level::rtl(),
                    Script::Arabic,
                    &[250],
                ),
                (
                    259..263,
                    RunSpecial::NoBreak,
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    263..sample.len(),
                    RunSpecial::None,
                    Level::rtl(),
                    Script::Arabic,
                    &[
                        264, 277, 300, 316, 319, 330, 345, 352, 363, 368, 377, 390, 405, 412, 425,
                        442, 457, 476, 483, 494, 501, 518, 531, 546, 553, 558, 567, 580, 587, 602,
                        615, 620, 631, 646, 657, 664, 683, 704, 719, 730, 743, 760, 773,
                    ],
                ),
            ],
        );
    }
}
