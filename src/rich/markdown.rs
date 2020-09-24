// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — parsing

use super::{FormatList, FormatSpecifier, Text};
use crate::fonts::{fonts, FontSelector, Style, Weight};
use pulldown_cmark::{Event, Parser, Tag};

// TODO: error handling
// TODO: reduce calls to load_font via caching?
pub(crate) fn parse(input: &str) -> Text {
    let mut text = String::with_capacity(input.len());
    let mut formatting = FormatList::default();

    let fonts = fonts();

    let mut stack = Vec::with_capacity(16);
    let mut item = StackItem::default();
    let mut first_para = true;

    // This is really just to ensure load_default gets called first:
    item.spec.font_id = Some(fonts.load_default().unwrap());

    // TODO: parser options — perhaps strikethrough?
    let mut parser = Parser::new(input);
    while let Some(ev) = parser.next() {
        dbg!(&ev);
        match ev {
            Event::Start(tag) => {
                item.spec.start = text.len() as u32;
                stack.push(item.clone());
                if item.tag(tag) {
                    if first_para {
                        first_para = false;
                    } else {
                        text.push_str("\n\n");
                    }
                }
                item.spec.font_id = Some(fonts.load_font(item.sel.clone()).unwrap());
                dbg!(&item.spec);
                formatting.set_last(item.spec);
            }
            Event::End(_) => {
                if let Some(x) = stack.pop() {
                    item = x;
                    item.spec.start = text.len() as u32;
                    dbg!(&item.spec);
                    formatting.set_last(item.spec);
                }
            }
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

#[derive(Clone, Debug, Default)]
struct StackItem {
    sel: FontSelector,
    spec: FormatSpecifier,
}

impl StackItem {
    // process a tag; return true on paragraph
    fn tag(&mut self, tag: Tag) -> bool {
        match tag {
            Tag::Paragraph => return true,
            Tag::Emphasis => self.sel.set_style(Style::Italic),
            Tag::Strong => self.sel.set_weight(Weight::BOLD),
            tag @ _ => unimplemented!("{:?}", tag),
        }
        false
    }
}
