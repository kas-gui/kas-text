# Changelog
The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.3] — 2020-08-14

-   Fix re-ordering of runs on a line (#18)

## [0.1.2] — 2020-08-14

-   Add embedding level to result of `Text::text_glyph_pos` (#17)
-   Fix start offset for wrapped RTL text (#17)
-   Fix `Text::line_index_nearest` for right-most position in line (#17)

## [0.1.1] — 2020-08-13

-   `prepared::Text::positioned_glyphs` now takes an `FnMut` closure and
    emits glyphs in logical order (#13)

## [0.1.0] — 2020-08-11

Initial release version, comprising:

-   basic font loading and font metrics
-   `Environment` specifying font and layout properties
-   `rich::Text` struct (mostly placeholder around a raw `String`)
-   `prepared::Text` struct with state tracking required preparation steps

Text preparation includes:

-   run-breaking with BIDI support
-   glyph shaping via internal algorithm or via HarfBuzz
-   line wrapping and alignment
-   generating a vec of positioned glyphs
-   cursor position lookup (index→coord and coord→index)
-   generating highlighting rects for a range
