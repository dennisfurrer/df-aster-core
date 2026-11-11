//! BTCUSDT's day, last price and best bid and ask, and four symbols' last prices (every
//! symbol's answer cut down), recorded from fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn the_day_reads() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_ticker24hr_btcusdt.json"));
    let day = block_on(ticker_24hr(&transport, "BTCUSDT")).unwrap();
    assert_eq!((transport.only().path, transport.only().weight), ("/fapi/v3/ticker/24hr", 1));
    assert_eq!((day.price_change, day.price_change_percent, day.last_price), (d("-432.9"), d("-0.583"), d("73878.7")));
    assert_eq!((day.volume, day.quote_volume, day.count), (d("7465.871"), d("550043798.8"), 61_666));
    assert_eq!((day.first_id, day.last_id, day.prev_close_price), (149_730_747, 149_792_425, None));
    assert_eq!(tickers_24hr_call().weight, 40);
}

#[test]
fn last_prices_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_tickerPrice_btcusdt.json"));
    let btc = block_on(last_price(&transport, "BTCUSDT")).unwrap();
    assert_eq!(transport.only().path, "/fapi/v3/ticker/price");
    assert_eq!((btc.price, btc.time), (d("73878.7"), 1_794_400_050_050));
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_tickerPrice_some.json"));
    let all = block_on(last_prices(&transport)).unwrap();
    assert_eq!((all.len(), transport.only().weight), (4, 2));
}

#[test]
fn the_best_bid_and_ask_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_bookTicker_btcusdt.json"));
    let top = block_on(book_ticker(&transport, "BTCUSDT")).unwrap();
    assert_eq!(transport.only().path, "/fapi/v3/ticker/bookTicker");
    assert_eq!(
        (top.bid_price, top.bid_qty, top.ask_price, top.ask_qty),
        (d("73878.6"), d("0.506"), d("73878.7"), d("2.219"))
    );
    assert_eq!(top.last_update_id, Some(648_145_000_741));
    assert_eq!(book_tickers_call().weight, 2);
}
