//! Strategy orders: the leg list as JSON, the counts each type takes, a refusal carried in a
//! 200 answer, and the docs' example answers read whole.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const PLACED: &str = include_str!("../../../fixtures/aster/trade_strategy_place.json");
const UPDATED: &str = include_str!("../../../fixtures/aster/trade_strategy_update.json");
const OPEN: &str = include_str!("../../../fixtures/aster/trade_strategy_open.json");
const HISTORY: &str = include_str!("../../../fixtures/aster/trade_strategy_history.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn oto() -> Strategy {
    let mut entry = Leg::new("BTCUSDT", Side::Buy, OrderType::Limit);
    entry.quantity = Some(d("0.001"));
    entry.price = Some(d("50000"));
    entry.time_in_force = Some(TimeInForce::Gtc);
    let mut exit = Leg::new("BTCUSDT", Side::Sell, OrderType::TakeProfitMarket);
    exit.stop_price = Some(d("60000"));
    exit.close_position = Some(true);
    exit.first = Some(Driver { leg: 1, on: "FILLED".into(), action: "PLACE_ORDER".into() });
    Strategy {
        strategy_type: StrategyType::Oto,
        legs: vec![entry, exit],
        client_strategy_id: Some("myStrategy1".into()),
        builder: None,
        fee_rate: None,
    }
}

#[test]
fn a_strategy_goes_as_one_list_of_legs() {
    let call = place_strategy_call(&oto()).unwrap();
    assert_eq!(
        (call.method, call.path, call.orders, call.weight),
        (Method::Post, "/fapi/v3/placeStrategyOrder", 2, 50)
    );
    let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["clientStrategyId", "strategyType", "subOrderList"]);
    assert_eq!(
        call.value("subOrderList"),
        Some(
            r#"[{"strategySubId":"1","securityType":"USDT_FUTURES","symbol":"BTCUSDT","side":"BUY","type":"LIMIT","quantity":"0.001","price":"50000","timeInForce":"GTC"},{"strategySubId":"2","securityType":"USDT_FUTURES","symbol":"BTCUSDT","side":"SELL","type":"TAKE_PROFIT_MARKET","stopPrice":"60000","closePosition":"true","firstDrivenId":"1","firstDrivenOn":"FILLED","firstTrigger":"PLACE_ORDER"}]"#
        )
    );
    let mut three = oto();
    three.legs.push(three.legs[1].clone());
    assert!(matches!(place_strategy_call(&three), Err(Invalid::Count { count: 3, .. })));
    three.strategy_type = StrategyType::Otoco;
    assert!(place_strategy_call(&three).is_ok());
    let mut fee = oto();
    fee.fee_rate = Some(d("0.0001"));
    assert_eq!(place_strategy_call(&fee).unwrap_err(), Invalid::Conflict("feeRate", "no builder"));
    let mut pegged = oto();
    pegged.legs[0].peg = Some(PegPriceType::Queue1);
    assert!(
        place_strategy_call(&pegged)
            .unwrap()
            .value("subOrderList")
            .unwrap()
            .contains(r#""price":"0","pegPriceType":"QUEUE_1""#)
    );
    pegged.legs[1].peg = Some(PegPriceType::Queue1);
    assert!(matches!(place_strategy_call(&pegged), Err(Invalid::NotTaken { .. })));
    let mut ioc = oto();
    ioc.legs[0].time_in_force = Some(TimeInForce::Ioc);
    assert!(place_strategy_call(&ioc).is_err());
}

#[test]
fn placing_reads_a_failure_code_as_a_refusal() {
    let transport = Recorded::answering(PLACED);
    let placed = block_on(place_strategy(&transport, &oto())).unwrap();
    assert_eq!((placed.strategy_id, placed.strategy_status.as_str()), (123_456_789, "WORKING"));
    let refused = r#"{"strategyId":1,"strategyType":"OTO","strategyStatus":"REJECTED","failureCode":-4142,"failureReason":"would trigger"}"#;
    let error = block_on(place_strategy(&Recorded::answering(refused), &oto())).unwrap_err();
    assert_eq!(error, TradeError::Api(ApiError::Refused { code: -4142, message: "would trigger".into() }));
}

#[test]
fn an_update_names_each_leg_by_its_sub_id() {
    let mut exit = oto().legs[1].clone();
    exit.stop_price = Some(d("61000"));
    exit.first = None;
    let call = update_strategy_call(123_456_789, StrategyType::Oto, &[(2, exit.clone())]).unwrap();
    assert_eq!(call.path, "/fapi/v3/updateStrategyOrder");
    assert_eq!(call.value("strategyId"), Some("123456789"));
    assert!(call.value("subOrderList").unwrap().starts_with(r#"[{"strategySubId":"2","#));
    assert!(update_strategy_call(1, StrategyType::Oto, &[]).is_err());
    let answer =
        block_on(update_strategy(&Recorded::answering(UPDATED), 123_456_789, StrategyType::Oto, &[(2, exit)])).unwrap();
    assert_eq!((answer[0].updated_sub_order, answer[0].update_status.as_str()), (Some(1), "SUCCESS"));
}

#[test]
fn open_and_ended_strategies_read_with_their_legs() {
    let transport = Recorded::answering(OPEN);
    let open = block_on(open_strategy(&transport, &StrategyRef::Id(123_456_789), StrategyType::Oto)).unwrap();
    let leg = &open.sub_orders[0];
    assert_eq!((leg.order_id, leg.strategy_sub_id, leg.quantity), (Some(987_654_321), 1, d("0.001")));
    assert_eq!((leg.activate_price, leg.trigger_time), (None, 0));
    let call = transport.only();
    assert_eq!(call.fields, [("strategyId", "123456789".to_owned()), ("strategyType", "OTO".to_owned())]);

    let transport = Recorded::answering(HISTORY);
    let query = HistoryQuery { start_ms: Some(1_699_800_000_000), ..HistoryQuery::default() };
    let ended =
        block_on(strategy_history(&transport, &StrategyRef::Client("myStrategy1".into()), StrategyType::Oto, &query))
            .unwrap();
    assert_eq!((ended.strategy_status.as_str(), ended.sub_orders[0].status.clone()), ("EXPIRED", OrderStatus::Filled));
    let keys: Vec<&str> = transport.only().fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["clientStrategyId", "strategyType", "startTime"]);
}
