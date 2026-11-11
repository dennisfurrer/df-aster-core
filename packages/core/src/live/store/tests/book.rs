//! ETHUSDT's book kept from a REST snapshot and the diff stream recorded with it on 2026-11-11,
//! the docs' rules for the first update, a broken chain, and a snapshot too old for the buffer.

use super::*;
use crate::live::fixtures::{DIFFS, SNAPSHOT};
use crate::live::{Data, Frame, decode};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn diffs() -> Vec<DepthUpdate> {
    DIFFS
        .lines()
        .map(|line| match decode(line).unwrap() {
            Frame::Data { data: Data::Depth(update), .. } => update,
            other => panic!("{other:?}"),
        })
        .collect()
}

fn snapshot() -> Snapshot {
    serde_json::from_str(SNAPSHOT).unwrap()
}

#[test]
fn the_recorded_snapshot_and_diffs_make_a_book() {
    let snapshot = snapshot();
    assert_eq!(snapshot.last_update_id, 648_146_774_652);
    let updates = diffs();
    let mut book = Book::new();
    for update in updates.iter().take(12) {
        assert_eq!(book.update(update.clone()), Applied::Buffered);
    }
    assert!(!book.is_synced());
    // Ten updates end before the snapshot and are dropped; the eleventh straddles it.
    assert_eq!(book.snapshot(&snapshot), Applied::Applied { final_id: updates[11].final_id });
    for update in updates.iter().skip(12) {
        assert_eq!(book.update(update.clone()), Applied::Applied { final_id: update.final_id });
    }
    assert_eq!(book.last_update_id(), updates.last().map(|update| update.final_id));
    let (bid, ask) = (book.best_bid().unwrap(), book.best_ask().unwrap());
    assert!(bid.price < ask.price, "{bid:?} {ask:?}");
    assert!(book.spread().unwrap().is_positive());
    assert!(book.bids().zip(book.bids().skip(1)).all(|(a, b)| a.price > b.price));
    assert!(book.asks().zip(book.asks().skip(1)).all(|(a, b)| a.price < b.price));
    assert!(book.bids().chain(book.asks()).all(|level| level.quantity.is_positive()));
    assert_eq!(book.update(updates[20].clone()), Applied::Stale);
}

fn update(first: u64, last: u64, previous: u64, bids: &[(&str, &str)]) -> DepthUpdate {
    DepthUpdate {
        event_time: 0,
        transaction_time: 0,
        symbol: "BTCUSDT".into(),
        first_id: first,
        final_id: last,
        previous_final_id: previous,
        bids: bids.iter().map(|(price, quantity)| Level { price: d(price), quantity: d(quantity) }).collect(),
        asks: Vec::new(),
    }
}

fn small_snapshot(id: u64) -> Snapshot {
    Snapshot {
        last_update_id: id,
        bids: vec![Level { price: d("100"), quantity: d("1") }, Level { price: d("99"), quantity: d("2") }],
        asks: vec![Level { price: d("101"), quantity: d("1") }],
    }
}

#[test]
fn levels_are_absolute_and_zero_removes_one() {
    let mut book = Book::new();
    book.snapshot(&small_snapshot(10));
    assert_eq!(
        book.update(update(9, 12, 8, &[("100", "3"), ("99", "0"), ("98", "0")])),
        Applied::Applied { final_id: 12 }
    );
    let bids: Vec<Level> = book.bids().collect();
    assert_eq!(bids, vec![Level { price: d("100"), quantity: d("3") }]);
}

#[test]
fn a_broken_chain_empties_the_book_until_a_fresh_snapshot() {
    let mut book = Book::new();
    book.snapshot(&small_snapshot(10));
    book.update(update(10, 12, 9, &[]));
    assert_eq!(book.update(update(15, 16, 14, &[("100", "5")])), Applied::Gap { expected: 12, got: 14 });
    assert!(!book.is_synced() && book.best_bid().is_none());
    assert_eq!(book.buffered(), 1, "the update that broke the chain starts the new buffer");
    assert_eq!(book.update(update(17, 18, 16, &[])), Applied::Buffered);
    assert_eq!(book.snapshot(&small_snapshot(16)), Applied::Applied { final_id: 18 });
    assert_eq!(book.best_bid().map(|level| level.quantity), Some(d("5")));
}

#[test]
fn a_snapshot_older_than_the_buffer_asks_for_another() {
    let mut book = Book::new();
    book.update(update(20, 25, 19, &[]));
    assert_eq!(book.snapshot(&small_snapshot(10)), Applied::Gap { expected: 10, got: 20 });
    assert!(!book.is_synced());
    assert_eq!(book.buffered(), 1, "the buffer is kept for the next snapshot");
    assert_eq!(book.snapshot(&small_snapshot(22)), Applied::Applied { final_id: 25 });
}
