//! The frames each request writes, with its id.

use super::*;

#[test]
fn each_request_writes_its_method_params_and_id() {
    let names = vec!["btcusdt@aggTrade".to_owned(), "btcusdt@depth".to_owned()];
    assert_eq!(subscribe(1, &names), r#"{"id":1,"method":"SUBSCRIBE","params":["btcusdt@aggTrade","btcusdt@depth"]}"#);
    assert_eq!(unsubscribe(312, &names[1..]), r#"{"id":312,"method":"UNSUBSCRIBE","params":["btcusdt@depth"]}"#);
    assert_eq!(list_subscriptions(3), r#"{"id":3,"method":"LIST_SUBSCRIPTIONS"}"#);
    assert_eq!(Asked::SetCombined(true).frame(5), r#"{"id":5,"method":"SET_PROPERTY","params":["combined",true]}"#);
    assert_eq!(Asked::GetCombined.frame(2), r#"{"id":2,"method":"GET_PROPERTY","params":["combined"]}"#);
    let read: serde_json::Value = serde_json::from_str(&subscribe(9, &names)).unwrap();
    assert_eq!((read["method"].as_str(), read["id"].as_u64()), (Some("SUBSCRIBE"), Some(9)));
}
