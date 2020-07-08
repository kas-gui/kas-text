// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Models of text in abstract from an environment

/// A rich text representation
///
/// This format may be used to input and share text, but is not ready for
/// presentation.
#[derive(Clone, Default)]
pub struct Text {
    // TODO: API needs to include paragraphs, line breaks, etc.
    pub parts: Vec<TextPart>,
}

impl From<String> for Text {
    fn from(text: String) -> Text {
        Text {
            parts: vec![TextPart { text }],
        }
    }
}

impl<'a> From<&'a str> for Text {
    fn from(text: &'a str) -> Text {
        let text = text.to_string();
        Text {
            parts: vec![TextPart { text }],
        }
    }
}

#[derive(Clone, Default)]
pub struct TextPart {
    // TODO: API needs to include font and colour selectors
    pub text: String,
}
