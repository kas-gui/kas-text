// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! Allsorts bindings

use allsorts::binary::read::ReadScope;
use allsorts::error::{ParseError, ShapingError};
use allsorts::font_data_impl::read_cmap_subtable;
use allsorts::gpos::{gpos_apply, Info};
use allsorts::gsub::{gsub_apply_default, GlyphOrigin, GsubFeatureMask, RawGlyph};
use allsorts::layout::{new_layout_cache, GDEFTable, LayoutTable, GPOS, GSUB};
use allsorts::tables::cmap::{Cmap, CmapSubtable};
use allsorts::tables::{MaxpTable, OffsetTable, OpenTypeFile, OpenTypeFont};
use allsorts::tag;
use allsorts::tinyvec::tiny_vec;
use allsorts::unicode::VariationSelector;
use std::convert::TryFrom;

use crate::fonts::{fonts, FontError, FontId};
use crate::{prepared, shaper};

#[cfg(feature = "allsorts")]
pub(crate) struct AllsortsFont<'a> {
    pub(crate) cmap: Cmap<'a>,
    pub(crate) cmap_subtable: CmapSubtable<'a>,
    pub(crate) maxp: MaxpTable,
    opt_gsub_table: Option<LayoutTable<GSUB>>,
    opt_gdef_table: Option<GDEFTable>,
    opt_gpos_table: Option<LayoutTable<GPOS>>,
}

#[cfg(feature = "allsorts")]
impl<'a> AllsortsFont<'a> {
    pub(crate) fn try_from_bytes(data: &'a [u8], index: u32) -> Result<Self, FontError> {
        let fontfile = ReadScope::new(&data).read::<OpenTypeFile>()?;
        let scope = &fontfile.scope;
        let ttf = match fontfile.font {
            OpenTypeFont::Single(ttf) => {
                if index != 0 {
                    return Err(FontError::InvalidIndex);
                }
                ttf
            }
            OpenTypeFont::Collection(ttc) => {
                if index as usize >= ttc.offset_tables.len() {
                    return Err(FontError::InvalidIndex);
                }
                let offset = ttc.offset_tables.read_item(index as usize)?;
                scope.offset(offset as usize).read::<OffsetTable>()?
            }
        };

        let cmap = if let Some(cmap_scope) = ttf.read_table(scope, tag::CMAP)? {
            cmap_scope.read::<Cmap>()?
        } else {
            return Err(FontError::NoCmap);
        };
        let (_, cmap_subtable) = if let Some(cmap_subtable) = read_cmap_subtable(&cmap)? {
            cmap_subtable
        } else {
            return Err(FontError::NoCmapSubtable);
        };
        let maxp = if let Some(maxp_scope) = ttf.read_table(scope, tag::MAXP)? {
            maxp_scope.read::<MaxpTable>()?
        } else {
            return Err(FontError::NoMaxp);
        };

        let mut opt_gsub_table = None;
        if let Some(gsub_record) = ttf.find_table_record(tag::GSUB) {
            opt_gsub_table = Some(gsub_record.read_table(scope)?.read::<LayoutTable<GSUB>>()?);
        }

        let opt_gdef_table = match ttf.find_table_record(tag::GDEF) {
            Some(gdef_record) => Some(gdef_record.read_table(scope)?.read::<GDEFTable>()?),
            None => None,
        };

        let opt_gpos_table = match ttf.find_table_record(tag::GPOS) {
            Some(gpos_record) => {
                let gpos_table = gpos_record.read_table(scope)?.read::<LayoutTable<GPOS>>()?;
                Some(gpos_table)
            }
            None => None,
        };

        Ok(AllsortsFont {
            cmap,
            cmap_subtable,
            maxp,
            opt_gsub_table,
            opt_gdef_table,
            opt_gpos_table,
        })
    }
}

// from https://github.com/yeslogic/allsorts-tools/blob/master/src/glyph.rs
mod glyph {
    use super::*;

    pub(crate) fn map(
        cmap_subtable: &CmapSubtable,
        ch: char,
        variation: Option<VariationSelector>,
    ) -> Result<Option<RawGlyph<()>>, ParseError> {
        if let Some(glyph_index) = cmap_subtable.map_glyph(ch as u32)? {
            let glyph = make(ch, glyph_index, variation);
            Ok(Some(glyph))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn make(
        ch: char,
        glyph_index: u16,
        variation: Option<VariationSelector>,
    ) -> RawGlyph<()> {
        RawGlyph {
            unicodes: tiny_vec![[char; 1], ch],
            glyph_index: Some(glyph_index),
            liga_component_pos: 0,
            glyph_origin: GlyphOrigin::Char(ch),
            small_caps: false,
            multi_subst_dup: false,
            is_vert_alt: false,
            fake_bold: false,
            fake_italic: false,
            extra_data: (),
            variation,
        }
    }
}

// largely from https://github.com/yeslogic/allsorts-tools/blob/master/src/shape.rs
pub(crate) fn shape(
    font_id: FontId,
    dpem: f32,
    text: &str,
    run: &prepared::Run,
) -> Result<shaper::ShapeResult, ShapingError> {
    let font = fonts().get_allsorts(font_id);
    let (script, lang) = (0, 0); // FIXME

    let mut chars_iter = text.chars().peekable();
    let mut opt_glyphs = Vec::with_capacity(text.len());
    while let Some(ch) = chars_iter.next() {
        match VariationSelector::try_from(ch) {
            Ok(_) => {} // filter out variation selectors
            Err(()) => {
                let vs = chars_iter
                    .peek()
                    .and_then(|&next| VariationSelector::try_from(next).ok());
                let glyph = glyph::map(&font.cmap_subtable, ch, vs)?;
                opt_glyphs.push(glyph);
            }
        }
    }

    let mut glyphs = opt_glyphs.into_iter().flatten().collect();

    if let Some(gsub_table) = font.opt_gsub_table {
        // TODO: gsub_cache should persist in thread-local storage?
        // It cannot be in static memory due to usage of Rc!
        // Same for gpos_cache below.
        let gsub_cache = new_layout_cache::<GSUB>(gsub_table);
        let opt_gdef_table = font.opt_gdef_table.as_ref();

        gsub_apply_default(
            &|| make_dotted_circle(&font.cmap_subtable),
            &gsub_cache,
            opt_gdef_table,
            script,
            lang,
            GsubFeatureMask::default(),
            font.maxp.num_glyphs,
            &mut glyphs,
        )?;
        match font.opt_gpos_table {
            Some(gpos_table) => {
                let gpos_cache = new_layout_cache::<GPOS>(gpos_table);
                let kerning = true;
                let mut infos = Info::init_from_glyphs(opt_gdef_table, glyphs)?;
                gpos_apply(
                    &gpos_cache,
                    opt_gdef_table,
                    kerning,
                    script,
                    lang,
                    &mut infos,
                )?;
            }
            None => {}
        }
    }

    // FIXME:
    let glyphs = glyphs
        .into_iter()
        .map(|glyph| shaper::Glyph {
            index: (),
            id: ab_glyph::GlyphId(glyph.glyph_index.unwrap()),
            position: (),
        })
        .collect();
    let breaks = ();
    let no_space_end = ();
    let caret = ();

    Ok((glyphs, breaks, no_space_end, caret))
}

fn make_dotted_circle(cmap_subtable: &CmapSubtable) -> Vec<RawGlyph<()>> {
    match glyph::map(cmap_subtable, '\u{25cc}', None) {
        Ok(Some(raw_glyph)) => vec![raw_glyph],
        _ => Vec::new(),
    }
}
