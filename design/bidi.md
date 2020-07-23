Bidirectional text
===========

This is a rough design document for what BIDI support should do and how it might
be implemented.

Operation roughly proceeds as follows:

-   split text into paragraphs
-   detect the base paragraph direction
-   detect embedded runs of opposite direction
-   re-order text
-   mirror some glyphs


Environment (env)
----------------

Text processing always has an environment which sets some properties (e.g. size
bounds, font size, alignment). This may carry set some BIDI properties:

-   Set the base paragraph direction: autodetect or force LTR or RTL
-   Determine whether line-breaks reset the paragraph direction detection?
-   Alternative modes (not BIDI compliant but may be useful for editing?):

    -   disable re-ordering of embedded runs (still respecting base direction)
    -   as above, but mirror glyphs instead of re-ordering (so that the whole
        embedded run looks mirrored)
    -   whether left/right arrows mean left/right or prev/next

Note: the author is not familiar with BIDI text entry, and does not know which
of the above options are actually useful or sensible.

Further note: whether each text object has a distinct environment with copies
of all above properties or whether some properties are moved into shared state
is an unanswered question (possibly more applicable to rich text than BIDI).


Processing steps
-----------

### Paragraphs and line breaks

A `Text` object is given a contiguous text which may contain explicit
paragraph breaks, explicit line breaks and implicit line wrapping.
An explicit paragraph break should reset directional detection of the text.
An explicit line break may or may not (make this an env option)?

### Embedded runs

Much of Unicode TR#9 concerns detection of embedded runs with a strongly
directional character requiring an alternate direction.

### Re-ordering text

Unicode TR#9 covers how to re-order code-point sequences on lines.

### Mirroring glyphs

If e.g. an opening bracket is detected in RTL text, typically it is substituted
with a closing bracket to mirror it (although this doesn't work for italic).
In other cases, the renderer may need to draw a glyph mirrored.

### Text-edit position marker

Typically, the edit marker is positioned *before* a glyph; with RTL text this
means to-the-right-of the glyph. With bidirectional text, this means the same
screen position may be used for two different logical positions in the text.
Typically, therefore, text editors use a distinct marker in RTL text sections.

While it seems obvious that the *backspace* key should delete the *previous*
item, it's less clear whether the *left arrow* should move *left* or to the
*previous* item. Possibly this should be an env option.

Ultimately, the preferred text-editing behaviour may depend on the user, type of
text (e.g. code vs prose), and whether text is mostly LTR, mostly RTL or heavily
bidirectional, thus it *may* be sensible to have several options here.


Existing libraries
--------------

[unicode-bidi](https://crates.io/crates/unicode-bidi) (part of Servo) covers
detection of base direction and of embedded runs. It may be a little out-dated
(three years since last release).
[unic-bidi](https://crates.io/crates/unic-bidi) is a fork? (Last release is one
year old.)

[unic-ucd-bidi](https://crates.io/crates/unic-ucd-bidi) provides per-char bidi
class backing data needed by `unic-bidi`.

[unicode-bidi-mirroring](https://crates.io/crates/unicode-bidi-mirroring) covers
mapping glyphs to their mirrored glyph.
