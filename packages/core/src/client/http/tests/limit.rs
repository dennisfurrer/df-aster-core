//! Weights, the minute, the order limits and the gate.

use super::*;

#[test]
fn weights_follow_the_docs() {
    assert_eq!(weight(Method::Post, "/fapi/v3/order"), 0);
    assert_eq!(weight(Method::Get, "/fapi/v3/order"), 1);
    assert_eq!(weight(Method::Post, "/fapi/v3/batchOrders"), 5);
    assert_eq!(weight(Method::Delete, "/fapi/v3/batchOrders"), 1);
    assert_eq!(weight(Method::Get, "/fapi/v3/positionSide/dual"), 30);
    assert_eq!(weight(Method::Post, "/fapi/v3/positionSide/dual"), 1);
    assert_eq!(weight(Method::Get, "/fapi/v3/builder/userTrades"), 5);
    assert_eq!(weight(Method::Post, "/aster-chain/v3/perp/user-withdraw"), 5);
    assert_eq!(weight(Method::Post, "/fapi/v3/registerAndApproveAgent"), 50);
}

#[test]
fn reads_wait_near_four_fifths_of_the_minute() {
    let mut meter = Meter::default();
    for at in 0..96 {
        meter.record(at, "/fapi/v3/klines", 20, 0);
    }
    let spent = meter.spent(100);
    assert_eq!((spent.minute, spent.kinds.len(), spent.budget), (1_920, 1, WEIGHT_BUDGET));
    assert_eq!(meter.wait_for(100, 1, 0), Some(WINDOW_MS - 100));
    assert_eq!(meter.wait_for(WINDOW_MS, 20, 0), None);
}

#[test]
fn the_venues_own_count_holds_reads_back_while_fresh() {
    let mut meter = Meter::default();
    meter.report(1_000, 1_919);
    assert!(meter.wait_for(1_000, 2, 0).is_some());
    assert_eq!(meter.wait_for(1_000 + 5_000, 2, 0), None);
}

#[test]
fn orders_wait_past_three_hundred_in_ten_seconds() {
    let mut meter = Meter::default();
    meter.record(0, "/fapi/v3/batchOrders", 5, 300);
    assert_eq!(meter.spent(5).orders_10s, 300);
    assert_eq!(meter.wait_for(1, 0, 1), Some(10_000));
    assert_eq!(meter.wait_for(10_000, 0, 1), None);
    assert_eq!(meter.spent(10_000).orders_minute, 300);
}

#[test]
fn the_gate_backs_off_and_counts_a_burst_once() {
    let mut gate = Gate::default();
    let first = gate.open(0).unwrap();
    let second = gate.open(0).unwrap();
    assert_eq!(gate.struck(first, 0, None), BASE_COOLDOWN_MS);
    assert_eq!(gate.struck(second, 0, None), BASE_COOLDOWN_MS);
    assert_eq!(gate.open(1_000).unwrap_err(), 1_000);
    let third = gate.open(2_000).unwrap();
    assert_eq!(gate.struck(third, 2_000, None), 2 * BASE_COOLDOWN_MS);
    let fourth = gate.open(10_000).unwrap();
    assert_eq!(gate.banned(fourth, 10_000, None), BAN_MS);
    let fifth = gate.open(10_000 + BAN_MS).unwrap();
    gate.passed(fifth);
    let sixth = gate.open(10_000 + BAN_MS).unwrap();
    assert_eq!(gate.blocked(sixth, 10_000 + BAN_MS), MAX_COOLDOWN_MS);
}
