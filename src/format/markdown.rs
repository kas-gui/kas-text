// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Markdown parsing

use super::{EditableText, FontToken, FormattableText};
use crate::conv::to_u32;
use crate::fonts::{self, FamilyName, FontId, FontSelector, Style, Weight};
#[cfg(not(feature = "gat"))]
use crate::OwningVecIter;
use crate::{Effect, EffectFlags};
use pulldown_cmark::{Event, Tag};
use std::iter::FusedIterator;

#[derive(Clone, Debug, PartialEq)]
pub struct Markdown {
    text: String,
    fmt: Vec<Fmt>,
    effects: Vec<Effect<()>>,
}

impl Markdown {
    #[inline]
    pub fn new(input: &str) -> Self {
        parse(input)
    }
}

pub struct FontTokenIter<'a> {
    index: usize,
    fmt: &'a [Fmt],
    fonts: &'a fonts::FontLibrary,
    font_id: FontId,
    font_sel: FontSelector,
    base_dpem: f32,
}

impl<'a> FontTokenIter<'a> {
    fn new(fmt: &'a [Fmt], base_dpem: f32) -> Self {
        FontTokenIter {
            index: 0,
            fmt,
            fonts: fonts::fonts(),
            font_id: FontId::default(),
            font_sel: FontSelector::default(),
            base_dpem,
        }
    }
}

impl<'a> Iterator for FontTokenIter<'a> {
    type Item = FontToken;

    fn next(&mut self) -> Option<FontToken> {
        if self.index < self.fmt.len() {
            let fmt = &self.fmt[self.index];
            if self.font_sel != fmt.sel {
                self.font_id = self.fonts.load_font(&fmt.sel).unwrap();
                self.font_sel.assign(&fmt.sel);
            }
            self.index += 1;
            Some(FontToken {
                start: fmt.start,
                font_id: self.font_id,
                dpem: self.base_dpem * fmt.rel_size,
            })
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.fmt.len();
        (len, Some(len))
    }
}

impl<'a> ExactSizeIterator for FontTokenIter<'a> {}
impl<'a> FusedIterator for FontTokenIter<'a> {}

impl FormattableText for Markdown {
    #[cfg(feature = "gat")]
    type FontTokenIter<'a> = FontTokenIter<'a>;

    #[inline]
    fn as_str(&self) -> &str {
        &self.text
    }

    #[cfg(feature = "gat")]
    #[inline]
    fn font_tokens<'a>(&'a self, dpp: f32, pt_size: f32) -> Self::FontTokenIter<'a> {
        FontTokenIter::new(&self.fmt, dpp * pt_size)
    }
    #[cfg(not(feature = "gat"))]
    #[inline]
    fn font_tokens(&self, dpp: f32, pt_size: f32) -> OwningVecIter<FontToken> {
        let iter = FontTokenIter::new(&self.fmt, dpp * pt_size);
        OwningVecIter::new(iter.collect())
    }

    fn effect_tokens(&self) -> &[Effect<()>] {
        &self.effects
    }
}

impl EditableText for Markdown {
    fn set_string(&mut self, string: String) {
        self.text = string;
        self.fmt.clear();
    }

    fn swap_string(&mut self, string: &mut String) {
        std::mem::swap(&mut self.text, string);
        self.fmt.clear();
    }

    fn insert_char(&mut self, index: usize, c: char) {
        self.text.insert(index, c);
        let start = to_u32(index);
        let len = to_u32(c.len_utf8());
        for fmt in &mut self.fmt {
            if fmt.start >= start {
                fmt.start += len;
            }
        }
    }

    fn replace_range(&mut self, start: usize, end: usize, replace_with: &str) {
        self.text.replace_range(start..end, replace_with);

        let start = to_u32(start);
        let end = to_u32(end);
        let len = end - start;
        let mut last = None;
        let mut i = 0;
        while i < self.str_len() {
            let fmt = &mut self.fmt[i];
            if fmt.start >= start {
                if fmt.start < end {
                    fmt.start = start;
                } else {
                    fmt.start -= len;
                }
                if let Some((index, start)) = last {
                    if start == fmt.start {
                        self.fmt.remove(index);
                        continue;
                    }
                }
                last = Some((i, fmt.start));
            }
            i += 1;
        }
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

    let options = pulldown_cmark::Options::ENABLE_STRIKETHROUGH;
    let mut parser = pulldown_cmark::Parser::new_ext(input, options);
    while let Some(ev) = parser.next() {
        match ev {
            Event::Start(tag) => {
                item.fmt.start = to_u32(text.len());
                if let Some(clone) = item.start_tag(&mut text, &mut state, tag) {
                    stack.push(item);
                    item = clone;
                    set_last(&item.fmt);
                }
            }
            Event::End(tag) => {
                if item.end_tag(&mut state, tag) {
                    item = stack.pop().unwrap();
                    item.fmt.start = to_u32(text.len());
                    set_last(&item.fmt);
                }
            }
            Event::Text(part) => {
                state.part(&mut text);
                text.push_str(&part);
            }
            Event::Code(part) => {
                state.part(&mut text);
                item.fmt.start = to_u32(text.len());

                let mut item2 = item.clone();
                item2.fmt.sel.set_families(vec![FamilyName::Monospace]);
                set_last(&item2.fmt);

                text.push_str(&part);

                item.fmt.start = to_u32(text.len());
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

    // TODO(opt): don't need to store flags in fmt?
    let mut effects = Vec::new();
    let mut flags = EffectFlags::default();
    for token in &fmt {
        if token.flags != flags {
            effects.push(Effect {
                start: token.start,
                flags: token.flags,
                aux: (),
            });
            flags = token.flags;
        }
    }

    Markdown { text, fmt, effects }
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
pub struct Fmt {
    start: u32,
    sel: FontSelector,
    rel_size: f32,
    flags: EffectFlags,
}

impl Default for Fmt {
    fn default() -> Self {
        Fmt {
            start: 0,
            sel: FontSelector::default(),
            rel_size: 1.0,
            flags: EffectFlags::empty(),
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
                self.fmt.start = to_u32(text.len());
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
                self.fmt.start = to_u32(text.len());
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
                // NOTE: we use \t for indent, which indents only the first
                // line. Without better flow control we cannot fix this.
                match &mut self.list {
                    Some(x) => {
                        text.push_str(&format!("{}\t", x));
                        *x = *x + 1;
                    }
                    None => text.push_str("•\t"),
                }
                None
            }
            Tag::Emphasis => with_clone(self, |item| item.fmt.sel.set_style(Style::Italic)),
            Tag::Strong => with_clone(self, |item| item.fmt.sel.set_weight(Weight::BOLD)),
            Tag::Strikethrough => with_clone(self, |item| {
                item.fmt.flags.set(EffectFlags::STRIKETHROUGH, true)
            }),
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
