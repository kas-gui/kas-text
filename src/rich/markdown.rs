// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! KAS Rich-Text library — parsing

use super::{FormatList, FormatSpecifier, Text};
use crate::fonts::{fonts, FamilyName, FontSelector, Style, Weight};
use pulldown_cmark::{Event, Parser, Tag};

// TODO: error handling
// TODO: reduce calls to load_font via caching?
pub(crate) fn parse(input: &str) -> Text {
    let mut text = String::with_capacity(input.len());
    let mut formatting = FormatList::default();

    let fonts = fonts();

    let mut line = Line::None;
    let mut stack = Vec::with_capacity(16);
    let mut item = StackItem::default();

    // This is really just to ensure load_default gets called first:
    item.spec.font_id = Some(fonts.load_default().unwrap());

    // TODO: parser options — perhaps strikethrough?
    let mut parser = Parser::new(input);
    while let Some(ev) = parser.next() {
        match ev {
            Event::Start(tag) => {
                item.spec.start = text.len() as u32;
                if let Some(clone) = item.start_tag(&mut text, &mut line, tag) {
                    stack.push(item);
                    item = clone;
                    item.spec.font_id = Some(fonts.load_font(item.sel.clone()).unwrap());
                    formatting.set_last(item.spec);
                }
            }
            Event::End(tag) => {
                if item.end_tag(&mut line, tag) {
                    item = stack.pop().unwrap();
                    item.spec.start = text.len() as u32;
                    formatting.set_last(item.spec);
                }
            }
            Event::Text(part) => text.push_str(&part),
            Event::Code(part) => {
                item.spec.start = text.len() as u32;
                let mut item2 = item.clone();
                item2.sel.set_families(vec![FamilyName::Monospace]);
                item2.spec.font_id = Some(fonts.load_font(item2.sel).unwrap());
                formatting.set_last(item2.spec);

                text.push_str(&part);

                item.spec.start = text.len() as u32;
                formatting.set_last(item.spec);
            }
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

// Line state
enum Line {
    None,
    Para,
    Item,
}

impl Line {
    fn paragraph(&mut self) -> &str {
        let ret = match self {
            Line::None => "",
            _ => "\n\n",
        };
        *self = Line::Para;
        ret
    }
    fn heading(&mut self) -> &str {
        self.paragraph()
    }
    fn item(&mut self) -> &str {
        let ret = match self {
            Line::None => "",
            Line::Para => "\n\n",
            _ => "\n",
        };
        *self = Line::Item;
        ret
    }
}

// TODO: this is temporary
const BASE_SIZE: f32 = 11.0;

#[derive(Clone, Debug)]
struct StackItem {
    sel: FontSelector,
    list: Option<u64>,
    spec: FormatSpecifier,
}

impl Default for StackItem {
    fn default() -> Self {
        StackItem {
            sel: Default::default(),
            list: None,
            spec: FormatSpecifier {
                pt_size: BASE_SIZE,
                ..Default::default()
            },
        }
    }
}

impl StackItem {
    // process a tag; may modify current item and may return new item
    fn start_tag(&mut self, text: &mut String, line: &mut Line, tag: Tag) -> Option<Self> {
        fn with_clone<F: Fn(&mut StackItem)>(s: &mut StackItem, c: F) -> Option<StackItem> {
            let mut item = s.clone();
            c(&mut item);
            Some(item)
        }

        match tag {
            Tag::Paragraph => {
                text.push_str(line.paragraph());
                None
            }
            Tag::Heading(level) => {
                text.push_str(line.heading());
                self.spec.start = text.len() as u32;
                with_clone(self, |item| {
                    item.spec.pt_size = match level {
                        1 => 2.0 * BASE_SIZE,
                        2 => 1.75 * BASE_SIZE,
                        3 => 1.5 * BASE_SIZE,
                        4 => 1.35 * BASE_SIZE,
                        5 => 1.2 * BASE_SIZE,
                        level => panic!("Heading({}) not supported", level),
                    }
                })
            }
            Tag::CodeBlock(_) => {
                text.push_str(line.item());
                self.spec.start = text.len() as u32;
                with_clone(self, |item| {
                    item.sel.set_families(vec![FamilyName::Monospace])
                })
                // TODO: within a code block, the last \n should be suppressed?
            }
            Tag::List(start) => {
                // TODO: a list is not a "line item", but should have extra space?
                text.push_str(line.item());
                self.list = start;
                None
            }
            Tag::Item => {
                text.push_str(line.item());
                match &mut self.list {
                    // TODO: indent properly
                    Some(x) => {
                        text.push_str(&format!("{:<4}", x));
                        *x = *x + 1;
                    }
                    None => text.push_str("•   "),
                }
                None
            }
            Tag::Emphasis => with_clone(self, |item| item.sel.set_style(Style::Italic)),
            Tag::Strong => with_clone(self, |item| item.sel.set_weight(Weight::BOLD)),
            tag @ _ => unimplemented!("{:?}", tag),
        }
    }
    // returns true if stack must be popped
    fn end_tag(&self, line: &mut Line, tag: Tag) -> bool {
        match tag {
            Tag::Paragraph => {
                *line = Line::Para;
                false
            }
            Tag::Heading(_) => true,
            Tag::CodeBlock(_) => true,
            Tag::List(_) => false,
            Tag::Item => false,
            Tag::Emphasis => true,
            Tag::Strong => true,
            tag @ _ => unimplemented!("{:?}", tag),
        }
    }
}
