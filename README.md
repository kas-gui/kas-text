KAS Text
==========

[![kas](https://img.shields.io/badge/GitHub-kas-blueviolet)](https://github.com/kas-gui/kas/)
[![Docs](https://docs.rs/kas-text/badge.svg)](https://docs.rs/kas-text/)

A pure-Rust rich-text processing library suitable for KAS and other GUI tools.

What it does (or may in the future) do:

- [x] Font discovery (very limited; system configuration is ignored)
- [x] Font fallback for missing glyphs
- [x] Text layout: yield a sequence of positioned glyphs
- [x] Supports bi-directional text
- [x] Text shaping (optional) via [rustybuzz](https://github.com/RazrFalcon/rustybuzz) or [harfbuzz](http://harfbuzz.org/)
- [ ] Handle combining diacritics correctly
- [x] Support position navigation / lookup
- [ ] Sub-ligature navigation
- [ ] Visual-order BIDI text navigation
- [ ] Emoticons
- [x] Rich text: choose font by style/weight/family for a sub-range
- [x] Text annotations: highlight range, underline
- [x] Raster glyphs (via `ab_glyph` or `fontdue`)
- [x] Fast-ish: good enough for snappy GUIs; further optimisation possible

What it does not do:

-   Draw: rastering glyphs yields a sequence of sprites. Caching these in a
    glyph atlas and rendering to a texture is beyond the scope of this project
    since it is dependent on the graphics libraries used.
-   Editing: mapping input actions (e.g. from a winit `WindowEvent`) to text
    edit operations is beyond the scope of this project. The API *does* cover
    replacing text ranges and finding the nearest glyph index to a coordinate.
-   Rich text: there is no packaged format for rich text. A `FormattableText`
    trait and a (limited) Markdown processor are included.
-   Full text layout: there is no support for custom inter-paragraph gaps,
    inter-line gaps, embedded images, or horizontal rules.

For more, see the initial [design document](design/requirements.md) and
[issue #1](https://github.com/kas-gui/kas-text/issues/1).


Examples
--------

Since `kas-text` only concerns text-layout, all examples here are courtesy of KAS GUI. See [the examples directory](https://github.com/kas-gui/kas/tree/master/examples).

![BIDI layout and editing](https://github.com/kas-gui/data-dump/blob/master/screenshots/layout.png)
![Markdown](https://github.com/kas-gui/data-dump/blob/master/screenshots/markdown.png)


Alternatives
------------

Pure-Rust alternatives for typesetting and rendering text:

-   [Swash](https://github.com/dfrg/swash): font introspection, shaping, character and script analysis, rendering
-   [fontdue](https://github.com/mooman219/fontdue): rastering and simple layout
-   [glyph_brush](https://github.com/alexheretic/glyph-brush): rendering and simple layout

Non-pure-Rust alternatives include [font-kit](https://crates.io/crates/font-kit)
and [piet](https://crates.io/crates/piet) among others.


Contributing
--------

Contributions are welcome. For the less straightforward contributions it is
advisable to discuss in an issue before creating a pull-request.

Testing is currently done in a very ad-hoc manner via KAS examples. This is
facilitated by tying KAS commits to kas-text commit hashes during development
and allows testing editing as well as display.
A comprehensive test framework must consider a *huge* number of cases and the
test framework alone would constitute considerably more work than building this
library, so for now user-testing and bug reports will have to suffice.


Copyright and License
-------

The [COPYRIGHT](COPYRIGHT) file includes a list of contributors who claim
copyright on this project. This list may be incomplete; new contributors may
optionally add themselves to this list.

The KAS library is published under the terms of the Apache License, Version 2.0.
You may obtain a copy of this license from the [LICENSE](LICENSE) file or on
the following web page: <https://www.apache.org/licenses/LICENSE-2.0>
