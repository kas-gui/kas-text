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

            Wrap { line_breaker } => {
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
