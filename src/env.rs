// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display environment

use crate::fonts::{fonts, FontId};
use crate::{Action, Vec2};

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    /// Flags enabling/disabling certain features
    ///
    /// ## Bidirectional text
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
    ///
    /// ## Line wrapping
    ///
    /// By default, this is true and long text lines are wrapped based on the
    /// width bounds. If set to false, lines are not wrapped at the width
    /// boundary, but explicit line-breaks such as `\n` still result in new
    /// lines.
    pub flags: EnvFlags,
    /// Default text direction
    ///
    /// Usually this may be left to its default value of [`Direction::Auto`].
    /// If `bidi == true`, this parameter sets the "paragraph embedding level"
    /// (whose main affect is on lines without strongly-directional characters).
    /// If `bidi == false` this directly sets the line direction, unless
    /// `dir == Auto`, in which case direction is auto-detected.
    pub dir: Direction,
    /// Alignment (`horiz`, `vert`)
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Environment::dir`]), and vertical alignment is
    /// to the top.
    pub align: (Align, Align),
    /// Default font
    ///
    /// This font is used unless a formatting token (see [`crate::format`])
    /// is used.
    pub font_id: FontId,
    /// Font size: pixels per Em
    ///
    /// This is a scaling factor used to convert font sizes, with units
    /// `pixels/Em`. Equivalently, this is the line-height in pixels.
    /// See [`crate::fonts`] documentation.
    ///
    /// To calculate this from text size in Points, use `dpem = dpp * pt_size`
    /// where the dots-per-point is usually `dpp = scale_factor * 96.0 / 72.0`
    /// on PC platforms, or `dpp = 1` on MacOS (or 2 for retina displays).
    pub dpem: f32,
    /// The available (horizontal and vertical) space
    ///
    /// This defaults to infinity (implying no bounds). To enable line-wrapping
    /// set at least a horizontal bound. The vertical bound is required for
    /// alignment (when aligning to the center or bottom).
    /// Glyphs outside of these bounds may not be drawn.
    pub bounds: Vec2,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            flags: Default::default(),
            dir: Direction::default(),
            font_id: Default::default(),
            dpem: 11.0 * 96.0 / 72.0,
            bounds: Vec2::INFINITY,
            align: Default::default(),
        }
    }
}

impl Environment {
    /// Returns the height of horizontal text
    ///
    /// This should be similar to the value of [`Self::dpem`], but depends on
    /// the font.
    ///
    /// To use "the standard font", use `font_id = Default::default()`.
    pub fn line_height(&self, font_id: FontId) -> f32 {
        fonts().get_first_face(font_id).height(self.dpem)
    }
}

/// Helper to modify an environment
#[derive(Debug)]
pub struct UpdateEnv<'a> {
    env: &'a mut Environment,
    action: Action,
}

impl<'a> UpdateEnv<'a> {
    pub(crate) fn new(env: &'a mut Environment) -> Self {
        let action = Action::None;
        UpdateEnv { env, action }
    }

    pub(crate) fn finish(self) -> Action {
        self.action
    }

    /// Read access to the environment
    pub fn env(&self) -> &Environment {
        self.env
    }

    /// Set font
    pub fn set_font_id(&mut self, font_id: FontId) {
        if font_id != self.env.font_id {
            self.env.font_id = font_id;
            self.action = Action::All;
        }
    }

    /// Set font size (pixels per Em)
    pub fn set_dpem(&mut self, dpem: f32) {
        if dpem != self.env.dpem {
            self.env.dpem = dpem;
            self.action = Action::Resize;
        }
    }

    /// Set the default direction
    pub fn set_dir(&mut self, dir: Direction) {
        if dir != self.env.dir {
            self.env.dir = dir;
            self.action = Action::All;
        }
    }

    /// Set the alignment
    ///
    /// Takes `(horiz, vert)` tuple to allow easier parameter passing.
    pub fn set_align(&mut self, align: (Align, Align)) {
        if align != self.env.align {
            self.env.align = align;
            self.action = self.action.max(Action::Wrap);
        }
    }

    /// Enable or disable line-wrapping
    pub fn set_wrap(&mut self, wrap: bool) {
        if wrap != self.env.flags.contains(EnvFlags::WRAP) {
            self.env.flags.set(EnvFlags::WRAP, wrap);
            self.action = self.action.max(Action::Wrap);
        }
    }

    /// Set the environment's bounds
    pub fn set_bounds(&mut self, bounds: Vec2) {
        if bounds != self.env.bounds {
            // Note (opt): if we had separate align and wrap actions, then we
            // would only need to do alignment provided:
            // self.width_required <= bounds.0.min(self.env.bounds.0)
            // This may not be worth pursuing however.
            self.env.bounds = bounds;
            self.action = self.action.max(Action::Wrap);
        }
    }
}

impl Environment {
    /// Construct, with explicit font size (pixels per Em)
    pub fn new(dpem: f32) -> Self {
        Environment {
            dpem,
            ..Default::default()
        }
    }
}

bitflags::bitflags! {
    /// Environment flags
    ///
    /// By default, all flags are enabled
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct EnvFlags: u8 {
        /// Enable bidirectional text support
        const BIDI = 1 << 0;
        /// Enable line wrapping
        const WRAP = 1 << 1;
        /// Vertically align to the nearest pixel
        ///
        /// This is highly recommended to avoid rendering artifacts at small
        /// pixel sizes. Affect on layout is negligible.
        const PX_VALIGN = 1 << 2;
    }
}

impl Default for EnvFlags {
    fn default() -> Self {
        EnvFlags::all()
    }
}

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent. For example, for Left-To-Right text it means
    /// `TL`; for things which want to stretch it may mean `Stretch`.
    Default,
    /// Align to top or left
    TL,
    /// Align to center
    Center,
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

#[test]
fn size() {
    assert_eq!(std::mem::size_of::<Environment>(), 20);
}
