// Test serialization using json
#![cfg(feature = "serde")]

use kas_text::Vec2;
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
