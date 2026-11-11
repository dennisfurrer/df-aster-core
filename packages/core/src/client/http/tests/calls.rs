//! Calls built in order, answers read, and refusals told apart by code.

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn a_call_keeps_its_fields_in_order_and_knows_where_they_go() {
    let call =
        Call::delete("/fapi/v3/order").field("symbol", "BTCUSDT").maybe("orderId", Some(7)).maybe("x", None::<u8>);
    assert_eq!(call.fields, vec![("symbol", "BTCUSDT".to_owned()), ("orderId", "7".to_owned())]);
    assert_eq!((call.value("orderId"), call.uses_query(), call.weight), (Some("7"), true, 1));
    assert!(!Call::post("/fapi/v3/order").uses_query());
    assert!(Call::post("/fapi/v3/approveAgent").query_only().uses_query());
    assert_eq!(Call::post("/fapi/v3/order").weight, 0);
    assert_eq!(Call::get("/fapi/v3/account").weight, 5);
    assert_eq!(Call::get("/fapi/v3/income").weight, 30);
    assert_eq!(Call::get("/aster-chain/v3/perp/user-withdraw-info").on(Host::Chain).host, Host::Chain);
}

#[test]
fn refusals_carry_their_code_and_message() {
    let refused = decode::<serde_json::Value>("/fapi/v3/order", r#"{"code":-2019,"msg":"Margin is insufficient."}"#);
    let error = refused.unwrap_err();
    assert_eq!(error.code(), Some(codes::MARGIN_NOT_SUFFICIENT));
    assert!(error.is_insufficient() && !error.may_have_landed());
    for ok in [r#"{"code":200,"msg":"success"}"#, r#"{"code":0}"#, r#"{"code":"000000","data":[]}"#, "[]"] {
        assert!(decode::<serde_json::Value>("/x", ok).is_ok(), "{ok}");
    }
    let string_code = decode::<serde_json::Value>("/x", r#"{"code":"-1121","msg":"Invalid symbol."}"#).unwrap_err();
    assert_eq!(string_code.code(), Some(codes::BAD_SYMBOL));
    let unknown = decode::<serde_json::Value>("/x", r#"{"code":-1007,"msg":"Timeout"}"#).unwrap_err();
    assert!(unknown.may_have_landed());
    assert!(decode::<serde_json::Value>("/x", r#"{"code":-4225,"msg":"Nonce Expired"}"#).unwrap_err().is_nonce());
    assert!(decode::<serde_json::Value>("/x", r#"{"code":-5050,"msg":"d"}"#).unwrap_err().needs_deposit());
    assert!(decode::<serde_json::Value>("/x", r#"{"code":-1022,"msg":"s"}"#).unwrap_err().is_unauthorized());
}

#[test]
fn a_refusal_in_an_error_status_reads_as_the_refusal() {
    let failing = Recorded::failing(TransportError::Status {
        status: 400,
        body: r#"{"code":-1121,"msg":"Invalid symbol."}"#.into(),
    });
    let error = block_on(read::<serde_json::Value>(&failing, Call::get("/fapi/v3/depth"))).unwrap_err();
    assert_eq!(error, ApiError::Refused { code: -1121, message: "Invalid symbol.".into() });
    let bare = Recorded::failing(TransportError::Status { status: 503, body: "busy".into() });
    let error = block_on(read::<serde_json::Value>(&bare, Call::post("/fapi/v3/order"))).unwrap_err();
    assert!(error.may_have_landed());
    let garbled = Recorded::answering("{not json");
    let error = block_on(read::<Vec<u8>>(&garbled, Call::get("/fapi/v3/ping"))).unwrap_err();
    assert!(matches!(error, ApiError::Unreadable { .. }));
}
