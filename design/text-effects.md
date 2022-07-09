Text effects
========

These include:

-   underline, strikethrough
-   background colour (highlighting)
-   foreground (glyph) colour, sRGB
-   colour, by category (e.g. URL)
-   edit cursor (?)


Requirements
--------

The renderer wants two things:

1.  a list of positioned glyphs, with colour (or other, e.g. "section index")
2.  a list of rectangles, with colour (and order)

Kas-text wants to be able to process things efficiently, which means:

-   rare stuff such as text-selection should be via separate methods
-   glyph positioning without effects should be fast
-   glyph positioning with effects should not need to loop over glyphs twice


API
---

Currently we have `positioned_glyphs`, `highlight_lines` and `highlight_runs`.

Arguably, `highlight_runs` is a text effect (it produces rectangles around glyph
runs), while `highlight_lines` is not (its rectangles may cover whole lines).

We wish to have a rich-text representation which includes text effects:
underline, foreground colour, etc.; this might be stored just like font effects
(size and font face) but might be stored separately (since it is only used
later, and since effects could be provided at draw-time by the user).

Perhaps we should have:
```rust
impl Text {
    /// Returns a Vec of positioned glyphs, ignoring all effects
    pub fn glyphs<F: FnMut(FontId, f32, f32, Glyph)>(&self, mut f: F) {
        ..
    }
    
    /// Returns a Vec of positioned glyphs and a Vec of effects
    pub fn glyphs_and_effects<F: Fn(...) -> G>(&self) -> (Vec<G>, Vec<Drawable>) {
        if self.text_effects.is_empty() {
            (self.glyphs(f), vec![])
        } else {
            self.glyphs_with_effects_impl(&self.text_effects, f)
        }
    }
    
    /// Same as `glyphs_and_effects` but applies extra text-effects on-the-fly
    pub fn glyphs_with_effects<U, F>(&self, effects: &[Effect<U>], mut f: F) -> Vec<Drawable>
    where
        U: Copy + Default,
        F: FnMut(FontId, f32, f32, Glyph, U),
    {
        if !self.text_effects.is_empty() {
            effects = combine(&self.text_effects, effects);
        }
        self.glyphs_with_effects_impl(effects, f)
    }
}
```


Internal representation
-----------------

We have roughly:
```rust
struct Effect {
    start: u32,
    colour: Srgb,
    flags: EffectFlags,
}
```
... but we have additional requirements:

1.  Ability to apply to LTR and RTL — with current algorithm this implies that
    each item must set all effect state
2.  Ability to combine effects provided by formatting with effects provided at
    the draw stage: e.g. syntax highlighting + selection

### Late-provided effects

Cases:

1.  Underline accelerator keys: this *could* instead be baked into the formated
    text item, with a late modifier affecting visibility.
2.  Syntax highlighting: normally this will be provided by the parser (early).
3.  Selection: this changes colour of foreground and background, *but* these
    colours may depend on highlighting state (select from a matrix: category and
    whether selected).

### Classes

So what we *really* want is:

-   Input text in whatever format
-   Parser constructs contiguous text plus a sequence of class selectors
-   At run-breaking time, a resolver translates classes to fonts + sizes
-   At draw time, a resolver translates classes plus additional state to
    underline/strikethrough options + colour

Further, we should remove dpp & pt_size from Environment and provide to the
resolver at run-breaking time, maybe?

### Sources

The text source may provide highlighting information. The parser converts this
into class information.

The display environment provides bounds, direction, bidi switch, alignment,
wrap option, and optionally sizing (maybe relative).

The theme provides font selection, DPP and default font size.
Additionally, the theme provides the class resolver?

The draw call provides "late effects": selection range, option to indicate
accelerator keys.


Parser
-----

The parser turns some input (HTML, Markdown, ...) into a contiguous text and
formatting tokens. Each token has a start index and a class information, the
latter of which may contain:

-   size specification: dpem, pt_size, rel_size or default
-   direct formatting flags: bold, italic, underline, superscript, ...
-   standard indirect formatting flags: emphasis, strong, url(?)
-   custom formatting flags: accel label
-   highlighting class (enumeration)
-   direct colour information as sRGB or ...

We now have some choices to make about how we represent this:

1.  Somewhat limited custom binary. Have fields: `flags: u32, size: f32, aux: u32`
    and use flags to indicate what `size` means (none, dpem, pt_size, rel_size),
    and what `aux` means (none, sRGB, highlighting class tag, ...). Some bits in
    `flags` are reserved for usage downstream (0 if unused).
2.  Make `Text` generic over a `Class` type, of part of it.

Either way, for ease of usage by parsers, the class should support `Clone` and
should make it easy to set/unset flags and size/other class information.


Resolver
---------

This should be provided by the theme. Data components are:

-   theme colours and styles plus DPP and font size
-   parser classes (perhaps the parser provides a default style for each, but
    the resolver may override?)
-   text data with formatting provided by the parser
-   draw-time information: whether to highlight accelerator labels, selection
    range (the exact data may be determined by the theme)

Jobs are:

1.  Convert parser output to font face and size during run-breaking.
    Optionally this step could do some extra work as input to the next step.
2.  Convert parser output and/or run-break output plus draw state to auxilliary
    draw information (underlines, colour).

Use-cases:

-   plain-text: theme provides font and size and colours, including selection
    colour
-   `AccelString`: font and size as above; parser controls underlining with
    state provided at draw time
-   Markdown: theme provides base font and size; parser controls formatting
-   HTML: same; additionally, the parser may select font family and colour
-   Syntax highlighting using something like `syntect` with internal theme
    support: default font and font-size come from the KAS theme; the
    highlighting theme must be selected before resolving any font properties;
    at most the KAS theme can provide a path
-   Syntax highlighting using something like Kate: as above except that this
    also provides class names for each highlighting class and expects some
    colours to be provided by an external source; each class may have different
    highlighting colour (but this may still be provided by the theme)

Implications of the above:

-   the font resolver is defined by the parser and expects `FontId` and size
    to be provided; it might also benefit from an extension mechanism allowing
    the theme to provide parser-specific data (such as syntax highlighter info)
-   the effect resolver is again defined by the parser and may use data stored
    by the font resolver; additionally it wants custom draw state and may
    benefit from a theme extension mechanism

Design:
```rust
/// Trait object provided by downstream; may include default impl
// TODO: separate ThemeFont and ThemeAux?
pub trait Theme {
    /// Auxilliary data associated with glyphs; typically used for colour
    type Aux: Clone;

    /// Allow downcast as an extension mechanism
    fn as_any(&self) -> &Any;
    
    fn default_font(&self) -> FontId;
    fn select_font(&self, selector: &FontSelector) -> FontId;
    
    /// Provides default font size as `(dpp, pt_size)`
    fn font_size(&self) -> (f32, f32);
    
    /// Provides default instance of `Aux` type
    fn default_aux(&self) -> Aux;
    /// Provides `Aux` instance
    fn select_aux(&self, selector: AuxSelector) -> Aux;
}

#[non_exhaustive]
pub enum AuxSelector {
    Default,
    Selected, // selected text
}

/// Trait impls provided by rich-text parsers, both provided and downstream impls
pub trait FormattableText: std::fmt::Debug {
    type FontTokenIter<'a>: Iterator<Item = FontToken>;
    /// Storage available
    type Storage: Clone + Default + Debug;
    /// State used to generate effect tokens
    type DrawState: ?Sized;
    /// Effect tokens as generated by this object
    type EffectToken;

    fn clone_boxed(&self) -> Box<dyn FormattableText>;
    fn str_len(&self) -> usize;
    fn as_str(&self) -> &str;
    
    /// Updates `storage` and provides an iterator yielding `FontToken` items
    /// 
    /// This is used during run-breaking. It initialises `storage` for usage by
    /// `effect_tokens`.
    fn font_tokens<'a>(&'a self, storage: &mut Self::Storage, dpp: f32, pt_size: f32)
        -> Self::FontTokenIter<'a>;
    
    /// Provides an iterator yielding `EffectToken` items
    ///
    /// This is run during drawing. It uses `storage` which will have been
    /// initialised by `font_tokens`
    /// 
    /// This method generates a sequence of effect tokens of type
    /// `Self::EffectToken` which the passed closure then translates to type
    /// `EffectToken<Aux>`; additionally, the closure may force generation of
    /// additional tokens at the given index.
    fn effect_tokens<'a, Aux>(&'a self,
        storage: &mut Self::Storage,
        state: &DrawState,
        translate: &mut FnMut(u32, Self::EffectToken) -> (u32, EffectToken<Aux>),
    ) -> Vec<EffectToken<Aux>>;
    fn effect_tokens<'a, Aux>(&'a self,
        storage: &mut Self::Storage,
        state: &DrawState,
    ) -> Vec<Self::EffectToken>;
}

struct FontToken {
    start: u32,
    face: FaceId,
    dpem: f32,
    // And maybe this, for consumption by draw-stage resolver:
    tag: u32,
}

struct EffectToken<X> {
    start: u32,
    flags: EffectFlags,
    aux: X,
}
```

`Text::prepare` can wrap the above for users not requiring an object-safe
interface and able to provide a `&dyn Theme` object — i.e. widgets, with
`SizeHandle` or some such providing the `Theme` trait-object.
(Do prepare in `set_size`?)

KAS's themes can wrap this functionality for known text types with `SizeHandle`
and `DrawHandle` methods (e.g. `text_string`) while providing lower-level
interfaces for use with other types (`text_display`).

### Parser-theme interaction

We have several types of data:

-   input to parser
-   parser output: contiguous text + custom formatting tokens/spans
-   font formatting
-   class properties (underline/strikethrough, colour / highlighting class)
-   underline/strikethrough flags
-   colour property

Processing of this data looks like:

-   parser converts input as early as possible
-   during run-breaking, parser iterates over its spans, yielding a sequence of
    tokens encapsulating font properties + class information
-   during draw glyph-collection, parser class tokens are converted to flags
    and colour information, also using theme/draw input

Question 1: why give parser an internal format for spans/whatever *and* make it
yield a sequence of font+class tokens during run-breaking? Mostly to allow
copying rich text (e.g. to clipboard).

Question 2: how are class tokens transformed to standardised flags and colour
information specific to the renderer? It seems we have few options:

1.  the parser yields fixed colour information which the renderer interprets directly
2.  as above, but the theme translates colour information between parser and renderer
3.  the theme understands the parser's data format and does all translation

Note that consistent syntax highlighting can work in two ways: (a) the syntax
highlighter assigns colours directly (roughly 1 above) or (b) the highlighter
yields class names which the theme matches to colours (3).


Text object
------------

Fields:

-   environment data
-   contiguous text
-   formatting data
-   derived stuff (runs, lines, glyphs)

Methods:

-   constructors
-   clone/extract formatted text
-   read contiguous text
-   edit contiguous text, adjusting formatting
-   read/update environment
-   prepare: line-break, wrap-lines
-   read derived data (size requirement, lines, position translation, ...)

Field usage:

-   environment: constructors, read/update environment, prepare, highlight_lines
-   contiguous text: constructors, text read/edit fns, prepare (line-break)
-   formatting data: constructors, clone/edit text fns, prepare (line-break)
-   derived stuff: constructors (default only), prepare, read fns

Future: derived formatting tokens may be touched by text edit.

Text object usage through non-generic references (dyn or fixed type):

-   read/update env
-   prepare
-   read derived

Lesson: we don't need text/formatting data here, except in a form available for calling `prepare`.


### Design derived from above

Change Text type to:
```rust
struct Text<T: TextRep> {
    text: T,
    derived: TextDerived,
}

struct TextDerived {
    // env and all derived
}

trait TextDerivedApi {
    // all read-env and read-derived fns
}

trait TextApi: TextDerivedApi {
    // update env, prepare
    // read text (fns independent of T)
    // access as &TextDerived
}
```

Then, use as follows:

-   widgets use base `Text` type, which allows direct access to the text repr
    and impls all methods from both traits
-   handles use `&dyn TextApi` or `&dyn TextDerivedApi` or `&TextDerived`;
    they can derive the latter from the former
-   small caveat: users need to import `TextApi`

Alternative: as above, but without `TextDerivedApi`; all its methods get impl'd
on `Text` and `TextDerived`. Saves users a trait import.


### Draw state type

**Problem:** drawing text requires passing `DrawState`, which is dependent on
a type parameter. Derived formatting tokens are also dependent on this type
parameter. This implies that both `prepare_runs` and `glyphs_with_effects` are
dependent on this type parameter.

Partial solution: store derived formatting tokens in `Text`, not `TextDisplay`.
This may require extracting `prepare_runs` from `TextDisplay`.

`TextDisplay::glyphs_and_effects` requires an iterator (with random access?)
over translated formatting tokens. The widget provides the draw-state used in
this translation and the theme provides the translation machinery: this may mean
that a separate `DrawHandle` method is required for each type of text object
supported, though only a single `DrawText` method is needed.

This translation could be solved a few ways:

1.  First, construct a new Vec/VLA containing translated effect tokens,
    in order. This may include extra tokens for selection range start/end.
    Second, the `&TextDisplay` and this slice are passed to the draw method.
2.  Construct some type of iterator which produces elements of the above slice,
    in order, on request. This saves an allocation (which would be unnecessary
    with VLAs anyway) but requires more indirection.

Note that if we take the first approach, `glyphs_and_effects` does not need to
yield colour information: it simply includes an index into the slice with each
glyph. This may have other problems however: forcing the widget or theme code
to construct a second slice of colour information.

Errata
------

Either:

-   have separate HasStr and HasString classes; only the latter supports setting
    content (or possibly the former supports set_static_str)
-   do not support Label<&'static str>;
    Label::new ctor only for main text types so that `Label::new("abc")` works
    since we do not have specialization yet

Note 1: inefficiency of copying `&str` to `String` everywhere is irrelevant
Note 2: typically usage is with `String` or `&'a str` (which we cannot support)
due to translation layer.

Errata
------

Complex Markdown or HTML documents can have things like horizontal rules,
tables and text boxes. This is *not* simply text formatting and attempting to
support this in a "text" object feels like madness (better to use widgets).

Conclusion: true MD/HTML parsers should be able to build *widget trees*.
OTOH being able to construct formatted text from MD/HTML is convenient so
keeping a limited parser makes sense.
