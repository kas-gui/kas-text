// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display enviroment

use crate::Vec2;

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    /// Bidirectional text
    ///
    /// If enabled, right-to-left text embedded within left-to-right text and
    /// LTR within RTL will be re-ordered according to the Unicode Bidirectional
    /// Algorithm (Unicode Technical Report #9).
    ///
    /// If disabled, the base paragraph direction may be LTR or RTL depending on
    /// [`Environment::dir`], but embedded text is not re-ordered.
    ///
    /// Default value: `true`. This should normally be enabled unless there is
    /// a specific reason to disable it.
    pub bidi: bool,
    /// Default text direction
    ///
    /// Usually this may be left to its default value of [`Direction::Auto`].
    /// If `bidi == true`, this parameter sets the "paragraph embedding level"
    /// (whose main affect is on lines without strongly-directional characters).
    /// If `bidi == false` this directly sets the line direction, unless
    /// `dir == Auto`, in which case direction is auto-detected.
    pub dir: Direction,
    /// Pixels-per-point
    ///
    /// This is a scaling factor used to convert font sizes (in points) to a
    /// size in pixels (dots). Units are `pixels/point`. See [`crate::fonts`]
    /// documentation.
    ///
    /// Default value: `96.0 / 72.0`
    pub dpp: f32,
    /// Default font size in points
    ///
    /// We use "point sizes" (Points per Em), since this is a widely used
    /// measure of font size. See [`crate::fonts`] module documentation.
    ///
    /// Default value: `11.0`
    pub pt_size: f32,
    /// The available (horizontal and vertical) space
    ///
    /// This defaults to infinity (implying no bounds). To enable line-wrapping
    /// set at least a horizontal bound. The vertical bound is required for
    /// alignment (when aligning to the centre or bottom).
    /// Glyphs outside of these bounds may not be drawn.
    pub bounds: Vec2,
    /// Line wrapping
    ///
    /// By default, this is true and long text lines are wrapped based on the
    /// width bounds. If set to false, lines are not wrapped at the width
    /// boundary, but explicit line-breaks such as `\n` still result in new
    /// lines.
    pub wrap: bool,
    /// Horizontal alignment
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Environment::dir`]).
    pub halign: Align,
    /// Vertical alignment
    ///
    /// By default, vertical alignment is to-the-top.
    pub valign: Align,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            bidi: true,
            dir: Direction::default(),
            dpp: 96.0 / 72.0,
            pt_size: 11.0,
            bounds: Vec2::INFINITY,
            wrap: true,
            halign: Align::default(),
            valign: Align::default(),
        }
    }
}

impl Environment {
    /// Construct, with explicit font size
    pub fn new(dpp: f32, pt_size: f32) -> Self {
        Environment {
            dpp,
            pt_size,
            ..Default::default()
        }
    }
}

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent. For example, for Left-To-Right text it means
    /// `TL`; for things which want to stretch it may mean `Stretch`.
    Default,
    /// Align to top or left
    TL,
    /// Align to centre
    Centre,
    /// Align to bottom or right
    BR,
    /// Stretch to fill space
    ///
    /// For text, this is known as "justified alignment".
    Stretch,
}

impl Default for Align {
    fn default() -> Self {
        Align::Default
    }
}

/// Directionality of environment
///
/// This can be used to force the text direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Direction {
    /// Auto-detect (default)
    Auto,
    /// Left-to-Right
    LR,
    /// Right-to-Left
    RL,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Auto
    }
}
