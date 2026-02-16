Kas Text
========

[![kas](https://img.shields.io/badge/GitHub-kas-blueviolet)](https://github.com/kas-gui/kas/)
[![Docs](https://docs.rs/kas-text/badge.svg)](https://docs.rs/kas-text/)

A pure-Rust rich-text processing library suitable for KAS and other GUI tools.

Kas-text is intended to address the needs of common GUI text tasks: fast, able to handle plain text well in common scripts including common effects like bold text and underline, along with support for editing plain texts.

More on what Kas-text does do:

- [x] Font discovery via [Fontique](https://github.com/linebender/parley?tab=readme-ov-file#fontique)
- [x] Font loading and management
- [x] Script-aware font selection and glyph-level fallback
- [x] Emoji support
- [x] Text layout via a choice of [rustybuzz](https://github.com/harfbuzz/rustybuzz) or a simple built-in shaper
- [ ] Vertical text support
- [x] Supports bi-directional texts
- [x] A low-level API for text editing including logical-order and mouse navigation
- [ ] Visual-order navigation
- [ ] Sub-ligature navigation
- [x] Font styles (weight, width, italic)
- [x] Text decorations: highlight range, underline
- [x] Decently optimized: good enough for snappy GUIs

Rich text support is limited to changing font properties (e.g. weight, italic), size, family and underline/strikethrough decorations. A (very limited) Markdown processor is included to facilitate construction of these texts using the lower-level `FormattableText` trait.

Glyph rastering and painting is not implemented here, though `kas-text` can provide font references for [Swash] and (optionally) [ab_glyph] libraries. Check the [`kas-wgpu`] code for an example of rastering and painting.

Text editing is only supported via a low-level API. [`kas_widgets::edit::EditField`](https://docs.rs/kas-widgets/latest/kas_widgets/edit/struct.EditField.html) is a simple editor built over this API.


Examples
--------

Since `kas-text` only concerns text-layout, all examples here are courtesy of KAS GUI. See [the examples directory](https://github.com/kas-gui/kas/tree/master/examples).

![BIDI layout and editing](https://github.com/kas-gui/data-dump/blob/master/kas_0_17/image/layout.png)
![Markdown](https://github.com/kas-gui/data-dump/blob/master/kas_0_17/image/markdown.png)


Alternatives
------------

Pure-Rust alternatives for typesetting and rendering text:

-   [Parley] provides an API for implementing rich text layout. It is backed by [Swash].
-   [COSMIC Text] provides advanced text shaping, layout, and rendering wrapped up into a simple abstraction.
-   [glyph_brush](https://github.com/alexheretic/glyph-brush) is a fast caching text render library using [ab_glyph].


Crates and features
-------------------

Significant external dependencies:

-   [rustybuzz](https://crates.io/crates/rustybuzz): a complete harfbuzz's shaping algorithm port to Rust
-   [fontique](https://crates.io/crates/fontique): Font enumeration and fallback

### Feature flags

This crate has a few optional features (all are disabled by default). See [Cargo.toml](https://github.com/kas-gui/kas-text/blob/master/Cargo.toml#L21) for a full list. Highlighted features:

-   `shaping`: enable text shaping (recommended)
-   `markdown`: rich text support with Markdown parsing (only supports a small subset of Markdown features)


Contributing
--------

Contributions are welcome. For the less straightforward contributions it is
advisable to discuss in an issue before creating a pull-request.

Testing is done in an ad-hoc manner using examples. The [Layout Demo](https://github.com/kas-gui/kas/tree/master/examples#layout) often proves useful for quick tests. It helps to use a patch like that below in `kas/Cargo.toml`:
```toml
[patch.crates-io.kas-text]
path = "../kas-text"
```


Copyright and License
-------

The [COPYRIGHT](COPYRIGHT) file includes a list of contributors who claim
copyright on this project. This list may be incomplete; new contributors may
optionally add themselves to this list.

The KAS library is published under the terms of the Apache License, Version 2.0.
You may obtain a copy of this license from the [LICENSE](LICENSE) file or on
the following web page: <https://www.apache.org/licenses/LICENSE-2.0>


[ab_glyph]: https://github.com/alexheretic/ab-glyph
[Swash]: https://github.com/dfrg/swash
[Parley]: https://github.com/linebender/parley
[COSMIC Text]: https://github.com/linebender/parley
[`kas-wgpu`]: https://crates.io/crates/kas-wgpu
