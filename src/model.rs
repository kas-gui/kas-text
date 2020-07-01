// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — models of text in abstract from an environment

/// Text, in abstract from an environment
#[derive(Clone, Default)]
pub struct RichText {
    // TODO: API needs to include paragraphs, line breaks, etc.
    pub parts: Vec<TextPart>,
}

impl From<String> for RichText {
    fn from(text: String) -> RichText {
        RichText {
            parts: vec![TextPart { text }],
        }
    }
}

impl<'a> From<&'a str> for RichText {
    fn from(text: &'a str) -> RichText {
        let text = text.to_string();
        RichText {
            parts: vec![TextPart { text }],
        }
    }
}

#[derive(Clone, Default)]
pub struct TextPart {
    // TODO: API needs to include font and colour selectors
    pub text: String,
}
