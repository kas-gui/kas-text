// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — text-display environment

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

impl Direction {
    /// Is auto-detection enabled?
    #[inline]
    pub fn is_auto(self) -> bool {
        matches!(self, Direction::Auto | Direction::AutoRtl)
    }

    /// Is this a right-to-left mode (`Rtl` or `AutoRtl`)?
    #[inline]
    pub fn is_rtl(self) -> bool {
        matches!(self, Direction::AutoRtl | Direction::Rtl)
    }

    /// Get the base directionality of `text`
    ///
    /// If <code>self.[is_auto](Direction::is_auto)()</code>, this method uses
    /// function [`text_is_rtl`] to infer the text direction, falling back to
    /// [`Self::is_rtl`] on indeterminate texts.
    /// If <code>!self.[is_auto](Direction::is_auto)()</code>, this method
    /// simply returns <code>self.[is_rtl](Direction::is_rtl)()</code>.
    pub fn text_is_rtl(&self, text: &str) -> bool {
        if self.is_auto()
            && let Some(is_rtl) = text_is_rtl(text)
        {
            is_rtl
        } else {
            self.is_rtl()
        }
    }
}

/// Try to infer the base directionality of `text`
///
/// This method attempts to infer the base direction of `text` and returns
/// `Some(is_rtl)` if successful. Specifically, this searches `text` for the
/// first strongly-directional character (type `L`, `R` or `AL`) up to the end
/// of the first paragraph and uses this to infer LTR (`L`) or RTL (`R`, `AL`).
/// See [UAX#9: Bidirectional Character Types](https://www.unicode.org/reports/tr9/#Bidirectional_Character_Types).
pub fn text_is_rtl(text: &str) -> Option<bool> {
    match unicode_bidi::get_base_direction(text) {
        unicode_bidi::Direction::Ltr => Some(false),
        unicode_bidi::Direction::Rtl => Some(true),
        unicode_bidi::Direction::Mixed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_text_dir() {
        assert_eq!(text_is_rtl("abc"), Some(false));
        assert_eq!(text_is_rtl("אבג"), Some(true));
        assert_eq!(text_is_rtl(""), None);
        assert_eq!(text_is_rtl("123"), None);
        assert_eq!(text_is_rtl(" 2 - 2 = 0. "), None);
        assert_eq!(text_is_rtl("123 abc"), Some(false));
    }
}
