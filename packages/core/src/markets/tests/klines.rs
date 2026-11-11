//! BTCUSDT's last three hourly trade, index and mark price candles, recorded from
//! fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn trade_candles_read_cell_by_cell() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_klines_btcusdt_1h.json"));
    let query = KlineQuery::latest(Interval::H1).limit(3);
    let candles = block_on(klines(&transport, "BTCUSDT", &query)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/klines", 1));
    assert_eq!(
        call.fields,
        vec![("symbol", "BTCUSDT".to_owned()), ("interval", "1h".to_owned()), ("limit", "3".to_owned())]
    );
    assert_eq!(candles.len(), 3);
    let first = &candles[0];
    assert_eq!((first.open_time, first.close_time, first.trades), (1_794_391_200_000, 1_794_394_799_999, 2_127));
    assert_eq!(
        (first.open, first.high, first.low, first.close),
        (d("73759.3"), d("74118.8"), d("73641.5"), d("74021.5"))
    );
    assert_eq!((first.volume, first.quote_volume), (d("217.336"), d("16070761.422")));
    assert_eq!((first.taker_buy_volume, first.taker_buy_quote_volume), (d("93.903"), d("6945752.6127")));
}

#[test]
fn index_and_mark_candles_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_indexPriceKlines_btcusdt.json"));
    let query = KlineQuery::latest(Interval::H1).limit(3).between(1, 2);
    let index = block_on(index_price_klines(&transport, "BTCUSDT", &query)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.path, call.value("pair"), call.value("startTime")),
        ("/fapi/v3/indexPriceKlines", Some("BTCUSDT"), Some("1"))
    );
    assert_eq!((index[0].open, index[0].points), (d("73792.23021739"), 3_600));
    assert_eq!(index[2].points, 1_622);

    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_markPriceKlines_btcusdt.json"));
    let mark = block_on(mark_price_klines(&transport, "BTCUSDT", &KlineQuery::latest(Interval::H1))).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.value("symbol"), call.weight), ("/fapi/v3/markPriceKlines", Some("BTCUSDT"), 5));
    assert_eq!((mark[1].close, mark[1].close_time), (d("73965.3"), 1_794_398_399_999));
}

#[test]
fn weights_follow_the_limit_and_short_rows_are_refused() {
    let weights: Vec<u32> = [Some(1), Some(99), Some(100), Some(499), Some(500), Some(1_000), Some(1_001), None]
        .into_iter()
        .map(kline_weight)
        .collect();
    assert_eq!(weights, vec![1, 1, 2, 2, 5, 5, 10, 5]);
    assert_eq!(klines_call("X", &KlineQuery::latest(Interval::M1).limit(9_999)).value("limit"), Some("1500"));
    assert!(serde_json::from_str::<Vec<Kline>>(r#"[[1,"1","1","1","1"]]"#).is_err());
    assert!(serde_json::from_str::<Vec<Kline>>(r#"[[1,1.5,"1","1","1","1",2,"1",3,"1","1","0"]]"#).is_err());
}
