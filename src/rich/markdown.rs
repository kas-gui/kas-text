// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — parsing

use super::{Text, FormatList};
use pulldown_cmark::{Event, Parser};

pub(crate) fn parse(input: &str) -> Text {
    let mut text = String::with_capacity(input.len());
    let mut formatting = FormatList::default();
    
    // TODO: parser options — perhaps strikethrough?
    let mut parser = Parser::new(input);
    while let Some(item) = parser.next() {
        match item {
            Event::Start(tag) => unimplemented!("{:?}", tag),
            Event::End(tag) => unimplemented!("{:?}", tag),
            Event::Text(part) => text.push_str(&part),
            Event::Code(part) => unimplemented!("{:?}", part),
            Event::Html(part) => unimplemented!("{:?}", part),
            Event::FootnoteReference(part) => unimplemented!("{:?}", part),
            Event::SoftBreak => (),
            Event::HardBreak => (),
            Event::Rule => unimplemented!(),
            Event::TaskListMarker(checked) => unimplemented!("{:?}", checked),
        }
    }
    
    Text { text, formatting }
}
