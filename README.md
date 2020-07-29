KAS Text
==========

A rich-text processing library suitable for KAS and other GUI tools.

Development status
----------------

An initial [design document](design/requirements.md) was posted (see
[#1](https://github.com/kas-gui/kas-text/issues/1)).

Glyph layout is implemented twice: via a simple algorithm (supporting kerning
but not shaping), and via [HarfBuzz](https://harfbuzz.github.io/). The `shaping`
feature flag enables the latter implementation, at the cost of extra dependencies.

Text wrapping is enabled using `xi-unicode` to find break points. Bidirectional
text support is enabled, using `unicode-bidi` to find embedding levels and an
internal algorithm to rearrange text runs, but with some limitations (see
[#9](https://github.com/kas-gui/kas-text/pull/9)).
These two features are necessarily interlinked.

Currently missing features:

-   text navigation is somewhat broken
-   font fallback support (not planned, but potentially required to support
    many texts, depending on the primary font used)
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
library, so for now user-testing and bug reports will have to suffice.


Copyright and Licence
-------

The [COPYRIGHT](COPYRIGHT) file includes a list of contributors who claim
copyright on this project. This list may be incomplete; new contributors may
optionally add themselves to this list.

The KAS library is published under the terms of the Apache License, Version 2.0.
You may obtain a copy of this licence from the [LICENSE](LICENSE) file or on
the following webpage: <https://www.apache.org/licenses/LICENSE-2.0>
