//! New orders: the fields in the order Aster's web app writes them (held to the globe.li
//! vectors byte for byte), each type's rules refused here, fitting to a grid, batches and their
//! per-row refusals, and a round trip through a client. The answers are the docs' examples.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, Wire, agent, block_on, ok, signing_client};
use crate::utils::sign::sign_params;
use crate::utils::{Address, OffGrid};

const VECTORS: &str = include_str!("../../../vectors/aster_web.json");
const PLACED: &str = include_str!("../../../fixtures/aster/trade_order.json");
const BATCH: &str = include_str!("../../../fixtures/aster/trade_batch_orders.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn globe_li(name: &str) -> NewOrder {
    let result = ResponseType::Result;
    match name {
        "limit" => NewOrder::limit("BTCUSDT", Side::Buy, d("0.002"), d("83000.1"), TimeInForce::Gtc).reduce_only(false),
        "market_reduce_only" => NewOrder::market("ETHUSDT", Side::Sell, d("0.5")).reduce_only(true),
        "stop_market" => NewOrder::stop_market("BTCUSDT", Side::Sell, d("0.002"), d("80000")).reduce_only(true),
        "cjk_symbol" => {
            NewOrder::limit("龙虾USDT", Side::Buy, d("100"), d("0.012345"), TimeInForce::Gtx).reduce_only(false)
        }
        other => panic!("no order {other}"),
    }
    .response(result)
}

#[test]
fn orders_sign_as_globe_li_signs_them() {
    let vectors: Value = serde_json::from_str(VECTORS).unwrap();
    let agent = agent();
    for name in ["limit", "market_reduce_only", "stop_market", "cjk_symbol"] {
        let case = vectors["trades"].as_array().unwrap().iter().find(|case| case["name"] == name).unwrap();
        let nonce: u64 = case["nonce"].as_str().unwrap().parse().unwrap();
        let call = place_call(&globe_li(name)).unwrap();
        let signed = sign_params(&call.fields, nonce, &agent).unwrap();
        assert_eq!(signed.msg, case["msg"].as_str().unwrap(), "{name}");
        assert_eq!(signed.signature.to_string(), case["signature"].as_str().unwrap(), "{name}");
        assert_eq!(signed.encoded(), case["request"]["body"].as_str().unwrap(), "{name}");
    }
}

#[test]
fn a_placement_is_a_signed_post_that_counts_one_order() {
    let call = place_call(&globe_li("limit")).unwrap();
    assert_eq!(
        (call.method, call.path, call.signed, call.orders, call.weight),
        (Method::Post, "/fapi/v3/order", true, 1, 0)
    );
    assert!(!call.uses_query());
}

#[test]
fn every_type_carries_what_it_needs_and_nothing_else() {
    let q = d("1");
    let p = d("100");
    let gtc = || TimeInForce::Gtc;
    let missing = |ty: OrderType, param| Err(Invalid::Missing { order_type: ty, param });
    let not_taken = |ty: OrderType, param| Err(Invalid::NotTaken { order_type: ty, param });
    let mut limit_without_tif = NewOrder::limit("X", Side::Buy, q, p, gtc());
    limit_without_tif.time_in_force = None;
    assert_eq!(limit_without_tif.check(), missing(OrderType::Limit, "timeInForce"));
    let mut limit_with_stop = NewOrder::limit("X", Side::Buy, q, p, gtc());
    limit_with_stop.stop_price = Some(p);
    assert_eq!(limit_with_stop.check(), not_taken(OrderType::Limit, "stopPrice"));
    assert_eq!(
        NewOrder::limit("X", Side::Buy, q, p, gtc()).working_type(WorkingType::MarkPrice).check(),
        not_taken(OrderType::Limit, "workingType")
    );
    assert_eq!(
        NewOrder::market("X", Side::Buy, q).time_in_force(gtc()).check(),
        not_taken(OrderType::Market, "timeInForce")
    );
    let mut market_with_price = NewOrder::market("X", Side::Buy, q);
    market_with_price.price = Some(p);
    assert_eq!(market_with_price.check(), not_taken(OrderType::Market, "price"));
    let mut stop_without_price = NewOrder::stop("X", Side::Sell, q, p, p);
    stop_without_price.price = None;
    assert_eq!(stop_without_price.check(), missing(OrderType::Stop, "price"));
    assert!(
        NewOrder::take_profit("X", Side::Sell, q, p, p)
            .working_type(WorkingType::MarkPrice)
            .price_protect(true)
            .check()
            .is_ok()
    );
    let mut stop_market_with_price = NewOrder::stop_market("X", Side::Sell, q, p);
    stop_market_with_price.price = Some(p);
    assert_eq!(stop_market_with_price.check(), not_taken(OrderType::StopMarket, "price"));
    assert_eq!(
        NewOrder::take_profit_market("X", Side::Sell, q, p).time_in_force(TimeInForce::Ioc).check(),
        not_taken(OrderType::TakeProfitMarket, "timeInForce other than GTC")
    );
    assert!(NewOrder::close_stop_market("X", Side::Sell, p).check().is_ok());
    assert!(NewOrder::close_take_profit_market("X", Side::Buy, p).time_in_force(gtc()).check().is_ok());
    let mut close_with_quantity = NewOrder::close_stop_market("X", Side::Sell, p);
    close_with_quantity.quantity = Some(q);
    assert_eq!(close_with_quantity.check(), Err(Invalid::Conflict("closePosition", "quantity")));
    assert_eq!(
        NewOrder::close_stop_market("X", Side::Sell, p).reduce_only(true).check(),
        Err(Invalid::Conflict("closePosition", "reduceOnly"))
    );
    let trailing = NewOrder::trailing_stop_market("X", Side::Sell, q, d("1")).activation_price(p);
    assert!(trailing.check().is_ok());
    for rate in ["0.05", "5.1"] {
        let order = NewOrder::trailing_stop_market("X", Side::Sell, q, d(rate));
        assert!(matches!(order.check(), Err(Invalid::OutOfRange { param: "callbackRate", .. })), "{rate}");
    }
    assert_eq!(
        NewOrder::market("X", Side::Buy, q).peg(PegPriceType::Queue1).check(),
        not_taken(OrderType::Market, "pegPriceType")
    );
    assert!(NewOrder::pegged_limit("X", Side::Buy, q, PegPriceType::Counterparty1, TimeInForce::Gtx).check().is_ok());
    assert_eq!(
        NewOrder::market("X", Side::Buy, q).position_side(PositionSide::Long).reduce_only(true).check(),
        Err(Invalid::Conflict("reduceOnly", "a hedge mode positionSide"))
    );
    assert!(NewOrder::market("X", Side::Buy, q).position_side(PositionSide::Long).check().is_ok());
    assert!(matches!(NewOrder::market("X", Side::Buy, Dec::ZERO).check(), Err(Invalid::OutOfRange { .. })));
    let mut liquidation = NewOrder::market("X", Side::Buy, q);
    liquidation.order_type = OrderType::Liquidation;
    assert!(matches!(liquidation.check(), Err(Invalid::OutOfRange { param: "type", .. })));
}

#[test]
fn client_ids_follow_the_venues_pattern() {
    let market = || NewOrder::market("X", Side::Buy, d("1"));
    assert!(market().client_id("a.B:c/d_e-1").check().is_ok());
    assert!(market().client_id("x".repeat(36)).check().is_ok());
    for bad in ["", "has space", "x".repeat(37).as_str(), "emoji😀"] {
        assert_eq!(market().client_id(bad).check(), Err(Invalid::ClientId(bad.to_owned())), "{bad}");
    }
}

#[test]
fn every_field_goes_out_in_one_order() {
    let builder: Address = "0x014c85ffb0fF2F2972237AA950B452f92C69Ae1D".parse().unwrap();
    let order = NewOrder::stop("BTCUSDT", Side::Sell, d("0.010"), d("79000"), d("80000"))
        .position_side(PositionSide::Short)
        .time_in_force(TimeInForce::Gtc)
        .working_type(WorkingType::MarkPrice)
        .price_protect(true)
        .client_id("sl-1")
        .stp_mode(StpMode::ExpireTaker)
        .response(ResponseType::Ack)
        .builder(builder, d("0.00001"));
    let keys: Vec<&str> = order.fields().unwrap().iter().map(|(key, _)| *key).collect();
    assert_eq!(
        keys,
        [
            "symbol",
            "side",
            "positionSide",
            "type",
            "quantity",
            "price",
            "stopPrice",
            "timeInForce",
            "workingType",
            "priceProtect",
            "newClientOrderId",
            "stpMode",
            "newOrderRespType",
            "builder",
            "feeRate"
        ]
    );
    let fields = order.fields().unwrap();
    assert_eq!(fields[4].1, "0.01");
    assert_eq!(fields[9].1, "TRUE");
    assert_eq!(fields[13].1, "0x014c85ffb0fF2F2972237AA950B452f92C69Ae1D");
    assert_eq!(fields[14].1, "0.00001");
    let close = NewOrder::close_stop_market("BTCUSDT", Side::Sell, d("80000")).fields().unwrap();
    assert_eq!(close[3..], [("stopPrice", "80000".to_owned()), ("closePosition", "true".to_owned())]);
    let mut fee_alone = NewOrder::market("X", Side::Buy, d("1"));
    fee_alone.fee_rate = Some(d("0.0001"));
    assert_eq!(fee_alone.check(), Err(Invalid::Conflict("feeRate", "no builder")));
    let negative = NewOrder::market("X", Side::Buy, d("1")).builder(builder, d("-0.1"));
    assert!(matches!(negative.check(), Err(Invalid::OutOfRange { param: "feeRate", .. })));
}

fn grid() -> Grid {
    Grid {
        tick: d("0.1"),
        min_price: d("1"),
        max_price: d("1000000"),
        step: d("0.001"),
        min_qty: d("0.001"),
        max_qty: d("1000"),
        market_step: d("0.01"),
        market_min_qty: d("0.01"),
        market_max_qty: d("120"),
        min_notional: d("5"),
    }
}

#[test]
fn orders_fit_the_grid() {
    let limit = NewOrder::limit("BTCUSDT", Side::Buy, d("0.0029"), d("83000.17"), TimeInForce::Gtc);
    let fitted = limit.fit(&grid(), Round::Down).unwrap();
    assert_eq!((fitted.quantity, fitted.price), (Some(d("0.002")), Some(d("83000.1"))));
    let market = NewOrder::market("BTCUSDT", Side::Buy, d("0.029")).fit(&grid(), Round::Down).unwrap();
    assert_eq!(market.quantity, Some(d("0.02")));
    let stop = NewOrder::stop_market("BTCUSDT", Side::Sell, d("0.019"), d("80000.05")).fit(&grid(), Round::Up).unwrap();
    assert_eq!((stop.quantity, stop.stop_price), (Some(d("0.01")), Some(d("80000.1"))));
    let small = NewOrder::limit("BTCUSDT", Side::Buy, d("0.001"), d("1000"), TimeInForce::Gtc);
    assert_eq!(
        small.clone().fit(&grid(), Round::Down),
        Err(Invalid::OffGrid(OffGrid::BelowNotional { notional: d("1"), least: d("5") }))
    );
    assert!(small.reduce_only(true).fit(&grid(), Round::Down).is_ok());
    let over = NewOrder::market("BTCUSDT", Side::Buy, d("500")).fit(&grid(), Round::Down);
    assert!(matches!(over, Err(Invalid::OffGrid(OffGrid::AboveMax { .. }))));
}

#[test]
fn a_placement_answers_the_order() {
    let transport = Recorded::answering(PLACED);
    let order = block_on(place(&transport, &globe_li("limit"))).unwrap();
    assert_eq!((order.order_id, order.client_order_id.as_str()), (22_542_179, "testOrder"));
    assert_eq!(transport.only().path, "/fapi/v3/order");
    let refused = block_on(place(&transport, &NewOrder::market("X", Side::Buy, Dec::ZERO)));
    assert!(matches!(refused, Err(TradeError::Invalid(_))));
    assert_eq!(transport.sent().len(), 1, "a refused order is never sent");
}

#[test]
fn a_batch_is_a_json_list_with_a_row_an_order() {
    let orders = [globe_li("limit"), NewOrder::market("ETHUSDT", Side::Sell, d("0.5")).reduce_only(true)];
    let call = place_batch_call(&orders).unwrap();
    assert_eq!((call.method, call.path, call.orders, call.weight), (Method::Post, "/fapi/v3/batchOrders", 2, 5));
    assert_eq!(
        call.value("batchOrders").unwrap(),
        r#"[{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","quantity":"0.002","price":"83000.1","timeInForce":"GTC","reduceOnly":"false","newOrderRespType":"RESULT"},{"symbol":"ETHUSDT","side":"SELL","type":"MARKET","quantity":"0.5","reduceOnly":"true"}]"#
    );
    let six = vec![NewOrder::market("X", Side::Buy, d("1")); 6];
    assert_eq!(place_batch_call(&six).unwrap_err(), Invalid::Count { what: "batchOrders", count: 6, range: "1 to 5" });
    assert!(place_batch_call(&[]).is_err());

    let transport = Recorded::answering(BATCH);
    let rows = block_on(place_batch(&transport, &orders)).unwrap();
    assert_eq!(rows[0].as_ref().unwrap().order_id, 22_542_179);
    assert_eq!(rows[1], Err(ApiError::Refused { code: -2022, message: "ReduceOnly Order is rejected.".into() }));
    let garbled = rows_of(r#"[{"orderId":1}]"#);
    assert!(matches!(garbled[0], Err(ApiError::Unreadable { .. })));
}

fn rows_of(text: &str) -> Vec<Result<Order, ApiError>> {
    rows("/fapi/v3/batchOrders", serde_json::from_str(text).unwrap())
}

#[test]
fn a_placement_goes_through_a_client_signed_in_its_body() {
    let client = signing_client(Wire::answering([ok(PLACED)]));
    let order = block_on(place(&client, &globe_li("limit"))).unwrap();
    assert_eq!(order.order_id, 22_542_179);
    let bodies = client.http().bodies();
    assert_eq!(bodies[0].0, "https://fapi.asterdex.com/fapi/v3/order");
    assert!(
        bodies[0].1.starts_with("symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=0.002&price=83000.1&timeInForce=GTC&reduceOnly=false&newOrderRespType=RESULT&nonce=1790000000000000&user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&signer=0xe0a5193404A0f6d94c087169c539eB0180FCf828&signature=0x"),
        "{}",
        bodies[0].1
    );
}
