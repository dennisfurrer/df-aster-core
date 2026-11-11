//! Leverage, margin type, isolated margin and its history: fields, ranges refused here, and the
//! docs' example answers, a bare JSON number among them.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const LEVERAGE: &str = include_str!("../../../fixtures/aster/trade_leverage.json");
const MARGIN_TYPE: &str = include_str!("../../../fixtures/aster/trade_margin_type.json");
const POSITION_MARGIN: &str = include_str!("../../../fixtures/aster/trade_position_margin.json");
const HISTORY: &str = include_str!("../../../fixtures/aster/trade_margin_history.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn leverage_is_one_to_125() {
    let transport = Recorded::answering(LEVERAGE);
    let set = block_on(set_leverage(&transport, "BTCUSDT", 21)).unwrap();
    assert_eq!(set, Leverage { symbol: "BTCUSDT".into(), leverage: 21, max_notional_value: d("1000000") });
    let call = transport.only();
    assert_eq!((call.method, call.path, call.signed), (Method::Post, "/fapi/v3/leverage", true));
    assert_eq!(call.fields, [("symbol", "BTCUSDT".to_owned()), ("leverage", "21".to_owned())]);
    for bad in [0, 126] {
        assert!(set_leverage_call("BTCUSDT", bad).is_err());
    }
}

#[test]
fn margin_type_and_isolated_margin() {
    let transport = Recorded::answering(MARGIN_TYPE);
    block_on(set_margin_type(&transport, "BTCUSDT", &MarginType::Isolated)).unwrap();
    assert_eq!(transport.only().fields, [("symbol", "BTCUSDT".to_owned()), ("marginType", "ISOLATED".to_owned())]);

    let transport = Recorded::answering(POSITION_MARGIN);
    let changed = block_on(change_margin(
        &transport,
        "BTCUSDT",
        Some(&PositionSide::Long),
        d("100"),
        MarginChange::Add,
        Some("add-1"),
    ))
    .unwrap();
    assert_eq!((changed.amount, changed.change), (d("100"), 1));
    let call = transport.only();
    assert_eq!(call.path, "/fapi/v3/positionMargin");
    let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["symbol", "positionSide", "amount", "type", "clientTranId"]);
    assert_eq!(call.value("type"), Some("1"));
    assert!(change_margin_call("BTCUSDT", None, Dec::ZERO, MarginChange::Reduce, None).is_err());
    assert!(change_margin_call("BTCUSDT", None, d("1"), MarginChange::Reduce, Some(&"x".repeat(65))).is_err());
    assert_eq!(MarginChange::from_code(2), Some(MarginChange::Reduce));
}

#[test]
fn the_history_reads_each_change() {
    let transport = Recorded::answering(HISTORY);
    let query = MarginQuery { change: Some(MarginChange::Add), limit: Some(10), ..MarginQuery::default() };
    let history = block_on(margin_history(&transport, "BTCUSDT", &query)).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!((history[0].amount, history[0].delta_type.as_str()), (d("23.36332311"), "TRADE"));
    assert_eq!(history[1].position_side, PositionSide::Long);
    let call = transport.only();
    assert_eq!((call.method, call.path), (Method::Get, "/fapi/v3/positionMargin/history"));
    assert_eq!(call.fields, [("symbol", "BTCUSDT".to_owned()), ("type", "1".to_owned()), ("limit", "10".to_owned())]);
}
