// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Models of text in abstract from an environment

use crate::Range;
use smallvec::SmallVec;

/// A rich text representation
///
/// This format may be used to input and share text, but is not ready for
/// presentation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Text {
    /// The raw text
    ///
    /// It is not necessary for the whole text to be displayed: this may include
    /// extra data such as markup. See [`Text::runs`].
    pub text: String,

    /// A "run" of text
    ///
    /// Each run corresponds to a range within the raw text ([`Text::text`])
    /// and may apply formatting. The text displayed is the result of
    /// concatenating each run. If no runs are present, no text is displayed.
    pub runs: SmallVec<[Run; 1]>,
}

impl From<String> for Text {
    fn from(text: String) -> Text {
        let range = (0..text.len()).into();
        let run = Run { range };
        Text {
            text,
            runs: std::iter::once(run).collect(),
        }
    }
}

impl<'a> From<&'a str> for Text {
    fn from(text: &'a str) -> Text {
        Text::from(text.to_string())
    }
}

/// A "run" of formatted text
///
/// This is used to apply formatting within a [`Text`] object.
/// See also documentation of [`Text::runs`].
///
/// TODO: include formatting (font and font effect selectors, line breaks,
/// paragraph breaks).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Run {
    /// The range within the raw text
    ///
    /// It is an error if this range exceeds that of the raw text.
    /// Implementations may choose to restrict to the raw range or may choose
    /// to generate an explicit error (via panic or return value).
    ///
    /// Although unusual, it is acceptable for ranges of multiple runs to
    /// overlap or occur out-of-order.
    pub range: Range,
}
