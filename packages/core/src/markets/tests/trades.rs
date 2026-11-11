//! BTCUSDT's last three trades, older trades and aggregate trades, recorded from
//! fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn the_tape_reads() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_trades_btcusdt.json"));
    let trades = block_on(recent_trades(&transport, "BTCUSDT", Some(3))).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/trades", 1));
    assert_eq!(call.fields, vec![("symbol", "BTCUSDT".to_owned()), ("limit", "3".to_owned())]);
    assert_eq!(trades.len(), 3);
    let second = &trades[1];
    assert_eq!(
        (second.id, second.price, second.qty, second.quote_qty),
        (149_792_423, d("73870.3"), d("0.382"), d("28218.4"))
    );
    assert_eq!((second.time, second.is_buyer_maker), (1_794_400_042_700, true));
}

#[test]
fn older_trades_read_from_an_id() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_historicalTrades_btcusdt.json"));
    let trades = block_on(historical_trades(&transport, "BTCUSDT", Some(5_000), Some(149_792_422))).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/historicalTrades", 20));
    assert_eq!(call.value("limit"), Some("1000"));
    assert_eq!(call.value("fromId"), Some("149792422"));
    assert_eq!(trades[0].id, 149_792_422);
}

#[test]
fn aggregate_trades_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_aggTrades_btcusdt.json"));
    let query = AggTradeQuery { start_time: Some(1), end_time: Some(2), limit: Some(3), ..AggTradeQuery::default() };
    let trades = block_on(agg_trades(&transport, "BTCUSDT", &query)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/aggTrades", 20));
    let fields: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(fields, vec!["symbol", "startTime", "endTime", "limit"]);
    let first = &trades[0];
    assert_eq!((first.id, first.price, first.qty), (89_237_978, d("73870.4"), d("0.003")));
    assert_eq!(
        (first.first_trade_id, first.last_trade_id, first.time, first.is_buyer_maker),
        (149_792_422, 149_792_422, 1_794_400_040_050, false)
    );
}
