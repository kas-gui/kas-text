# Changelog
The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2020-11-23

This release changes a *very large* part of the API. Both `prepared` and `rich`
modules are removed/hidden; three new modules `conv`, `fonts` and `format`
are added/exposed. Formatted text traits are added. The API around the main
`Text` type changes massively.

### Text type

-   Export `prepared::Text` directly and hide the `prepared` module (#30)
-   Split what was `prepared::Text` into `TextDisplay` (which excludes the text,
    environment and formatting data but includes positioned glyph information),
    and `Text` struct which wraps `TextDisplay` along with text and environment (#32)
-   Add `TextApi` trait for run-time polymorphism over `FormattableText` texts (#32, #33)
-   Add `TextApiExt` auto-implemented extension-trait (#33)
-   Add `EditableTextApi` trait (#33)
-   Replace `prepared::Prepare` with `Action` (#30, #33)
-   Rename `Text::positioned_glyphs` → `glyphs` (#31)
-   Add `Text::glyphs_with_effects` for glyph-drawing with underline/strikethrough (#31, #32)
-   Make `TextDisplay::prepare_runs` and `prepare_lines` functions public (#33)
-   Support indentation with tab character `\t` (#34)

### Parsing and representation

-   Add `Markdown` parser (#29 - #37)
-   Add `FontToken` struct, `FormattableText` and `EditableText` traits with
    impls for `&str`, `String` and `Markdown`; these allow custom
    representations of formatted text (#32)
-   Add `FormattableTextDyn` for run-time polymorphism (#33)
-   Add `FormattableText::effect_tokens` for custom underlike/strikethrough effects (#34)

### Fonts API

-   All font API is moved into the public `fonts` module (#28)
-   Add `FontSelector` and `FontLibrary::load_font`, `load_pathbuf` functions (#28)
-   Adjust how loaded fonts are exposed; instead of an `ab_glyph` font we
    expose the font file data and font index (#30, #31)
-   Adjust use of font-size units and add documentation (#31, #33)
-   Switch dependency from `ab_glyph` to `ttf-parser`, but keep compatibility
    with `ab_glyph` (#31)
-   Remove `FontScale` data type (unused; #30)

### Miscellaneous

-   Add type-conversion helpers `conv::to_32` and `to_usize` (#31)
-   Add `Effect` and `EffectFlags` types used for underline and strikethrough (#31)
-   Add `gat` (Generic Associated Types) experimental feature (#33)
-   Move `Environment::bidi` and `wrap` fields to new `flags` field; combine
    `halign` and `valign` to into `align` field (#33)
-   Update `xi-unicode` dependency, allowing cleaner code (#34)

## [0.1.5] — 2020-09-21

-   Rewrite line-wrapping code, supporting must-not-wrap-at-end runs and
    resulting in much cleaner code (#24)
-   Do not allow selection of the position after a space causing a line-wrap (#25)
-   Fix alignment for wrapped RTL text with multiple spaces at wrap point (#26)

## [0.1.4] — 2020-09-09

-   Fixes for empty RTL lines (#21, #23)
-   When wrapping text against paragraph's direction, do not force a line break (#22)

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
