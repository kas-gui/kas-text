// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display environment

use crate::Vec2;

/// Environment in which text is prepared for display
///
/// An `Environment` can be default-constructed (without line-wrapping).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// Alignment (`horiz`, `vert`)
    ///
    /// By default, horizontal alignment is left or right depending on the
    /// text direction (see [`Self::direction`]), and vertical alignment
    /// is to the top.
    pub align: (Align, Align),
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
            bounds: Vec2::INFINITY,
            align: Default::default(),
        }
    }
}

/// Alignment of contents
///
/// Note that alignment information is often passed as a `(horiz, vert)` pair.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Align {
    /// Default alignment
    ///
    /// This is context dependent. For example, for Left-To-Right text it means
    /// `TL`; for things which want to stretch it may mean `Stretch`.
    #[default]
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

/// Directionality of text
///
/// Texts may be bi-directional as specified by Unicode Technical Report #9.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Direction {
    /// Auto-detect direction
    ///
    /// The text direction is inferred from the first strongly-directional
    /// character. In case no such character is found, the text will be
    /// left-to-right.
    #[default]
    Auto = 2,
    /// Auto-detect, default right-to-left
    ///
    /// The text direction is inferred from the first strongly-directional
    /// character. In case no such character is found, the text will be
    /// right-to-left.
    AutoRtl = 3,
    /// The base text direction is left-to-right
    ///
    /// If the text contains right-to-left content, this will be considered an
    /// embedded right-to-left run. Non-directional leading and trailing
    /// characters (e.g. a full stop) will normally not be included within this
    /// right-to-left section.
    ///
    /// This uses Unicode TR9 HL1 to set an explicit paragraph embedding level of 0.
    Ltr = 0,
    /// The base text direction is right-to-left
    ///
    /// If the text contains left-to-right content, this will be considered an
    /// embedded left-to-right run. Non-directional leading and trailing
    /// characters (e.g. a full stop) will normally not be included within this
    /// left-to-right section.
    ///
    /// This uses Unicode TR9 HL1 to set an explicit paragraph embedding level of 1.
    Rtl = 1,
}
