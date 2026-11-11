//! Cancels, the countdown, modifies and chases: each call's method, path and fields, the lists a
//! batch names its orders by, per-row refusals, and the docs' example answers read whole.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, Wire, block_on, ok, signing_client};

const CANCELLED: &str = include_str!("../../../fixtures/aster/trade_cancel.json");
const CANCELLED_BATCH: &str = include_str!("../../../fixtures/aster/trade_cancel_batch.json");
const CANCELLED_ALL: &str = include_str!("../../../fixtures/aster/trade_cancel_all.json");
const COUNTDOWN: &str = include_str!("../../../fixtures/aster/trade_countdown.json");
const MODIFIED: &str = include_str!("../../../fixtures/aster/trade_modify.json");
const MODIFIED_BATCH: &str = include_str!("../../../fixtures/aster/trade_batch_modify.json");
const CHASED: &str = include_str!("../../../fixtures/aster/trade_chase.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn fields(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs.iter().map(|(key, value)| (*key, (*value).to_owned())).collect()
}

#[test]
fn a_cancel_names_its_order_by_id_or_client_id() {
    let transport = Recorded::answering(CANCELLED);
    let order = block_on(cancel(&transport, "BTCUSDT", &OrderRef::Id(283_194_212))).unwrap();
    assert_eq!((order.order_id, order.status.clone()), (283_194_212, crate::utils::OrderStatus::Canceled));
    let call = transport.only();
    assert_eq!(
        (call.method, call.path, call.signed, call.uses_query()),
        (Method::Delete, "/fapi/v3/order", true, true)
    );
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("orderId", "283194212")]));
    let by_client = cancel_call("BTCUSDT", &OrderRef::from("myOrder1"));
    assert_eq!(by_client.fields, fields(&[("symbol", "BTCUSDT"), ("origClientOrderId", "myOrder1")]));
}

#[test]
fn a_batch_cancel_names_its_orders_in_one_list() {
    let by_id = cancel_batch_call("BTCUSDT", &[OrderRef::Id(1_234_567), OrderRef::Id(2_345_678)]).unwrap();
    assert_eq!((by_id.method, by_id.path), (Method::Delete, "/fapi/v3/batchOrders"));
    assert_eq!(by_id.fields, fields(&[("symbol", "BTCUSDT"), ("orderIdList", "[1234567,2345678]")]));
    let by_client = cancel_batch_call("BTCUSDT", &["my_id_1".into(), "my_id_2".into()]).unwrap();
    assert_eq!(by_client.value("origClientOrderIdList"), Some(r#"["my_id_1","my_id_2"]"#));
    let mixed = cancel_batch_call("BTCUSDT", &[OrderRef::Id(1), "a".into()]);
    assert_eq!(mixed.unwrap_err(), Invalid::Conflict("orderIdList", "origClientOrderIdList"));
    let eleven: Vec<OrderRef> = (0..11).map(OrderRef::Id).collect();
    assert!(matches!(cancel_batch_call("BTCUSDT", &eleven), Err(Invalid::Count { count: 11, .. })));

    let transport = Recorded::answering(CANCELLED_BATCH);
    let rows = block_on(cancel_batch(&transport, "BTCUSDT", &[OrderRef::Id(283_194_212), OrderRef::Id(9)])).unwrap();
    assert_eq!(rows[0].as_ref().unwrap().order_id, 283_194_212);
    assert_eq!(rows[1], Err(ApiError::Refused { code: -2011, message: "Unknown order sent.".into() }));
}

#[test]
fn cancel_all_and_the_countdown() {
    let transport = Recorded::answering(CANCELLED_ALL);
    block_on(cancel_all(&transport, "BTCUSDT")).unwrap();
    let call = transport.only();
    assert_eq!((call.method, call.path), (Method::Delete, "/fapi/v3/allOpenOrders"));
    let transport = Recorded::answering(COUNTDOWN);
    let countdown = block_on(countdown_cancel_all(&transport, "BTCUSDT", 120_000)).unwrap();
    assert_eq!(countdown, Countdown { symbol: "BTCUSDT".into(), countdown_time: 100_000 });
    let call = transport.only();
    assert_eq!((call.method, call.path, call.weight), (Method::Post, "/fapi/v3/countdownCancelAll", 10));
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("countdownTime", "120000")]));
}

#[test]
fn a_modify_puts_a_new_price_and_quantity() {
    let change = Modify::new("GOOGLUSDT", 405, d("0.150"), d("295.50")).side(Side::Buy);
    let call = modify_call(&change).unwrap();
    assert_eq!((call.method, call.path, call.orders, call.uses_query()), (Method::Put, "/fapi/v3/order", 1, false));
    assert_eq!(
        call.fields,
        fields(&[
            ("symbol", "GOOGLUSDT"),
            ("orderId", "405"),
            ("side", "BUY"),
            ("quantity", "0.15"),
            ("price", "295.5")
        ])
    );
    assert!(modify_call(&Modify::new("X", 1, Dec::ZERO, d("1"))).is_err());
    let transport = Recorded::answering(MODIFIED);
    let order = block_on(modify(&transport, &change)).unwrap();
    assert_eq!((order.order_id, order.price), (405, d("295.5")));

    let batch = [
        Modify::new("BTCUSDT", 20_072_994_001, d("0.01"), d("65000")),
        Modify::new("ETHUSDT", "x", d("0.2"), d("3500")),
    ];
    let call = modify_batch_call(&batch).unwrap();
    assert_eq!((call.method, call.path, call.orders, call.weight), (Method::Put, "/fapi/v3/batchOrders", 2, 5));
    assert_eq!(
        call.value("batchOrders"),
        Some(
            r#"[{"symbol":"BTCUSDT","orderId":"20072994001","quantity":"0.01","price":"65000"},{"symbol":"ETHUSDT","origClientOrderId":"x","quantity":"0.2","price":"3500"}]"#
        )
    );
    let transport = Recorded::answering(MODIFIED_BATCH);
    let rows = block_on(modify_batch(&transport, &batch)).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].as_ref().unwrap_err().code(), Some(-2013));
    assert_eq!(rows[2].as_ref().unwrap().symbol, "ETHUSDT");
}

#[test]
fn a_chase_checks_its_offsets_and_id() {
    let mut chase_order = Chase::new("BTCUSDT", Side::Buy, QuantityUnit::Base, d("0.1"));
    chase_order.offset = Some(d("0.5"));
    chase_order.max_offset = Some((d("10.0"), OffsetType::Absolute));
    chase_order.client_strategy_id = Some("my_chase_1".into());
    let call = chase_call(&chase_order).unwrap();
    assert_eq!((call.method, call.path, call.orders), (Method::Post, "/fapi/v3/chase", 1));
    assert_eq!(
        call.fields,
        fields(&[
            ("symbol", "BTCUSDT"),
            ("side", "BUY"),
            ("quantityUnit", "BASE"),
            ("quantity", "0.1"),
            ("chaseOffset", "0.5"),
            ("maxChaseOffset", "10"),
            ("maxChaseOffsetType", "ABSOLUTE"),
            ("clientStrategyId", "my_chase_1"),
        ])
    );
    let mut bad = chase_order.clone();
    bad.max_offset = Some((d("1.005"), OffsetType::Percentage));
    assert!(matches!(chase_call(&bad), Err(Invalid::OutOfRange { param: "maxChaseOffset", .. })));
    bad.max_offset = None;
    bad.client_strategy_id = Some("x".repeat(29));
    assert!(matches!(chase_call(&bad), Err(Invalid::ClientId(_))));
    bad.client_strategy_id = None;
    bad.offset = Some(d("-1"));
    assert!(matches!(chase_call(&bad), Err(Invalid::OutOfRange { param: "chaseOffset", .. })));

    let client = signing_client(Wire::answering([ok(CHASED)]));
    let answer = block_on(chase(&client, &chase_order)).unwrap();
    assert_eq!((answer.strategy_id, answer.strategy_status.as_str()), (12_345, "NEW"));
    assert_eq!((answer.max_chase_offset, answer.time_in_force), (Some(d("10")), Some(TimeInForce::Gtx)));
    assert!(client.http().bodies()[0].1.starts_with("symbol=BTCUSDT&side=BUY&quantityUnit=BASE&"));
}
