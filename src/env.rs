// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display enviroment

use crate::{fonts, prepared::Action, FontId, Vec2};

// doc imports
#[allow(unused)]
use crate::FontScale;

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    /// The available (horizontal and vertical) space
    ///
    /// This defaults to infinity (implying no bounds).
    /// If line-wrapping is enabled, it is controlled by this width.
    /// Glyphs outside of these bounds may not be drawn.
    pub bounds: Vec2,
    /// Pixels-per-point
    ///
    /// This is a scaling factor used to convert font sizes (in points) to a
    /// size in pixels (dots). Units are `pixels/point`.
    ///
    /// Since "72 pt = 1 in" and the "standard" DPI is 96, calculate as:
    /// ```none
    /// dpp = dpi / 72 = scale_factor * (96 / 72)
    /// ```
    ///
    /// Note that most systems allow the user to adjust the "font DPI" or set a
    /// scaling factor, thus this value may not correspond to the real pixel
    /// density of the display.
    pub dpp: f32,
    /// Default font size in points
    ///
    /// This is stored in units of pt/em partly because this is a standard unit
    /// and partly because it allows fonts to scale with DPI.
    pub pt_size: f32,
    /// Default text direction
    ///
    /// Note: right-to-left text can still occur in a left-to-right environment
    /// and vice-versa, however the default alignment direction can affect the
    /// layout of complex texts.
    pub dir: Direction,
    /// Bidirectional text
    ///
    /// If enabled, right-to-left text embedded within left-to-right text and
    /// RTL within LTR will be re-ordered correctly (up to the maximum embedding
    /// level defined by Unicode Technical Report #9).
    ///
    /// If disabled, the base paragraph direction may be LTR or RTL depending on
    /// [`Environment::dir`], but embedded text is not re-ordered.
    ///
    /// Normally this should be enabled, but within text-editors it might be
    /// disabled (at user's preference).
    pub bidi: bool,
    /// Horizontal alignment
    pub halign: Align,
    /// Vertical alignment
    pub valign: Align,
    /// Line wrapping
    ///
    /// If true, text is wrapped at word boundaries to fit within the available
    /// width (regardless of height). If false, only explicit line-breaks such
    /// as `\n` result in new lines.
    pub wrap: bool,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            dpp: 96.0 / 72.0,
            pt_size: 11.0,
            dir: Direction::default(),
            bidi: true,
            halign: Align::default(),
            valign: Align::default(),
            wrap: true,
            bounds: Vec2::INFINITY,
        }
    }
}

impl Environment {
    /// Alternative default constructor
    ///
    /// Has left-to-right direction, default alignment, no line-wrapping,
    /// default font [`FontScale`] and zero-sized bounds.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the height of a standard line
    ///
    /// This depends on the `pt_size` and `dpp` fields.
    ///
    /// To use "the standard font", use `Default::default()`.
    pub fn line_height(&self, font_id: FontId) -> f32 {
        let dpem = self.pt_size * self.dpp;
        fonts().get(font_id).line_height(dpem)
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

    /// Set DPP
    ///
    /// Units are pixels/point (see [`Environment::dpp`]).
    pub fn set_dpp(&mut self, dpp: f32) {
        if dpp != self.env.dpp {
            self.env.dpp = dpp;
            self.action = Action::Dpem;
        }
    }

    /// Set default font size in points
    ///
    /// Units are points/em (see [`Environment::pt_size`]).
    pub fn set_pt_size(&mut self, pt_size: f32) {
        if pt_size != self.env.pt_size {
            self.env.pt_size = pt_size;
            self.action = Action::Dpem;
        }
    }

    /// Set the default direction
    pub fn set_dir(&mut self, dir: Direction) {
        if dir != self.env.dir {
            self.env.dir = dir;
            self.action = Action::Runs;
        }
    }

    /// Set the alignment
    ///
    /// Takes `(horiz_align, vert_align)` tuple to allow easier parameter passing.
    pub fn set_align(&mut self, (horiz, vert): (Align, Align)) {
        if horiz != self.env.halign || vert != self.env.valign {
            self.env.halign = horiz;
            self.env.valign = vert;
            self.action = self.action.max(Action::Wrap);
        }
    }

    /// Enable or disable line-wrapping
    pub fn set_wrap(&mut self, wrap: bool) {
        if wrap != self.env.wrap {
            self.env.wrap = wrap;
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

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent: for things that want to stretch it means
    /// stretch, for things which don't it means align-to-start (e.g. to
    /// top-left for left-to-right text).
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
