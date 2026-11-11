//! Bare JSON numbers read exactly, in plain or scientific spelling.

use super::*;

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[derive(serde::Deserialize)]
struct Holder {
    #[serde(deserialize_with = "exact")]
    value: Dec,
}

#[test]
fn bare_numbers_read_as_their_decimal() {
    for (json, value) in [("0.003", "0.003"), ("-0.02", "-0.02"), ("3", "3"), ("\"0.5\"", "0.5"), ("1e-7", "0.0000001")]
    {
        let held: Holder = serde_json::from_str(&format!("{{\"value\":{json}}}")).unwrap();
        assert_eq!(held.value, d(value), "{json}");
    }
    assert_eq!(scientific("2.5E3"), Some(d("2500")));
    assert_eq!(scientific("-1.5e-2"), Some(d("-0.015")));
    assert_eq!(scientific("nonsense"), None);
    assert!(serde_json::from_str::<Holder>("{\"value\":true}").is_err());
}
