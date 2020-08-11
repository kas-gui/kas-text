// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Models of rich text in abstract from an environment

/// A rich text representation
///
/// This format may be used to input and share text, but does not include
/// details specific to the presentation or presentation environment.
///
/// TODO: this is very much incomplete
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Text {
    /// The raw text
    ///
    /// This is a contiguous version of the text to be presented, without
    /// (non-unicode) formatting details. Note that Unicode control characters
    /// may be present, e.g. U+2029 (paragraph separator) and explicit
    /// directional formatting characters.
    pub text: String,
    // TODO: add formatting over the `text`
}

impl Text {
    /// The length of all concatenated runs
    pub fn len(&self) -> usize {
        self.text.len()
    }
}

/// Generate an unformatted `String` from the concatenation of all runs
impl ToString for Text {
    fn to_string(&self) -> String {
        self.text.clone()
    }
}

impl From<Text> for String {
    fn from(text: Text) -> String {
        text.text
    }
}

impl From<String> for Text {
    fn from(text: String) -> Text {
        Text { text }
    }
}

impl<'a> From<&'a str> for Text {
    fn from(text: &'a str) -> Text {
        Text::from(text.to_string())
    }
}
