# Changelog
The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.0] — 2022-12-13

Stabilise support for Generic Associated Types (GATs). This requires Rust 1.65.0,
removes the `gat` feature flag and affects the `FormattableText` trait. #75

Bump dependency versions: `ttf-parser` v0.17.1, `rustybuzz` v0.6.0 (#76),
`fontdb` v0.10.0 (#77).

## [0.5.0] — 2022-08-20

Error handling (#65):

-   Add `NotReady` error type
-   Most methods now return `Result<T, NotReady>` instead of panicking

Text environment (#68):

-   Remove `UpdateEnv` type
-   Rename `Text::new` to `new_env`, `Text::new_multi` to `Text::new` and
    remove `Text::new_single`. Note: in usage, the `Environment::wrap` flag
    is usually set anyway.
-   `Environment` is now `Copy`
-   `Text::env` returns `Environment` by copy not reference
-   `Text::env_mut` replaced with `Text::set_env`, which sets required actions
-   `Environment::dir` renamed to `direction`
-   Enum `Direction` adjusted to include bidi and non-bidi modes.
-   `Environment::flags` and its type `EnvFlags` removed.
    `Environment::wrap: bool` added and `Direction` adjusted (see above).
    `PX_PALIGN` option removed (behaviour is now always enabled).
-   Parameters `dpp` and `pt_size` of `Environment`, `TextDisplay::prepare_runs`
    and `FormattableText::font_tokens` are replaced with the single `dpem`.

Text preparation:

-   Add `Action::VAlign` requiring only `TextDisplay::vertically_align` action
-   Remove `TextDisplay::prepare` (but `TextApi::prepare` remains)
-   `TextDisplay::resize_runs` is no longer a public method
-   `TextDisplay::prepare_runs` may call `resize_runs` automatically depending
    on preparation status
-   Remove `TextApi::resize_runs` and `TextApi::prepare_lines`
-   All `TextApi` and `TextApiExt` methods doing any preparation now do all
    required preparation, and avoid unnecessary steps.

Text measurements (#68):

-   Add `TextDisplay::bounding_box` and `TextApiExt::bounding_box` (#68, #69)
-   Add `TextDisplay::measure_width` and `TextDisplay::vertically_align`
-   Add `TextApi::measure_width` and `TextApi::measure_height`
-   Remove `TextDisplay::line_is_ltr` and `TextApiExt::line_is_ltr`
-   Add `TextApiExt::text_is_rtl`
-   `TextDisplay::line_is_rtl` and `TextApiExt::line_is_rtl` now return type
    `Result<Option<bool>, NotReady>`, returning `Ok(None)` if text is empty
-   `TextDisplay::prepare_lines` returns the bottom-right corner of the bounding
    box around content instead of the size of content.

Font fallback:

-   `FontLibrary::face_for_char` and `face_for_char_or_first` take an extra
    parameter: `last_face_id: Option<FaceId>`. This allows the font fallback
    mechanism to avoid switching the font unnecessarily. In usage, letters and
    numbers are selected as before while other characters are selected from the
    last font face used if possible, resulting in longer runs being passed to
    the shaper when using fallback fonts.

Misc:

-   CI: test stable and check Clippy lints (#69).
-   Add `Range::is_empty`
-   Add `num_glyphs` feature flag (#69)
-   Memory optimisations for `TextDisplay`: remove `line_runs` (#71)
-   Replace `highlight_lines` with `highlight_range` (#72)
-   Add `fonts::any_loaded` (#73)

Fixes:

-   Do not add "line gap" before first line. (In practice this is often 0 anyway.)
-   Do not vertically align text too tall for the input bounds.
-   Markdown formatter: use heading level sizes as defined by CSS
-   Fix position of text highlights on vertically aligned text (#67).
-   Fix `r_bound` for trailing space (#71)

## [0.4.2] — 2022-02-10

-   Spellcheck documentation (#62)
-   Fix selection of best font from a family (#63)

## [0.4.1] — 2021-09-07

-   Document loading order requirements (#59)
-   Additional logging of loaded fonts (#59)

## [0.4.0] — 2021-09-03

This is a minor release (mostly non-breaking). The primary motivation is to
enable `resvg` to access the loaded font database. See PR #58:

-   **Breaking:** update dependencies: `bitflags = 1.3.1`, `fontdb = 0.6.0`,
    `harfbuzz_rs = 2.0`, `rustybuzz = 0.4.0`
-   Make `fontdb::Database` externally readable and set font families
-   Additional control over loading fonts
-   Performance improvements for alias lookups (trim and capitalise earlier)

## [0.3.4] — 2021-07-28

-   Fix sub-pixel positioning (#57)

## [0.3.3] — 2021-07-19

-   Document `raster` module and `Markdown` formatter (#56)
-   Export `DPU` (#56)
-   `Default`, `Debug` and `PartialEq` impls for some `raster` types (#56)

## [0.3.2] — 2021-06-30

-   Minor optimisations to `SpriteDescriptor::new` (#53)

## [0.3.1] — 2021-06-17

-   Make all families fall back to "sans-serif" fonts.
-   Cache `FontLibrary::face_for_char` glyph lookups.

## [0.3.0] — 2021-06-15

This release replaces all non-Rust dependencies allowing easier build/deployment
(though HarfBuzz is kept as an optional dependency). There is also direct
support for glyph rastering and some tweaks to improve raster quality making
even very small font sizes quite legible.

-   Add `Effect::default` method and `default_aux` param to `glyphs_with_effects` (#45)
-   Replace `font-kit` dependency with the pure-Rust `fontdb` using custom font-family lists (#46),
    with support for run-time configuration (#48)
-   Support font fallbacks (#47)
-   Support [rustybuzz](https://github.com/RazrFalcon/rustybuzz) for pure-Rust shaping (#47)
-   Vertical pixel alignment (#49)
-   Extend public API relating to fonts (#49)
-   Add (glyph) `raster` module with `Config` struct and `SpriteDescriptor` cache key (#50)
-   Use pixels-per-Em (dpem) for most glyph sizing, not pixels-per-font-unit (DPU) or height (#50)

## [0.2.1] — 2021-03-31

-   Add `Option<Vec2>` return value to `TextDisplay::prepare` and `TextApi::prepare` (#41)
-   Fix missing run for empty text lines
-   Fix justified text layout (#44)
-   Explicitly avoid justifying last line of justified text (#44)
-   Update `smallvec` to 0.6.1
-   Update `ttf-parser` to 0.12.0

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
