// Test serialization using json
#![cfg(feature = "serde")]

use fontique::GenericFamily;
use kas_text::Vec2;
use kas_text::fonts::{FamilyName, FontStyle, FontWeight, FontWidth};
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

    test(FontWidth::CONDENSED, "192");
    test(FontWeight::BOLD, "700");
    test(FontStyle::Normal, "\"Normal\"");
    test(FontStyle::Oblique(None), "{\"Oblique\":null}");
    test(FontStyle::Oblique(Some(50)), "{\"Oblique\":50}");
}
