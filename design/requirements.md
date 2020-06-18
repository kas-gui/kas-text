Text processing
========

Objective: allow text to be input in various forms (plain text, markdown, html)
then displayed in a given enviroment (an AABB plus default font selection,
including properties such as size and colour).

Essentially, this is meant to abstract over the entire font processing line
except actual rendering, spitting out either a list of pre-positioned glyphs or
the input values required by compatible rendering libraries.
https://mrandri19.github.io/2019/07/24/modern-text-rendering-linux-overview.html

Scope:

-   Rich-text representation: able to build a model of a rich text document
    or paragraph. It may be desirable to let this model cache transformations
    needed to fit its contents into a given display environment.
-   Font management and selection: maintain a collection of loaded fonts and
    assign each piece of text a `FontId` based on font family/property
    restrictions. Initially this will lean heavily on `font-kit`.
-   Rich-text parsing: able to convert input formats (e.g. via Markdown and
    HTML) to the internal model. (Later these translators may be moved to
    external libraries.)
-   Bidirectional text support: this will be omitted in initial versions but the
    design should support incorporating this in an update.
-   Text layout and shaping: the design should be compatible with external
    text shapers such as HarfBuzz, although it may not initially support them.
    This implies that the output sent to the rasteriser should be in the form of
    positioned glyphs. The library may embed a simple shaper, comparable with
    `glyph_brush_layout`.
-   Line-wrapping: support for this must be integrated with the bidirectional
    algorithm; additionally, it is required for multi-line text editing.
-   Text metrics: able to calculate text bounds and translate string indices to
    glyph coordinates and vice-versa.
-   Embedded objects: ideally this should support user-defined objects such as
    images and widgets being embedded within text.

Dependencies:

-   `font-kit` (possibly only for font selection only)
-   `ab_glyph` (possibly)
-   `unicode-linebreak` for determining line-break positions
-   `unicode-bidi` — but likely not since its API appears incompatible with
    rich text and embedded objects
-   `harfbuzz_rs` (possibly only later and behind a feature gate)
-   `allsorts` (possibly only later and behind a feature gate)
-   `palette` for colours?

Dependent libraries:

-   `kas` is my reason for building this, but hopefully it will be useful for
    `iced` and other libraries (GUIs, games)
-   some library binding this with `wgpu_glyph` (probably this will be simple
    enough to embed in `kas_wgpu` and `iced_wgpu`)


Environments
---------------

Text needs to be displayed *somewhere*; this *environment* must specify at least
the following:

-   an axis-aligned box within which text is displayed
-   whether to line-wrap text
-   default alignment of text
-   the default font size
-   the default font colour
-   any extra data to forward to the rasteriser, such as depth value
    (note: rich text may influence colour but not depth value)

Note: we need to specify a type to use for dimensions. We could just use `f32`;
HarfBuzz uses `i32` but IIRC shifted to allow 6-bits of sub-integer precision.


Font management
-------------------

Potentially, text may use quite a few different fonts; we should therefore load
fonts on demand unless we heavily restrict the number available.

Input should be able to select:

-   a default font
-   a bold variant, italic variant, bold-italic variant
-   possibly also light and condensed variants
-   a monospace font, with applicable variants
-   possibly other families

For now, we can let font-kit can do the work for us and not make this
configurable (though later configuration support is a must).
Properties: https://docs.rs/font-kit/0.8.0/font_kit/properties/struct.Properties.html
Families: https://docs.rs/font-kit/0.8.0/font_kit/family_name/enum.FamilyName.html

### Font & property selection

Input should be able to select:

-   font family (including "default")
-   italic property
-   weight (possibly only binary: bold or normal)
-   underline (not font selection but external effect)
-   strikethrough?
-   foreground colour (default, a name like "blue", or arbitrary value)
-   background colour


Rich text
----------

Several properties of text will be derived from the display environment (see
above). Some properties may be specified for rich text:

-   font selection, but likely only by family and properties including italic,
    bold and monospace (possibly using the `font-kit` API)
-   font size, but possibly only relative to the default size (this side-steps
    all the scaling problems associated with usage of units like pt, mm and
    pixels); we may also allow users to force size limits
-   foreground colour: possibly with the option to select from a list of named
    colours (specified by the theme) and the option to specify an absolute
    colour (according to some fixed colour space)
-   alignment may be overridden at the paragraph level or with tabulation

Some types of flow-control should also be supported:

-   explicit line breaks
-   indentation (e.g. paragraph start, code blocks)
-   tables?
-   bullet points and enumeration?

### Translation from HTML

HTML and CSS allow font sizes to be specified in various units. For now we can
ignore this problem, but it may be necessary to make the input parser aware of
the display's DPI / physical size and/or scaling factor.


Challenges
--------------

### Line-splitting

Text may include explicit line breaks, but usually line-breaks are introduced
via line-wrapping. The Unicode BIDI algorithm is explicit about how this works:
https://www.unicode.org/reports/tr9/#Reordering_Resolved_Levels
To summarise:

1.  text is split into paragraphs
2.  paragraphs are split into runs with given embedding level by the BIDI algorithm
3.  a shaper is applied to the result and used to calculate word positions
4.  line-breaking occurs
5.  the BIDI algorithm re-orders characters

Until we implement BIDI support, we may use a simpler model:

1.  text is split into paragraphs
2.  a shaper is applied to the result and used to calculate word positions
3.  line-breaking occurs

To do this line-breaking, we need to know:

-   where within a text line breaks may occur (another Unicode specification)
-   which characters are whitespace (trailing whitespace never wraps)
-   the position at which each word ends

Line splitting may be a view transformation, excepting in multi-line text
editors where it must be explicit.

### Rich text

It must be possible to adjust certain properties within a sub-span:

-   font (e.g. for bold and italic)
-   font size and weight — maybe?
-   subtext / supertext — maybe?
-   colours (foreground and background)
-   external effects (underline, strikethrough)

In general, text shapers are not equipped to deal with these, thus all font
changes must start a new "run".

#### Syntax highlighting

This is definitely not a short-term goal, but it's worth considering how this
might work. Essentially, a highlighter is just another input parser. It needs to
be able to:

-   select a font (usually monospaced)
-   select variants: bold, italic, underlined, strikethrough(?)
-   select colours
-   select background colours

Simple highlighters might choose colours from a list of names like "green" and
"red", thus allowing theme-customisation but not a great degree of flexibility.
Complex highlighters may wish to choose from a wider palette of colours and
allow user-adjustment; these must recognise that "theme" and "highlighting
scheme" cannot truely be separated from one another. To be fully customisable,
users should be able to adjust bold/italic/underline properties of each named
style used by the highlighter as well as foreground and background colours,
and possibly also explicitly choose colours for selected text.

#### Text selection

This operates on top of other processors: it must be possible to transform
processed output to make a span appear selected.

### External effects

Underline, strikethrough and edit carets are not directly part of rasterised
text, but should be supported somehow (though for edit carets probably only by
reporting the coordinates and line-height where it should occur).

### Embedded objects

We would like to support embedding objects (e.g. check-boxes and icons) within
paragraph text. For now, lets assume that such objects have a fixed size
(possibly derived from the base line height but not from the specific line in
which they appear). The library must provide a trait to bound such objects.

### Bidirectional text support

We may have to re-implement the Unicode BIDI algorithm over our paragraph
representation in order to handle rich text and embedded objects.


Design
------

We need essentially three phases of transforms:

1.  Input markup to internal stable representation
2.  Space fitting: at least this must perform line wrapping; it may also
    perform shaping (into a list of positioned glyphs)
3.  Rastorisation (which may or may not do layout/shaping on the fly)

We also require an "internal stable representation". This must be a concrete
type that users of the API can store; it might be acceptable if this is only
exposed as an associated type on the input parser. It must support testing
required dimensions (with line-wrapping) and position look-ups.

Additionally, we require a font library.
This will initially be a small abstraction over `font-kit` to select fonts based
on properties and load into memory.
