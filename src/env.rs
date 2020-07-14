// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display enviroment

use crate::{prepared::Action, FontScale, Vec2};

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Environment {
    /// Base font scale
    ///
    /// Fonts are sized relative to this scale. `FontScale` has a default size
    /// suitable for common usage.
    pub font_scale: FontScale,
    /// Default text direction
    ///
    /// Note: right-to-left text can still occur in a left-to-right environment
    /// and vice-versa, however the default alignment direction can affect the
    /// layout of complex texts.
    pub dir: Direction,
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
    /// The available (horizontal and vertical) space
    ///
    /// This defaults to zero but should normally be positive in usage.
    /// If line-wrapping is enabled, it is controlled by this width.
    /// Glyphs outside of these bounds may not be drawn.
    pub bounds: Vec2,
}

impl Environment {
    /// Alternative default constructor
    ///
    /// Has left-to-right direction, default alignment, no line-wrapping,
    /// default font [`FontScale`] and zero-sized bounds.
    pub fn new() -> Self {
        Self::default()
    }

    /// Default construct but with line-wrapping
    ///
    /// This is identical to [`Environment::new`] except that line-wrapping is
    /// enabled.
    pub fn new_wrap() -> Self {
        let mut env = Self::new();
        env.wrap = true;
        env
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

    /// Set base font scale
    pub fn set_font_scale(&mut self, scale: FontScale) {
        if scale != self.env.font_scale {
            self.env.font_scale = scale;
            self.action = Action::Shape;
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
    pub fn set_align(&mut self, horiz: Align, vert: Align) {
        if horiz != self.env.halign || vert != self.env.valign {
            self.env.halign = horiz;
            self.env.valign = vert;
            self.action = self.action.max(Action::Align);
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
/// TODO: support RL and possibly vertical directions.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum Direction {
    /// Left-to-Right (default)
    LR,
    // /// Right-to-Left
    // RL,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::LR
    }
}
