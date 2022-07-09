Complex text layout
===========

KAS-text now has partial support for Markdown formatting: bold and italic emphasis, headings (font size), and strikethrough (in progress). Indentation is supported via `\t`, but this is only suitable for indenting the first line of a wrapped paragraph.

This leaves several things poorly or not supported:

-   vertical spacing between paragraphs/headings/blocks must currently be a whole number of lines
-   horizontal rules are not supported
-   list items with wrapped text do not indent subsequent lines
-   block quotes do not use a different colour (although arguably this would be possible without extra kas-text functionality)
-   code samples do not support coloured background boxes
-   tables are not supported
-   embedded images are not supported

Potentially kas-text could be extended with enough text-layout features to support all the above (except tables) without too much trouble, but is this the right way to go? Full support for Markdown is not a goal for text; rather Markdown is merely a convenient way to input (some) formatted text. Beyond that we have HTML and perhaps other forms of rich text, and potentially yet more layout features (HTML is after all quite flexible).

Also to consider is how to make kas-text scalable to larger text documents: support for updating individual paragraphs/other parts of a text (potentially plus support for only partially laying out large texts), or alternately facilitating use of a separate `TextDisplay` object for each paragraph. Or... perhaps this is simply all beyond the sensible scope of the project?


Text sizing
------------

Our sizing approach:

1.  We check horizontal size requirements (may be a small or may be very large with a full paragraph).
    Small improvement: start with a width limit; as soon as this is exceeded stop doing further
    text layout. Caveat: long texts of many short lines will still be processed (unless we also
    have a small height limit, e.g. 1 pixel, during this step).
2.  We check vertical size requirements given a horizontal size.
3.  We prepare for drawing, hopefully reusing the layout from the previous step.

### Partial sizing

Potentially we can take a few shortcuts somewhere:

-   given a column of text widgets, we have a lower-bound on width (initially
    zero, updated for each row processed) and an upper-bound (the point at
    which wrapping is forced, or perhaps one derived from the window size);
    if these meet then we know there is no need to check further lines
-   we do not need to layout lines not currently visible, except to compute
    height for scrollbar sizing, but we may assume a minimum height of one
    line for each row and beyond a certain total height the scrollbar size is
    unaffected anyway (although relative position is still affected)

Alternatively we might simply use arbitrary limits on the number of lines we
check when sizing a column (e.g. 20 rows) and make assumptions about the rest
based on these; this potentially has caveats however (e.g. if somehow the entire
contents are visible, size will be incorrect). Perhaps when sizing the widget
should be given an upper bound on the size?
