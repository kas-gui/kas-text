// Test serialization using json
#![cfg(feature = "serde")]

use fontique::GenericFamily;
use kas_text::Vec2;
use kas_text::fonts::{FamilyName, FamilySelector, FontSelector, FontStyle, FontWeight, FontWidth};
use serde::{de::Deserialize, ser::Serialize};
use std::cmp::PartialEq;
use std::fmt::Debug;

fn test<X: Debug + PartialEq + Serialize + for<'a> Deserialize<'a>>(x: X, t: &str) {
    match serde_json::to_string(&x) {
        Ok(text) => assert_eq!(text, t),
        Err(err) => panic!("Ser of '{x:?}' failed: {err}"),
    }

    match serde_json::from_str::<X>(t) {
        Ok(v) => assert_eq!(v, x),
        Err(err) => panic!("Deser of '{t}' failed: {err}"),
    }
}

#[test]
fn vec2() {
    test(Vec2(1.0, 2.0), "[1.0,2.0]");
}

#[test]
fn font() {
    test(FamilyName::Named("abc".to_string()), "{\"Named\":\"abc\"}");
    test(
        FamilyName::Generic(GenericFamily::Cursive),
        "{\"Generic\":\"Cursive\"}",
    );

    test(FamilySelector::MATH, "\"math\"");
    test(FamilySelector::SYSTEM_UI, "\"system-ui\"");

    test(FontWidth::CONDENSED, "\"condensed\"");
    test(FontWidth::from_percentage(25.0), "\"25%\"");

    test(FontWeight::BOLD, "\"bold\"");
    test(FontWeight::new(300), "\"300\"");

    test(FontStyle::Normal, "\"normal\"");
    test(FontStyle::Oblique(None), "\"oblique\"");
    test(FontStyle::Oblique(Some(5120)), "\"oblique 20deg\"");

    test(FontSelector::default(), "\"system-ui\"");
    test(
        FontSelector::from(FamilySelector::FANG_SONG),
        "\"fangsong\"",
    );
    test(
        FontSelector {
            family: FamilySelector::SANS_SERIF,
            width: FontWidth::EXPANDED,
            weight: FontWeight::BOLD,
            style: FontStyle::Italic,
        },
        "\"italic bold expanded sans-serif\"",
    );
    test(
        FontSelector {
            family: FamilySelector::MONOSPACE,
            width: FontWidth::from_percentage(175.0),
            weight: FontWeight::MEDIUM,
            style: FontStyle::from_degrees(10.0),
        },
        "\"oblique 10deg 500 175% monospace\"",
    );
    test(
        FontSelector {
            family: FamilySelector::CURSIVE,
            weight: FontWeight::LIGHT,
            ..Default::default()
        },
        "\"300 cursive\"",
    );
}
