//! BTCUSDT's book at five levels, recorded from fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn the_book_reads_best_first() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_depth_btcusdt.json"));
    let book = block_on(depth(&transport, "BTCUSDT", DepthLimit::L5)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/depth", 2));
    assert_eq!(call.fields, vec![("symbol", "BTCUSDT".to_owned()), ("limit", "5".to_owned())]);
    assert_eq!(
        (book.last_update_id, book.message_time, book.transaction_time),
        (648_144_898_682, 1_794_400_048_849, 1_794_400_048_800)
    );
    assert_eq!((book.bids.len(), book.asks.len()), (5, 5));
    assert_eq!(book.best_bid(), Some(Level { price: d("73878.6"), qty: d("0.505") }));
    assert_eq!(book.best_ask().unwrap().price, d("73878.7"));
    assert_eq!(book.spread(), Some(d("0.1")));
    assert!(book.bids.windows(2).all(|pair| pair[0].price > pair[1].price));
    assert!(book.asks.windows(2).all(|pair| pair[0].price < pair[1].price));
}

#[test]
fn depth_weighs_by_its_limit() {
    let weights: Vec<u32> = DepthLimit::ALL.iter().map(|limit| depth_call("X", *limit).weight).collect();
    assert_eq!(weights, vec![2, 2, 2, 2, 5, 10, 20]);
    assert_eq!((DepthLimit::at_least(7), DepthLimit::at_least(5000)), (DepthLimit::L10, DepthLimit::L1000));
    assert_eq!(DepthLimit::default().count(), 500);
}

#[test]
fn a_refused_limit_is_the_venues_refusal() {
    let transport = Recorded::answering(r#"{"code":-4021,"msg":"7 is not valid depth limit"}"#);
    let error = block_on(depth(&transport, "BTCUSDT", DepthLimit::L5)).unwrap_err();
    assert_eq!(error.code(), Some(-4021));
}
