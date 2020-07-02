// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Simple layout engine — copied from glyph_brush_layout

use super::{characters::Characters, SectionGlyph};
use super::{BuiltInLineBreaker, GlyphPositioner, LineBreaker, SectionGeometry, ToSectionText};
use ab_glyph::*;

/// Built-in [`GlyphPositioner`](trait.GlyphPositioner.html) implementations.
///
/// Takes generic [`LineBreaker`](trait.LineBreaker.html) to indicate the wrapping style.
/// See [`BuiltInLineBreaker`](enum.BuiltInLineBreaker.html).
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Layout<L: LineBreaker> {
    /// Renders a single line from left-to-right according to the inner alignment.
    /// Hard breaking will end the line, partially hitting the width bound will end the line.
    SingleLine { line_breaker: L },
    /// Renders multiple lines from left-to-right according to the inner alignment.
    /// Hard breaking characters will cause advancement to another line.
    /// A characters hitting the width bound will also cause another line to start.
    Wrap { line_breaker: L },
}

impl Default for Layout<BuiltInLineBreaker> {
    #[inline]
    fn default() -> Self {
        Layout::default_wrap()
    }
}

impl Layout<BuiltInLineBreaker> {
    #[inline]
    pub fn default_single_line() -> Self {
        Layout::SingleLine {
            line_breaker: BuiltInLineBreaker::default(),
        }
    }

    #[inline]
    pub fn default_wrap() -> Self {
        Layout::Wrap {
            line_breaker: BuiltInLineBreaker::default(),
        }
    }
}

impl<L: LineBreaker> GlyphPositioner for Layout<L> {
    fn calculate_glyphs<F, S>(
        &self,
        fonts: &[F],
        geometry: &SectionGeometry,
        sections: &[S],
    ) -> Vec<SectionGlyph>
    where
        F: Font,
        S: ToSectionText,
    {
        use Layout::{SingleLine, Wrap};

        let SectionGeometry {
            screen_position,
            bounds: (bound_w, bound_h),
            ..
        } = *geometry;

        match *self {
            SingleLine { line_breaker } => Characters::new(
                fonts,
                sections.iter().map(|s| s.to_section_text()),
                line_breaker,
            )
            .words()
            .lines(bound_w)
            .next()
            .map(|line| line.aligned_on_screen(screen_position))
            .unwrap_or_default(),

            Wrap {
                line_breaker,
            } => {
                let mut out = vec![];
                let mut caret = screen_position;

                let lines = Characters::new(
                    fonts,
                    sections.iter().map(|s| s.to_section_text()),
                    line_breaker,
                )
                .words()
                .lines(bound_w);

                for line in lines {
                    // top align can bound check & exit early
                    if caret.1 >= screen_position.1 + bound_h {
                        break;
                    }

                    let line_height = line.line_height();
                    out.extend(line.aligned_on_screen(caret));
                    caret.1 += line_height;
                }

                out
            }
        }
    }
}

#[cfg(test)]
mod layout_test {
    use super::*;
    use crate::layout::SectionText;
    use crate::FontId;
    use approx::assert_relative_eq;
    use once_cell::sync::Lazy;
    use ordered_float::OrderedFloat;
    use std::{collections::*, f32};

    static A_FONT: Lazy<FontRef<'static>> = Lazy::new(|| {
        FontRef::try_from_slice(include_bytes!("../../fonts/DejaVuSansMono.ttf")).unwrap()
    });
    static CJK_FONT: Lazy<FontRef<'static>> = Lazy::new(|| {
        FontRef::try_from_slice(include_bytes!("../../fonts/WenQuanYiMicroHei.ttf")).unwrap()
    });
    static FONT_MAP: Lazy<[&'static FontRef<'static>; 2]> = Lazy::new(|| [&*A_FONT, &*CJK_FONT]);

    /// All the chars used in testing, so we can reverse lookup the glyph-ids
    const TEST_CHARS: &[char] = &[
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R',
        'S', 'T', 'U', 'V', 'W', 'X', 'Q', 'Z', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j',
        'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', ' ', ',',
        '.', '提', '高', '代', '碼', '執', '行', '率', '❤', 'é', 'ß', '\'', '_',
    ];

    /// Turns glyphs into a string, uses `☐` to denote that it didn't work
    fn glyphs_to_common_string<F>(glyphs: &[SectionGlyph], font: &F) -> String
    where
        F: Font,
    {
        glyphs
            .iter()
            .map(|sg| {
                TEST_CHARS
                    .iter()
                    .find(|cc| font.glyph_id(**cc) == sg.glyph.id)
                    .unwrap_or(&'☐')
            })
            .collect()
    }

    /// Checks the order of glyphs in the first arg iterable matches the
    /// second arg string characters
    /// $glyphs: Vec<(Glyph, Color, FontId)>
    macro_rules! assert_glyph_order {
        ($glyphs:expr, $string:expr) => {
            assert_glyph_order!($glyphs, $string, font = &*A_FONT)
        };
        ($glyphs:expr, $string:expr, font = $font:expr) => {{
            assert_eq!($string, glyphs_to_common_string(&$glyphs, $font));
        }};
    }

    /// Compile test for trait stability
    #[allow(unused)]
    #[derive(Hash)]
    enum SimpleCustomGlyphPositioner {}

    impl GlyphPositioner for SimpleCustomGlyphPositioner {
        fn calculate_glyphs<F, S>(
            &self,
            _fonts: &[F],
            _geometry: &SectionGeometry,
            _sections: &[S],
        ) -> Vec<SectionGlyph>
        where
            F: Font,
            S: ToSectionText,
        {
            <_>::default()
        }
    }

    #[test]
    fn zero_scale_glyphs() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello world",
                scale: 0.0.into(),
                ..<_>::default()
            }],
        );

        assert!(glyphs.is_empty(), "{:?}", glyphs);
    }

    #[test]
    fn negative_scale_glyphs() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(-20.0),
                ..<_>::default()
            }],
        );

        assert!(glyphs.is_empty(), "{:?}", glyphs);
    }

    #[test]
    fn single_line_chars_left_simple() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello world");

        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    #[test]
    fn single_line_chars_left_finish_at_newline() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello\nworld",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        assert!(
            glyphs[4].glyph.position.x > 0.0,
            "unexpected last position {:?}",
            glyphs[4].glyph.position
        );
    }

    #[test]
    fn wrap_word_left() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (85.0, f32::INFINITY), // should only be enough room for the 1st word
                ..SectionGeometry::default()
            },
            &[SectionText {
                text: "hello what's _happening_?",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello ");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );

        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (125.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[SectionText {
                text: "hello what's _happening_?",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello what's ");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    #[test]
    fn single_line_limited_horizontal_room() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (50.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello ");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
    }

    #[test]
    fn wrap_layout_with_new_lines() {
        let test_str = "Autumn moonlight\n\
                        a worm digs silently\n\
                        into the chestnut.";

        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: test_str,
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        // newlines don't turn up as glyphs
        assert_glyph_order!(
            glyphs,
            "Autumn moonlighta worm digs silentlyinto the chestnut."
        );
        assert!(
            glyphs[16].glyph.position.y > glyphs[0].glyph.position.y,
            "second line should be lower than first"
        );
        assert!(
            glyphs[36].glyph.position.y > glyphs[16].glyph.position.y,
            "third line should be lower than second"
        );
    }

    #[test]
    fn leftover_max_vmetrics() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (750.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[
                SectionText {
                    text: "Autumn moonlight, ",
                    scale: PxScale::from(30.0),
                    ..SectionText::default()
                },
                SectionText {
                    text: "a worm digs silently ",
                    scale: PxScale::from(40.0),
                    ..SectionText::default()
                },
                SectionText {
                    text: "into the chestnut.",
                    scale: PxScale::from(10.0),
                    ..SectionText::default()
                },
            ],
        );

        for g in glyphs {
            println!("{:?}", (g.glyph.scale, g.glyph.position));
            // all glyphs should have the same ascent drawing position
            let y_pos = g.glyph.position.y;
            assert_relative_eq!(y_pos, A_FONT.as_scaled(40.0).ascent());
        }
    }

    #[test]
    fn eol_new_line_hard_breaks() {
        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[
                SectionText {
                    text: "Autumn moonlight, \n",
                    ..SectionText::default()
                },
                SectionText {
                    text: "a worm digs silently ",
                    ..SectionText::default()
                },
                SectionText {
                    text: "\n",
                    ..SectionText::default()
                },
                SectionText {
                    text: "into the chestnut.",
                    ..SectionText::default()
                },
            ],
        );

        let y_ords: HashSet<OrderedFloat<f32>> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        println!("Y ords: {:?}", y_ords);
        assert_eq!(y_ords.len(), 3, "expected 3 distinct lines");

        assert_glyph_order!(
            glyphs,
            "Autumn moonlight, a worm digs silently into the chestnut."
        );

        let line_2_glyph = &glyphs[18].glyph;
        let line_3_glyph = &&glyphs[39].glyph;
        assert_eq!(line_2_glyph.id, A_FONT.glyph_id('a'));
        assert!(line_2_glyph.position.y > glyphs[0].glyph.position.y);

        assert_eq!(line_3_glyph.id, A_FONT.glyph_id('i'));
        assert!(line_3_glyph.position.y > line_2_glyph.position.y);
    }

    #[test]
    fn single_line_multibyte_chars_finish_at_break() {
        let unicode_str = "❤❤é❤❤\n❤ß❤";
        assert_eq!(
            unicode_str, "\u{2764}\u{2764}\u{e9}\u{2764}\u{2764}\n\u{2764}\u{df}\u{2764}",
            "invisible char funny business",
        );
        assert_eq!(unicode_str.len(), 23);
        assert_eq!(
            xi_unicode::LineBreakIterator::new(unicode_str).find(|n| n.1),
            Some((15, true)),
        );

        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: unicode_str,
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "\u{2764}\u{2764}\u{e9}\u{2764}\u{2764}");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        assert!(
            glyphs[4].glyph.position.x > 0.0,
            "unexpected last position {:?}",
            glyphs[4].glyph.position
        );
    }

    #[test]
    fn no_inherent_section_break() {
        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (50.0, ::std::f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[
                SectionText {
                    text: "The ",
                    ..SectionText::default()
                },
                SectionText {
                    text: "moon",
                    ..SectionText::default()
                },
                SectionText {
                    text: "light",
                    ..SectionText::default()
                },
            ],
        );

        assert_glyph_order!(glyphs, "The moonlight");

        let y_ords: HashSet<OrderedFloat<f32>> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        assert_eq!(y_ords.len(), 2, "Y ords: {:?}", y_ords);

        let first_line_y = y_ords.iter().min().unwrap();
        let second_line_y = y_ords.iter().max().unwrap();

        assert_relative_eq!(glyphs[0].glyph.position.y, first_line_y);
        assert_relative_eq!(glyphs[4].glyph.position.y, second_line_y);
    }

    /// Chinese sentance squeezed into a vertical pipe meaning each character is on
    /// a seperate line.
    #[test]
    fn wrap_word_chinese() {
        let glyphs = Layout::default().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (25.0, f32::INFINITY),
                ..<_>::default()
            },
            &[SectionText {
                text: "提高代碼執行率",
                scale: PxScale::from(20.0),
                font_id: FontId(1),
            }],
        );

        assert_glyph_order!(glyphs, "提高代碼執行率", font = &*CJK_FONT);

        let x_positions: HashSet<_> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.x))
            .collect();
        assert_eq!(x_positions, std::iter::once(OrderedFloat(0.0)).collect());

        let y_positions: HashSet<_> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        assert_eq!(y_positions.len(), 7, "{:?}", y_positions);
    }
}
