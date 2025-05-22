// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Markdown parsing

use super::{EditableText, FontToken, FormattableText};
use crate::conv::to_u32;
use crate::fonts::{self, FontId, FontSelector, FontStyle, FontWeight, GenericFamily};
use crate::{Effect, EffectFlags};
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use std::fmt::Write;
use std::iter::FusedIterator;
use thiserror::Error;

/// Markdown parsing errors
#[derive(Error, Debug)]
pub enum Error {
    #[error("Not supported by Markdown parser: {0}")]
    NotSupported(&'static str),
}

/// Basic Markdown formatter
///
/// Currently this misses several important Markdown features, but may still
/// prove a convenient way of constructing formatted texts.
///
/// Supported:
///
/// -   Text paragraphs
/// -   Code (embedded and blocks); caveat: extra line after code blocks
/// -   Explicit line breaks
/// -   Headings
/// -   Lists (numerated and bulleted); caveat: indentation after first line
/// -   Bold, italic (emphasis), strike-through
///
/// Not supported:
///
/// -   Block quotes
/// -   Footnotes
/// -   HTML
/// -   Horizontal rules
/// -   Images
/// -   Links
/// -   Tables
/// -   Task lists
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Markdown {
    text: String,
    fmt: Vec<Fmt>,
    effects: Vec<Effect<()>>,
}

impl Markdown {
    /// Parse the input as Markdown
    ///
    /// Parsing happens immediately. Fonts must be initialized before calling
    /// this method.
    #[inline]
    pub fn new(input: &str) -> Result<Self, Error> {
        parse(input)
    }
}

pub struct FontTokenIter<'a> {
    index: usize,
    fmt: &'a [Fmt],
    base_dpem: f32,
}

impl<'a> FontTokenIter<'a> {
    fn new(fmt: &'a [Fmt], base_dpem: f32) -> Self {
        FontTokenIter {
            index: 0,
            fmt,
            base_dpem,
        }
    }
}

impl<'a> Iterator for FontTokenIter<'a> {
    type Item = FontToken;

    fn next(&mut self) -> Option<FontToken> {
        if self.index < self.fmt.len() {
            let fmt = &self.fmt[self.index];
            self.index += 1;
            Some(FontToken {
                start: fmt.start,
                font_id: fmt.font_id,
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
    type FontTokenIter<'a> = FontTokenIter<'a>;

    #[inline]
    fn as_str(&self) -> &str {
        &self.text
    }

    #[inline]
    fn font_tokens<'a>(&'a self, dpem: f32) -> Self::FontTokenIter<'a> {
        FontTokenIter::new(&self.fmt, dpem)
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

    fn replace_range(&mut self, range: std::ops::Range<usize>, replace_with: &str) {
        self.text.replace_range(range.clone(), replace_with);

        let start = to_u32(range.start);
        let end = to_u32(range.end);
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

fn parse(input: &str) -> Result<Markdown, Error> {
    let mut text = String::with_capacity(input.len());
    let mut fmt: Vec<Fmt> = Vec::new();
    let fonts = fonts::library();
    let mut set_last = |item: &StackItem| {
        let f = Fmt::new(fonts, item);
        if let Some(last) = fmt.last_mut() {
            if last.start >= item.start {
                *last = f;
                return;
            }
        }
        fmt.push(f);
    };

    let mut state = State::None;
    let mut stack = Vec::with_capacity(16);
    let mut item = StackItem::default();

    let options = pulldown_cmark::Options::ENABLE_STRIKETHROUGH;
    for ev in pulldown_cmark::Parser::new_ext(input, options) {
        match ev {
            Event::Start(tag) => {
                item.start = to_u32(text.len());
                if let Some(clone) = item.start_tag(&mut text, &mut state, tag)? {
                    stack.push(item);
                    item = clone;
                    set_last(&item);
                }
            }
            Event::End(tag) => {
                if item.end_tag(&mut state, tag) {
                    item = stack.pop().unwrap();
                    item.start = to_u32(text.len());
                    set_last(&item);
                }
            }
            Event::Text(part) => {
                state.part(&mut text);
                text.push_str(&part);
            }
            Event::Code(part) => {
                state.part(&mut text);
                item.start = to_u32(text.len());

                let mut item2 = item.clone();
                // NOTE: we shouldn't need to specify both of these but for now
                // the former does not imply the latter (see parley#323).
                item2
                    .sel
                    .set_families([GenericFamily::UiMonospace, GenericFamily::Monospace]);
                set_last(&item2);

                text.push_str(&part);

                item.start = to_u32(text.len());
                set_last(&item);
            }
            Event::InlineMath(_) | Event::DisplayMath(_) => {
                return Err(Error::NotSupported("math expressions"))
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                return Err(Error::NotSupported("embedded HTML"))
            }
            Event::FootnoteReference(_) => return Err(Error::NotSupported("footnote")),
            Event::SoftBreak => state.soft_break(&mut text),
            Event::HardBreak => state.hard_break(&mut text),
            Event::Rule => return Err(Error::NotSupported("horizontal rule")),
            Event::TaskListMarker(_) => return Err(Error::NotSupported("task list")),
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

    Ok(Markdown { text, fmt, effects })
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
            State::ListItem | State::Part => text.push('\n'),
        }
        *self = State::ListItem;
    }
    fn soft_break(&mut self, text: &mut String) {
        text.push(' ');
    }
    fn hard_break(&mut self, text: &mut String) {
        text.push('\n');
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fmt {
    start: u32,
    font_id: FontId,
    rel_size: f32,
    flags: EffectFlags,
}

impl Fmt {
    fn new(fonts: &fonts::FontLibrary, item: &StackItem) -> Self {
        Fmt {
            start: item.start,
            font_id: fonts.select_font(&item.sel).unwrap(),
            rel_size: item.rel_size,
            flags: item.flags,
        }
    }
}

#[derive(Clone, Debug)]
struct StackItem {
    list: Option<u64>,
    start: u32,
    sel: FontSelector,
    rel_size: f32,
    flags: EffectFlags,
}

impl Default for StackItem {
    fn default() -> Self {
        StackItem {
            list: None,
            start: 0,
            sel: Default::default(),
            rel_size: 1.0,
            flags: EffectFlags::empty(),
        }
    }
}

impl StackItem {
    // process a tag; may modify current item and may return new item
    fn start_tag(
        &mut self,
        text: &mut String,
        state: &mut State,
        tag: Tag,
    ) -> Result<Option<Self>, Error> {
        fn with_clone<F: Fn(&mut StackItem)>(s: &mut StackItem, c: F) -> Option<StackItem> {
            let mut item = s.clone();
            c(&mut item);
            Some(item)
        }

        Ok(match tag {
            Tag::Paragraph => {
                state.start_block(text);
                None
            }
            Tag::Heading { level, .. } => {
                state.start_block(text);
                self.start = to_u32(text.len());
                with_clone(self, |item| {
                    // CSS sizes: https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#font-size-prop
                    item.rel_size = match level {
                        HeadingLevel::H1 => 2.0 / 1.0,
                        HeadingLevel::H2 => 3.0 / 2.0,
                        HeadingLevel::H3 => 6.0 / 5.0,
                        HeadingLevel::H4 => 1.0,
                        HeadingLevel::H5 => 8.0 / 9.0,
                        HeadingLevel::H6 => 3.0 / 5.0,
                    }
                })
            }
            Tag::CodeBlock(_) => {
                state.start_block(text);
                self.start = to_u32(text.len());
                with_clone(self, |item| {
                    // NOTE: as above, we shouldn't need to specify both of these:
                    item.sel
                        .set_families([GenericFamily::UiMonospace, GenericFamily::Monospace])
                })
                // TODO: within a code block, the last \n should be suppressed?
            }
            Tag::HtmlBlock => return Err(Error::NotSupported("embedded HTML")),
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
                        write!(text, "{x}\t").unwrap();
                        *x += 1;
                    }
                    None => text.push_str("•\t"),
                }
                None
            }
            Tag::Emphasis => with_clone(self, |item| item.sel.set_style(FontStyle::Italic)),
            Tag::Strong => with_clone(self, |item| item.sel.set_weight(FontWeight::BOLD)),
            Tag::Strikethrough => with_clone(self, |item| {
                item.flags.set(EffectFlags::STRIKETHROUGH, true)
            }),
            Tag::BlockQuote(_) => return Err(Error::NotSupported("block quote")),
            Tag::FootnoteDefinition(_) => return Err(Error::NotSupported("footnote")),
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                return Err(Error::NotSupported("definition"))
            }
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                return Err(Error::NotSupported("table"))
            }
            Tag::Link { .. } => return Err(Error::NotSupported("link")),
            Tag::Image { .. } => return Err(Error::NotSupported("image")),
            Tag::MetadataBlock(_) => return Err(Error::NotSupported("metadata block")),
        })
    }
    // returns true if stack must be popped
    fn end_tag(&self, state: &mut State, tag: TagEnd) -> bool {
        match tag {
            TagEnd::Paragraph | TagEnd::List(_) => {
                state.end_block();
                false
            }
            TagEnd::Heading(_) | TagEnd::CodeBlock => {
                state.end_block();
                true
            }
            TagEnd::Item => false,
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => true,
            tag => unimplemented!("{:?}", tag),
        }
    }
}
