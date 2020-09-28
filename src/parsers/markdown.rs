// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Markdown parsing

use crate::fonts::{self, FamilyName, FontId, FontSelector, Style, Weight};
use crate::text::{EditableText, Format, Text};
use crate::Environment;
use pulldown_cmark::{Event, Parser, Tag};

#[derive(Clone, Debug, PartialEq)]
pub struct Markdown {
    text: String,
    fmt: Vec<Fmt>,
}

impl Markdown {
    #[inline]
    pub fn new(input: &str) -> Self {
        parse(input)
    }
}

pub struct FormatIter<'a> {
    index: usize,
    fmt: &'a [Fmt],
    fonts: &'a fonts::FontLibrary,
    font_id: FontId,
    font_sel: FontSelector,
    base_dpem: f32,
}

impl<'a> FormatIter<'a> {
    fn new(fmt: &'a [Fmt], env: &Environment) -> Self {
        FormatIter {
            index: 0,
            fmt,
            fonts: fonts::fonts(),
            font_id: FontId::default(),
            font_sel: FontSelector::default(),
            base_dpem: env.dpp * env.pt_size,
        }
    }
}

impl<'a> Iterator for FormatIter<'a> {
    type Item = Format;

    fn next(&mut self) -> Option<Format> {
        if self.index < self.fmt.len() {
            let fmt = &self.fmt[self.index];
            if self.font_sel != fmt.sel {
                self.font_id = self.fonts.load_font(fmt.sel.clone()).unwrap();
                self.font_sel.assign(&fmt.sel);
            }
            self.index += 1;
            Some(Format {
                start: fmt.start,
                font_id: self.font_id,
                dpem: self.base_dpem * fmt.rel_size,
            })
        } else {
            None
        }
    }
}

impl Text for Markdown {
    #[cfg(not(feature = "gat"))]
    type FmtIter = FormatIter<'static>;
    #[cfg(feature = "gat")]
    type FmtIter<'a> = FormatIter<'a>;

    /// Access whole text as contiguous `str`
    #[inline]
    fn as_str(&self) -> &str {
        &self.text
    }

    #[cfg(not(feature = "gat"))]
    unsafe fn fmt_iter<'a>(&'a self, env: &'a Environment) -> FormatIter<'static> {
        let fmt = fonts::extend_lifetime(&self.fmt);
        FormatIter::new(fmt, env)
    }
    #[cfg(feature = "gat")]
    fn fmt_iter<'a>(&'a self, env: &'a Environment) -> FormatIter<'a> {
        FormatIter::new(&self.fmt, env)
    }
}

impl EditableText for Markdown {
    fn insert_char(&mut self, index: usize, c: char) {
        self.text.insert(index, c);

        let index = index as u32;
        let len = c.len_utf8() as u32;
        for fmt in &mut self.fmt {
            if fmt.start >= index {
                fmt.start += len;
            }
        }
    }

    fn replace_range<R>(&mut self, range: R, replace_with: &str)
    where
        R: std::ops::RangeBounds<usize> + std::iter::ExactSizeIterator + Clone,
    {
        self.text.replace_range(range.clone(), replace_with);

        use std::ops::Bound;
        let start = match range.start_bound() {
            Bound::Included(b) => *b as u32,
            Bound::Excluded(_) => unreachable!(),
            Bound::Unbounded => 0,
        };
        let src_len = range.len() as u32;
        let dst_len = replace_with.len() as u32;
        let old_end = start + src_len;
        let new_end = start + dst_len;

        let diff = new_end.wrapping_sub(old_end);
        let mut last = None;
        let mut i = 0;
        while i < self.fmt.len() {
            let fmt = &mut self.fmt[i];
            if fmt.start >= start {
                if fmt.start < old_end {
                    fmt.start = new_end;
                } else {
                    // wrapping_add effectively allows subtraction despite unsigned type
                    fmt.start = fmt.start.wrapping_add(diff);
                }
                if let Some((index, start)) = last {
                    if start == fmt.start {
                        self.fmt.remove(index as usize);
                        continue;
                    }
                }
                last = Some((i, fmt.start));
            }
            i += 1;
        }
    }

    fn swap_string(&mut self, string: &mut String) {
        std::mem::swap(&mut self.text, string);
        self.fmt.clear();
    }
}

fn parse(input: &str) -> Markdown {
    let mut text = String::with_capacity(input.len());
    let mut fmt: Vec<Fmt> = Vec::new();
    let mut set_last = |item: &Fmt| {
        if let Some(last) = fmt.last_mut() {
            if last.start >= item.start {
                *last = item.clone();
                return;
            }
        }
        fmt.push(item.clone());
    };

    let mut state = State::None;
    let mut stack = Vec::with_capacity(16);
    let mut item = StackItem::default();

    // TODO: parser options — perhaps strikethrough?
    let mut parser = Parser::new(input);
    while let Some(ev) = parser.next() {
        match ev {
            Event::Start(tag) => {
                item.fmt.start = text.len() as u32;
                if let Some(clone) = item.start_tag(&mut text, &mut state, tag) {
                    stack.push(item);
                    item = clone;
                    set_last(&item.fmt);
                }
            }
            Event::End(tag) => {
                if item.end_tag(&mut state, tag) {
                    item = stack.pop().unwrap();
                    item.fmt.start = text.len() as u32;
                    set_last(&item.fmt);
                }
            }
            Event::Text(part) => {
                state.part(&mut text);
                text.push_str(&part);
            }
            Event::Code(part) => {
                state.part(&mut text);
                item.fmt.start = text.len() as u32;

                let mut item2 = item.clone();
                item2.fmt.sel.set_families(vec![FamilyName::Monospace]);
                set_last(&item2.fmt);

                text.push_str(&part);

                item.fmt.start = text.len() as u32;
                set_last(&item.fmt);
            }
            Event::Html(part) => unimplemented!("{:?}", part),
            Event::FootnoteReference(part) => unimplemented!("{:?}", part),
            Event::SoftBreak => (),
            Event::HardBreak => (),
            Event::Rule => unimplemented!(),
            Event::TaskListMarker(checked) => unimplemented!("{:?}", checked),
        }
    }

    Markdown { text, fmt }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum State {
    None,
    BlockStart,
    BlockEnd,
    ListItem,
    Part,
}

impl State {
    fn start_block(&mut self, text: &mut String) {
        match *self {
            State::None | State::BlockStart => (),
            State::BlockEnd | State::ListItem | State::Part => text.push_str("\n\n"),
        }
        *self = State::BlockStart;
    }
    fn end_block(&mut self) {
        *self = State::BlockEnd;
    }
    fn part(&mut self, text: &mut String) {
        match *self {
            State::None | State::BlockStart | State::Part | State::ListItem => (),
            State::BlockEnd => text.push_str("\n\n"),
        }
        *self = State::Part;
    }
    fn list_item(&mut self, text: &mut String) {
        match *self {
            State::None | State::BlockStart | State::BlockEnd => {
                debug_assert_eq!(*self, State::BlockStart);
            }
            State::ListItem | State::Part => text.push_str("\n"),
        }
        *self = State::ListItem;
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Fmt {
    start: u32,
    sel: FontSelector,
    rel_size: f32,
}

impl Default for Fmt {
    fn default() -> Self {
        Fmt {
            start: 0,
            sel: FontSelector::default(),
            rel_size: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct StackItem {
    list: Option<u64>,
    fmt: Fmt,
}

impl StackItem {
    // process a tag; may modify current item and may return new item
    fn start_tag(&mut self, text: &mut String, state: &mut State, tag: Tag) -> Option<Self> {
        fn with_clone<F: Fn(&mut StackItem)>(s: &mut StackItem, c: F) -> Option<StackItem> {
            let mut item = s.clone();
            c(&mut item);
            Some(item)
        }

        match tag {
            Tag::Paragraph => {
                state.start_block(text);
                None
            }
            Tag::Heading(level) => {
                state.start_block(text);
                self.fmt.start = text.len() as u32;
                with_clone(self, |item| {
                    item.fmt.rel_size = match level {
                        1 => 2.0,
                        2 => 1.75,
                        3 => 1.5,
                        4 => 1.35,
                        5 => 1.2,
                        _ => panic!("Heading level > 5 not supported"),
                    }
                })
            }
            Tag::CodeBlock(_) => {
                state.start_block(text);
                self.fmt.start = text.len() as u32;
                with_clone(self, |item| {
                    item.fmt.sel.set_families(vec![FamilyName::Monospace])
                })
                // TODO: within a code block, the last \n should be suppressed?
            }
            Tag::List(start) => {
                state.start_block(text);
                self.list = start;
                None
            }
            Tag::Item => {
                state.list_item(text);
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
            Tag::Emphasis => with_clone(self, |item| item.fmt.sel.set_style(Style::Italic)),
            Tag::Strong => with_clone(self, |item| item.fmt.sel.set_weight(Weight::BOLD)),
            tag @ _ => unimplemented!("{:?}", tag),
        }
    }
    // returns true if stack must be popped
    fn end_tag(&self, state: &mut State, tag: Tag) -> bool {
        match tag {
            Tag::Paragraph | Tag::List(_) => {
                state.end_block();
                false
            }
            Tag::Heading(_) | Tag::CodeBlock(_) => {
                state.end_block();
                true
            }
            Tag::Item => false,
            Tag::Emphasis | Tag::Strong => true,
            tag @ _ => unimplemented!("{:?}", tag),
        }
    }
}
