// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Text preparation: line breaking and BIDI

use super::Forme;
#[allow(unused)]
use crate::Status;
use crate::conv::{to_u32, to_usize};
use crate::fonts::{self, FaceId, FontSelector, NoFontMatch};
use crate::util::{AnalyzedText, ends_with_hard_break, to_fontique_script};
use crate::{Direction, FontToken, Range, shaper, shaper::GlyphRun};
use icu_properties::CodePointMapData;
use icu_properties::props::{
    BinaryProperty, DefaultIgnorableCodePoint, EmojiModifier, EmojiPresentation, RegionalIndicator,
    Script,
};
use icu_segmenter::LineSegmenter;
use icu_segmenter::options::{LineBreakStrictness, LineBreakWordOption};
use std::sync::OnceLock;

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

impl Forme {
    /// Break `text` into runs, replacing existing content
    ///
    /// The `text` is split into a sequence of runs according to the text
    /// direction, script, font parameters and required font fallbacks (results
    /// may thus depend on available fonts). These runs are then
    /// [shaped](https://en.wikipedia.org/wiki/Text_shaping) into glyph
    /// sequences. [Line wrapping](Self::prepare_lines) should be performed
    /// after this.
    ///
    /// The `font_tokens` iterator controls font selection using
    /// [`FontToken::start`] indices relative to `text` (byte indices).
    /// This iterator must not be empty and the first token yielded
    /// must have [`FontToken::start`] == 0. (Failure to do so will result in an
    /// error on debug builds and usage of default values on release builds.)
    ///
    /// # Preparation status
    ///
    /// [Requires status][Self#states-of-preparation]: none.
    ///
    /// Must be called again if any of `text`, `direction` or `font_tokens`
    /// change.
    #[deprecated(since = "0.10.0", note = "use Self::set_text instead")]
    #[inline]
    pub fn prepare_runs(
        &mut self,
        text: &str,
        direction: Direction,
        font_tokens: impl Iterator<Item = FontToken>,
    ) -> Result<(), NoFontMatch> {
        self.set_text(text, direction)
            .with_tokens(font_tokens, true)?;
        Ok(())
    }

    /// Replace text content and construct shaped glyph-runs
    ///
    /// May be called from any [`Status`]; results in [`Status::Shaped`].
    ///
    /// Call [`Appender::with_tokens`] or [`Appender::with_font`] on the result.
    ///
    /// This method performs most demanding typesetting steps:
    ///
    /// 1. Run-breaking splits the input text into the longest *runs* possible
    ///    such that runs do not contain mandatory line-breaks and have a single
    ///    text direction (more accurately: a single BiDi embedding level),
    ///    [script](https://en.wikipedia.org/wiki/Script_(Unicode)) and set of
    ///    font properties.
    /// 2. Font matching assigns a font to each run based on the script and font
    ///    properties; in some cases, further run-breaking is required to use
    ///    fallback fonts.
    /// 3. [Shaping](https://en.wikipedia.org/wiki/Text_shaping); this either
    ///    uses [rustybuzz](https://crates.io/crates/rustybuzz) (requires the
    ///    `shaping` crate feature) or just does kerning. (Differences between
    ///    the two methods are most apparent when using emojis or complex
    ///    scripts such as Arabic.)
    //
    // Note: the only real difficulty in adding `fn push_text(..)` (to support
    // using multiple disjoint pieces of text) is that text indices get used in
    // various places; such a method would need to offset all text indices used
    // in GlyphRun to make them distinct and correctly ordered.
    #[must_use = "set_text(..) has no effect without calling with_tokens(..) or with_font(..) on the result"]
    pub fn set_text<'a>(&'a mut self, text: &'a str, direction: Direction) -> Appender<'a> {
        self.clear();
        Appender {
            forme: self,
            text: AnalyzedText::new(text, direction),
        }
    }
}

/// A shim for appending text runs
///
/// See [`Forme::set_text`].
#[must_use]
pub struct Appender<'a> {
    forme: &'a mut Forme,
    text: AnalyzedText<'a>,
}

impl<'a> Appender<'a> {
    /// Use a custom line-break strictness
    ///
    /// This only affects subsequent calls to [`Self::with_tokens`] and [`Self::with_font`].
    #[inline]
    pub fn with_line_break_strictness(&mut self, strictness: LineBreakStrictness) -> &mut Self {
        self.text.lb_opts.strictness = Some(strictness);
        self
    }

    /// Use a custom line-break word option
    ///
    /// This only affects subsequent calls to [`Self::with_tokens`] and [`Self::with_font`].
    #[inline]
    pub fn with_word_break_option(&mut self, word_option: LineBreakWordOption) -> &mut Self {
        self.text.lb_opts.word_option = Some(word_option);
        self
    }

    /// Specify a content locale (currently only affects line-breaking)
    ///
    /// This only affects subsequent calls to [`Self::with_tokens`] and [`Self::with_font`].
    #[inline]
    pub fn with_content_locale(&mut self, locale: &'a icu_locale::LanguageIdentifier) -> &mut Self {
        self.text.lb_opts.content_locale = Some(locale);
        self
    }

    /// Append the entire `text` using fonts inferred from `tokens`
    ///
    /// If `imply_empty_final_line` and `text` ends with a mandatory line-break
    /// then an empty text run will be added to represent the final line. This
    /// should not be used when another text part will be appended after this
    /// but should be used for the final text part of a multi-line text editor.
    #[inline]
    pub fn with_tokens(
        &mut self,
        font_tokens: impl Iterator<Item = FontToken>,
        imply_empty_final_line: bool,
    ) -> Result<(), NoFontMatch> {
        self.forme
            .push_text(&self.text, font_tokens, imply_empty_final_line)
    }

    /// Append `&text[range]` using a single font
    #[inline]
    pub fn with_font(
        &mut self,
        range: std::ops::Range<usize>,
        font: FontSelector,
        dpem: f32,
    ) -> Result<&mut Self, NoFontMatch> {
        self.forme.push_text_range(&self.text, range, font, dpem)?;
        Ok(self)
    }
}

impl Forme {
    /// Resolve font face and shape run
    ///
    /// This may sub-divide text as required to find matching fonts.
    #[cfg_attr(test, allow(unused_mut))]
    fn select_font_and_push_run(
        &mut self,
        font: FontSelector,
        input: shaper::Input,
        range: Range,
        mut breaks: tinyvec::TinyVec<[shaper::GlyphBreak; 4]>,
        special: RunSpecial,
        first_real: Option<char>,
    ) -> Result<(), NoFontMatch> {
        let fonts = fonts::library();
        let font_id = fonts.select_font(&font, to_fontique_script(input.script))?;
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
        #[cfg_attr(test, allow(unused_variables))]
        for (index, c) in text.char_indices() {
            if DefaultIgnorableCodePoint::for_char(c) {
                continue;
            }

            // HACK: disable font-fallback breaking in tests to get repeatable results
            #[cfg(not(test))]
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

                    self.push_run(shaper::shape(
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
        self.push_run(shaper::shape(input, sub_range, face, breaks, special));
        Ok(())
    }

    /// Break `text` into runs and push
    ///
    /// If `imply_empty_final_line` and `text` ends with a mandatory line-break
    /// then an empty text run will be added to represent the final line. This
    /// should not be used when another text part will be appended after this
    /// but should be used for the final text part of a multi-line text editor.
    fn push_text(
        &mut self,
        text: &AnalyzedText<'_>,
        mut font_tokens: impl Iterator<Item = FontToken>,
        imply_empty_final_line: bool,
    ) -> Result<(), NoFontMatch> {
        let (mut dpem, mut font) = read_initial_token(&mut font_tokens);

        let mut start = 0;
        for token in font_tokens {
            let end = to_usize(token.start);
            self.push_text_range(text, start..end, font, dpem)?;

            start = end;
            dpem = token.dpem;
            font = token.font;
        }

        let len = text.len();
        if start < len || len == 0 {
            self.push_text_range(text, start..len, font, dpem)?;
        }

        // Following a hard break we have an implied empty line.
        if imply_empty_final_line && ends_with_hard_break(text) {
            let input = shaper::Input {
                text,
                dpem,
                base_level: text.default_level(),
                level: text.default_level(),
                script: Script::Unknown,
            };
            let range = (text.len()..text.len()).into();
            let breaks = Default::default();
            self.select_font_and_push_run(font, input, range, breaks, RunSpecial::None, None)?;
        }

        /*
        println!("text: {}", &text[..]);
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

    /// Break `&text[range]` into runs and push
    fn push_text_range(
        &mut self,
        text: &AnalyzedText<'_>,
        range: std::ops::Range<usize>,
        font: FontSelector,
        dpem: f32,
    ) -> Result<(), NoFontMatch> {
        let starting_para_i = text.find_paragraph(range.start);

        let mut input = shaper::Input {
            text,
            dpem,
            base_level: text
                .paragraph(starting_para_i)
                .map(|p| p.level)
                .unwrap_or(text.default_level()),
            level: text.level(range.start).unwrap_or(text.default_level()),
            script: Script::Unknown,
        };
        let mut next_para_i = starting_para_i + 1;

        let mut breaks = Default::default();
        let mut start = range.start;

        let segmenter = LineSegmenter::new_auto(text.lb_opts);
        let mut break_iter = segmenter.segment_str(&input.text[range.clone()]);
        let mut next_break = break_iter.next();

        let mut first_real = None;
        let mut emoji_state = EmojiState::None;
        let mut emoji_start = 0;
        let mut emoji_end = 0;

        let mut last_is_control = false;
        let mut last_is_htab = false;
        let mut non_control_end = 0;

        for (sub_index, c) in input.text[range.clone()]
            .char_indices()
            .chain(std::iter::once((range.len(), '\0')))
        {
            let text_index = sub_index + range.start;

            // Handling for control chars
            if !last_is_control {
                non_control_end = text_index;
            }
            let is_htab = c == '\t';
            let mut require_break = last_is_htab;
            let is_control = c.is_control();

            // Is wrapping allowed at this position?
            let is_break = next_break == Some(sub_index);
            let hard_break = is_break && sub_index > 0 && ends_with_hard_break(&text[..text_index]);
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
                    new_emoji_start = text_index;
                    false
                }
                EmojiBreak::Prohibit => {
                    emoji_end = text_index;
                    true
                }
                EmojiBreak::End => {
                    require_break = true;
                    emoji_end = text_index;
                    debug_assert!(emoji_end > emoji_start);
                    is_emoji = true;
                    false
                }
                EmojiBreak::Restart => {
                    require_break = true;
                    emoji_end = text_index;
                    new_emoji_start = text_index;
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
            require_break |= text
                .level(text_index)
                .map(|level| level != input.level)
                .unwrap_or(true);

            if is_real(script) && script != input.script {
                require_break |= is_real(input.script);
            }

            let is_end = text_index == range.end;
            if is_end || !prohibit_break && (hard_break || require_break) {
                let special = match () {
                    _ if hard_break => RunSpecial::HardBreak,
                    _ if last_is_htab => RunSpecial::HTab,
                    _ if last_is_control || is_break => RunSpecial::None,
                    _ => RunSpecial::NoBreak,
                };

                if is_emoji {
                    let range = (emoji_start..emoji_end).into();
                    let face = emoji_face_id()?;
                    self.push_run(shaper::shape(input, range, face, breaks, special));
                } else {
                    // NOTE: the range may be empty; we need it anyway (unless
                    // we modify the last run's special property).
                    let range = (start..non_control_end).into();
                    self.select_font_and_push_run(font, input, range, breaks, special, first_real)?;
                };
                first_real = None;

                start = text_index;
                non_control_end = text_index;
                while let Some(para) = text.paragraph(next_para_i)
                    && para.range.start <= text_index
                {
                    input.base_level = para.level;
                    next_para_i += 1;
                }
                if let Some(level) = text.level(text_index) {
                    input.level = level;
                }
                input.script = script;
                breaks = Default::default();
            } else {
                if is_break && !is_control && text_index > start {
                    breaks.push(shaper::GlyphBreak::new(to_u32(text_index)));
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

            last_is_control = is_control;
            last_is_htab = is_htab;
            emoji_start = new_emoji_start;
        }

        Ok(())
    }

    #[inline]
    fn push_run(&mut self, run: GlyphRun) {
        self.runs.push(run);
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
        let font = fonts.select_font(&FontSelector::EMOJI, to_fontique_script(Script::Common));
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

    type Expected<'a> = &'a [(Range<usize>, RunSpecial, Level, Level, Script, &'a [u32])];

    fn test_breaking(text: &str, dir: Direction, expected: Expected) {
        let fonts = iter::once(FontToken {
            start: 0,
            dpem: 16.0,
            font: Default::default(),
        });

        let mut forme = Forme::default();
        assert!(forme.set_text(text, dir).with_tokens(fonts, false).is_ok());

        for (i, (run, expected)) in forme.runs.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                run.range.to_std(),
                expected.0,
                "text range for text \"{text}\", run {i}"
            );
            assert_eq!(
                run.special, expected.1,
                "RunSpecial for text \"{text}\", run {i}"
            );
            assert_eq!(run.base_level, expected.2, "for text \"{text}\", run {i}");
            assert_eq!(run.level, expected.3, "for text \"{text}\", run {i}");
            assert_eq!(
                run.breaks.iter().map(|b| b.index).collect::<Vec<_>>(),
                expected.5,
                "wrap-points for text \"{text}\", run {i}"
            );
        }
        assert_eq!(forme.runs.len(), expected.len(), "number of runs");
    }

    #[test]
    fn test_breaking_empty() {
        let sample = "";

        test_breaking(
            sample,
            Direction::Auto,
            &[(
                0..0,
                RunSpecial::None,
                Level::ltr(),
                Level::ltr(),
                Script::Unknown,
                &[],
            )],
        );
        test_breaking(
            sample,
            Direction::Ltr,
            &[(
                0..0,
                RunSpecial::None,
                Level::ltr(),
                Level::ltr(),
                Script::Unknown,
                &[],
            )],
        );
        test_breaking(
            sample,
            Direction::AutoRtl,
            &[(
                0..0,
                RunSpecial::None,
                Level::rtl(),
                Level::rtl(),
                Script::Unknown,
                &[],
            )],
        );
        test_breaking(
            sample,
            Direction::Rtl,
            &[(
                0..0,
                RunSpecial::None,
                Level::rtl(),
                Level::rtl(),
                Script::Unknown,
                &[],
            )],
        );
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
                (
                    0..4,
                    RunSpecial::None,
                    Level::ltr(),
                    Level::ltr(),
                    Script::Latin,
                    &[],
                ),
                (
                    4..21,
                    RunSpecial::NoBreak,
                    Level::ltr(),
                    Level::rtl(),
                    Script::Hebrew,
                    &[11],
                ),
                (
                    21..25,
                    RunSpecial::None,
                    Level::ltr(),
                    Level::ltr(),
                    Script::Latin,
                    &[22],
                ),
            ],
        );

        let sample = "אחת one two שתיים";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (
                    0..7,
                    RunSpecial::None,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Hebrew,
                    &[],
                ),
                (
                    7..14,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Latin,
                    &[11],
                ),
                (
                    14..25,
                    RunSpecial::None,
                    Level::rtl(),
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

        let expected_ltr: Expected = &[(
            0..9,
            RunSpecial::None,
            Level::ltr(),
            Level::ltr(),
            Script::Common,
            &[4],
        )];
        test_breaking(sample, Direction::Auto, &expected_ltr[..]);
        test_breaking(sample, Direction::Ltr, &expected_ltr[..]);

        let expected_rtl: Expected = &[
            (
                0..3,
                RunSpecial::NoBreak,
                Level::rtl(),
                Level::new(2).unwrap(),
                Script::Common,
                &[],
            ),
            (
                3..5,
                RunSpecial::NoBreak,
                Level::rtl(),
                Level::rtl(),
                Script::Common,
                &[4],
            ),
            (
                5..8,
                RunSpecial::NoBreak,
                Level::rtl(),
                Level::new(2).unwrap(),
                Script::Common,
                &[],
            ),
            (
                8..9,
                RunSpecial::None,
                Level::rtl(),
                Level::rtl(),
                Script::Common,
                &[],
            ),
        ];
        test_breaking(sample, Direction::AutoRtl, &expected_rtl[..]);
        test_breaking(sample, Direction::Rtl, &expected_rtl[..]);
    }

    // Additional tests for right-to-left languages: Hebrew, Arabic.
    // Samples are translations of the first article of the UDHR from https://r12a.github.io/
    #[test]
    fn test_breaking_hebrew() {
        let sample = "סעיף א. כל בני אדם נולדו בני חורין ושווים בערכם ובזכויותיהם. כולם חוננו בתבונה ובמצפון, לפיכך חובה עליהם לנהוג איש ברעהו ברוח של אחוה.";
        test_breaking(
            sample,
            Direction::Auto,
            &[(
                0..sample.len(),
                RunSpecial::None,
                Level::rtl(),
                Level::rtl(),
                Script::Hebrew,
                &[
                    9, 13, 18, 25, 32, 43, 50, 61, 74, 85, 109, 118, 129, 142, 158, 169, 178, 189,
                    200, 207, 218, 227, 232,
                ],
            )],
        );
    }

    #[test]
    fn test_breaking_arabic() {
        let sample = "المادة 1 يولد جميع الناس أحرارًا متساوين في الكرامة والحقوق. وقد وهبوا عقلاً وضميرًا وعليهم أن يعامل بعضهم بعضًا بروح الإخاء.";
        test_breaking(
            sample,
            Direction::Auto,
            &[
                (
                    0..13,
                    RunSpecial::None,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Arabic,
                    &[],
                ),
                (
                    13..14,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    14..sample.len(),
                    RunSpecial::None,
                    Level::rtl(),
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
                    Level::rtl(),
                    Script::Arabic,
                    &[1, 12, 21, 34, 39],
                ),
                (
                    42..54,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(3).unwrap(),
                    Script::Arabic,
                    &[],
                ),
                (
                    54..58,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[55],
                ),
                (
                    58..197,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Arabic,
                    &[62, 69, 82, 91, 107, 118, 127, 138, 153, 166, 179, 196],
                ),
                (
                    197..215,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Latin,
                    &[205],
                ),
                (
                    215..244,
                    RunSpecial::None,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Arabic,
                    &[219, 228, 239],
                ),
                (
                    244..246,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    246..247,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Common,
                    &[],
                ),
                (
                    247..249,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    249..259,
                    RunSpecial::None,
                    Level::rtl(),
                    Level::rtl(),
                    Script::Arabic,
                    &[250],
                ),
                (
                    259..263,
                    RunSpecial::NoBreak,
                    Level::rtl(),
                    Level::new(2).unwrap(),
                    Script::Common,
                    &[],
                ),
                (
                    263..sample.len(),
                    RunSpecial::None,
                    Level::rtl(),
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
