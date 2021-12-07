KAS Text
==========

[![kas](https://img.shields.io/badge/GitHub-kas-blueviolet)](https://github.com/kas-gui/kas/)
[![Docs](https://docs.rs/kas-text/badge.svg)](https://docs.rs/kas-text/)

A rich-text processing library suitable for KAS and other GUI tools.

What it does (or may in the future) do:

- [ ] Provides a representation for rich-text
- [x] Font discovery (very limited; system configuration is ignored)
- [x] Font fallback for missing glyphs (performance issues)
- [ ] Emoticons (not really planned)
- [x] Transforms input text to a sequence of positioned glyphs
- [x] Performs line-wrapping and alignment
- [x] Supports bi-directional text
- [ ] Vertical text (not planned)
- [x] Supports font shaping via HarfBuzz (optional: `shaping` feature; requires HarfBuzz library)
- [x] Simple integrated "shaper" supporting kerning
- [ ] Handle combining diacritics correctly
- [x] Provides helpers for text editing / navigation
- [ ] Visual-order BIDI text navigation
- [ ] Sub-ligature navigation
- [x] Fast line-wrapping when only width changes
- [ ] Scale well to large documents
- [x] Raster glyphs

What it does not do:

-   Draw text. This may or may not be done via a GPU, and is beyond the scope of this library
-   Directly handle text editing — this is mostly about handling input, however
    this library does provide helper methods for navigating prepared text

For more, see the initial [design document](design/requirements.md) and
[issue #1](https://github.com/kas-gui/kas-text/issues/1).


Examples
--------

Since `kas-text` only concerns text-layout, all examples here are courtesy of KAS GUI. See [the examples directory](https://github.com/kas-gui/kas/tree/master/examples).

![BIDI layout and editing](https://github.com/kas-gui/data-dump/blob/master/screenshots/layout.png)
![Markdown](https://github.com/kas-gui/data-dump/blob/master/screenshots/markdown.png)


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
