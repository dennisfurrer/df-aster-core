//! `encodeURIComponent`, byte for byte.

use super::*;

#[test]
fn encodes_as_encode_uri_component() {
    assert_eq!(encode_component("BTCUSDT"), "BTCUSDT");
    assert_eq!(encode_component("a b&c=d"), "a%20b%26c%3Dd");
    assert_eq!(encode_component("-_.!~*'()"), "-_.!~*'()");
    assert_eq!(encode_component("[{\"x\":1}]"), "%5B%7B%22x%22%3A1%7D%5D");
    assert_eq!(encode_component("龙虾USDT"), "%E9%BE%99%E8%99%BEUSDT");
    assert_eq!(encode_component("/:,+"), "%2F%3A%2C%2B");
}

#[test]
fn query_keeps_the_order_given() {
    let fields = [("symbol", "BTCUSDT".to_owned()), ("side", "BUY".to_owned()), ("price", "1.5".to_owned())];
    assert_eq!(query(&fields), "symbol=BTCUSDT&side=BUY&price=1.5");
    assert_eq!(query(&[]), "");
}
