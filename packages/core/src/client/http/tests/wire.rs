//! What counts as changing the account, for the `offline` build.

use super::*;

#[test]
fn a_signed_post_put_or_delete_changes_the_account_and_a_signed_get_does_not() {
    let signed = "symbol=BTCUSDT&nonce=1&signature=0xab".to_owned();
    assert!(Request::form(Method::Post, "https://x/fapi/v3/order", signed.clone()).is_signed());
    assert!(Request::form(Method::Put, "https://x/fapi/v3/order", signed.clone()).is_signed());
    assert!(Request::new(Method::Delete, format!("https://x/fapi/v3/order?{signed}")).is_signed());
    assert!(Request::new(Method::Post, format!("https://x/fapi/v3/approveAgent?{signed}")).is_signed());
    assert!(!Request::get(format!("https://x/fapi/v3/balance?{signed}")).is_signed());
    assert!(!Request::form(Method::Post, "https://x/fapi/v3/listenKey", "nosignature=1".to_owned()).is_signed());
    assert!(!Request::post_json("https://x/info", "{}".to_owned()).is_signed());
}

#[test]
fn headers_read_whatever_their_case() {
    let response = Response {
        status: 200,
        headers: vec![
            ("X-MBX-USED-WEIGHT-1M".to_owned(), " 42 ".to_owned()),
            ("Retry-After".to_owned(), "3".to_owned()),
        ],
        body: b"{}".to_vec(),
    };
    assert_eq!((response.used_weight(), response.retry_after_ms()), (Some(42), Some(3_000)));
    assert!(response.is_success());
}
