//! Decimals the venue writes as JSON numbers, fractions and exponents among them, read exactly.

use super::*;

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[derive(serde::Deserialize)]
struct Row {
    #[serde(deserialize_with = "number_dec")]
    value: Dec,
    #[serde(default, deserialize_with = "maybe_number_dec")]
    maybe: Option<Dec>,
}

fn row(json: &str) -> Row {
    serde_json::from_str(json).unwrap()
}

#[test]
fn numbers_fractions_exponents_and_strings_read_exactly() {
    assert_eq!(row(r#"{"value":0.0065}"#).value, d("0.0065"));
    assert_eq!(row(r#"{"value":10000}"#).value, d("10000"));
    assert_eq!(row(r#"{"value":1e-7}"#).value, d("0.0000001"));
    assert_eq!(row(r#"{"value":2.5E3}"#).value, d("2500"));
    assert_eq!(row(r#"{"value":"-1.25"}"#).value, d("-1.25"));
    assert_eq!(row(r#"{"value":0.1,"maybe":""}"#).maybe, None);
    assert_eq!(row(r#"{"value":0.1,"maybe":null}"#).maybe, None);
    assert_eq!(row(r#"{"value":0.1,"maybe":"-1"}"#).maybe, Some(d("-1")));
    assert!(serde_json::from_str::<Row>(r#"{"value":true}"#).is_err());
    assert!(serde_json::from_str::<Row>(r#"{"value":"1e999"}"#).is_err());
}

#[test]
fn addresses_list_as_one_field() {
    let a: crate::utils::Address = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed".parse().unwrap();
    let b: crate::utils::Address = "0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359".parse().unwrap();
    assert_eq!(address_list(&[]), None);
    assert_eq!(
        address_list(&[a, b]).as_deref(),
        Some("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed,0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359")
    );
}
