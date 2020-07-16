KAS Text
==========

A rich-text processing library suitable for KAS and other GUI tools.

Development status
----------------

An initial [design document](design/requirements.md) was posted (see #1).

Simple text layout was initially implemented via
[glyph_brush_layout](https://crates.io/crates/glyph_brush_layout).
On reflection, this did not provide a good path to future development, thus
was abandoned.

Line-breaking and simple text shaping have now been implemented directly in
this library using a design more appropriate for inclusion of bidirectional text
processing and usage of an external shaper.

Currently missing features:

-   font fallback support (not planned, but required to support many texts)
-   bidirectional text algorithm (Unicode TR9)
-   usage of an external shaper (`harfbuzz-rf` and/or `rustybuzz`); it is
    intended that this is an optional feature (otherwise using the existing
    simple shaper in this lib)
-   rich text formatting


Contributing
--------

Contributions are welcome. For the less straightforward contributions it is
advisable to discuss in an issue before creating a pull-request.

Testing is currently done in a very ad-hoc manner via KAS examples. This is
facilitated by tying KAS commits to kas-text commit hashes during development
and allows testing editing as well as display.
A comprehensive test framework must consider a *huge* number of cases and the
test framework alone would consitute considerably more work than building this
library, so for now user-testing a bug reports will have to suffice.


Copyright and Licence
-------

The [COPYRIGHT](COPYRIGHT) file includes a list of contributors who claim
copyright on this project. This list may be incomplete; new contributors may
optionally add themselves to this list.

The KAS library is published under the terms of the Apache License, Version 2.0.
You may obtain a copy of this licence from the [LICENSE](LICENSE) file or on
the following webpage: <https://www.apache.org/licenses/LICENSE-2.0>
