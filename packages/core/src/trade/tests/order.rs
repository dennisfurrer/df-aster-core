//! The docs' answers read whole, `""` read as none, and the fields only some answers carry.

use super::*;

#[test]
fn reads_the_docs_new_order_and_query_answers() {
    let placed = r#"{"clientOrderId":"testOrder","cumQty":"0","cumQuote":"0","executedQty":"0","orderId":22542179,
        "avgPrice":"0.00000","origQty":"10","price":"0","reduceOnly":false,"side":"BUY","positionSide":"SHORT",
        "status":"NEW","stopPrice":"9300","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC",
        "type":"TRAILING_STOP_MARKET","origType":"TRAILING_STOP_MARKET","activatePrice":"9020","priceRate":"0.3",
        "updateTime":1566818724722,"workingType":"CONTRACT_PRICE","priceProtect":false,"pegPriceType":"","stpMode":""}"#;
    let order: Order = serde_json::from_str(placed).unwrap();
    assert_eq!(
        (order.order_id, order.side.clone(), order.position_side.clone()),
        (22_542_179, Side::Buy, PositionSide::Short)
    );
    assert_eq!(order.order_type, OrderType::TrailingStopMarket);
    assert_eq!((order.activate_price, order.price_rate), (Dec::parse("9020").ok(), Dec::parse("0.3").ok()));
    assert_eq!((order.peg_price_type.clone(), order.stp_mode.clone(), order.time), (None, None, None));
    assert!(order.is_open());
    assert_eq!(order.remaining(), Dec::from_int(10));

    let queried = r#"{"avgPrice":"0.00000","clientOrderId":"abc","cumQuote":"0","executedQty":"0","orderId":1917641,
        "origQty":"0.40","origType":"TRAILING_STOP_MARKET","price":"0","reduceOnly":false,"side":"BUY",
        "positionSide":"SHORT","status":"NEW","stopPrice":"9300","closePosition":false,"symbol":"BTCUSDT",
        "time":1579276756075,"timeInForce":"GTC","type":"TRAILING_STOP_MARKET","activatePrice":"9020",
        "priceRate":"0.3","updateTime":1579276756075,"workingType":"CONTRACT_PRICE","priceProtect":false,
        "stpMode":"EXPIRE_MAKER"}"#;
    let order: Order = serde_json::from_str(queried).unwrap();
    assert_eq!((order.time, order.stp_mode.clone()), (Some(1_579_276_756_075), Some(StpMode::ExpireMaker)));
    assert_eq!(order.orig_qty, Dec::parse("0.4").unwrap());
}
