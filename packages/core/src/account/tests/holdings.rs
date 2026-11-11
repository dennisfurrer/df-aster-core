//! Each holdings read sends the right call, and every row reads from the docs' example answers
//! (fixtures/aster/account_*.json, from the docs: no account was read live).

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const BALANCE: &str = include_str!("../../../fixtures/aster/account_balance.json");
const ACCOUNT: &str = include_str!("../../../fixtures/aster/account_account.json");
const POSITIONS: &str = include_str!("../../../fixtures/aster/account_position_risk.json");
const COMMISSION: &str = include_str!("../../../fixtures/aster/account_commission_rate.json");
const BRACKETS: &str = include_str!("../../../fixtures/aster/account_leverage_brackets.json");
const ADL: &str = include_str!("../../../fixtures/aster/account_adl_quantile.json");
const REMAINING: &str = include_str!("../../../fixtures/aster/account_remaining_notional.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn fields(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs.iter().map(|(key, value)| (*key, (*value).to_owned())).collect()
}

#[test]
fn balances_read_each_asset() {
    let transport = Recorded::answering(BALANCE);
    let rows = block_on(balances(&transport)).unwrap();
    let call = transport.only();
    assert_eq!((call.method, call.path, call.signed, call.weight), (Method::Get, "/fapi/v3/balance", true, 5));
    assert!(call.fields.is_empty());
    let usdt = &rows[0];
    assert_eq!((usdt.asset.as_str(), usdt.account_alias.as_str()), ("USDT", "SgsR"));
    assert_eq!((usdt.balance, usdt.available_balance), (d("122607.35137903"), d("23.72469206")));
    assert!(usdt.margin_available);
    assert_eq!(usdt.update_time, 1_617_939_110_373);
}

#[test]
fn the_account_reads_its_totals_assets_and_positions() {
    for (path, read_it) in [("/fapi/v3/account", false), ("/fapi/v3/accountWithJoinMargin", true)] {
        let transport = Recorded::answering(ACCOUNT);
        let info = match read_it {
            false => block_on(account(&transport)),
            true => block_on(account_with_join_margin(&transport)),
        }
        .unwrap();
        let call = transport.only();
        assert_eq!((call.path, call.signed, call.weight), (path, true, 5));
        assert!(info.can_trade && info.can_deposit && info.can_withdraw);
        assert_eq!(info.total_wallet_balance, d("23.72469206"));
        assert_eq!(info.asset("BUSD").unwrap().wallet_balance, d("103.12345678"));
        assert_eq!(info.positions[0].leverage, 100);
        assert!(info.positions[0].isolated);
        assert_eq!(info.positions[0].max_notional, d("250000"));
        assert_eq!(info.open_positions().count(), 0);
    }
}

#[test]
fn positions_read_both_sides_of_a_hedge() {
    let transport = Recorded::answering(POSITIONS);
    let rows = block_on(positions(&transport, Some("BTCUSDT"))).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.path, call.signed, call.fields.clone()),
        ("/fapi/v3/positionRisk", true, fields(&[("symbol", "BTCUSDT")]))
    );
    let (long, short) = (&rows[0], &rows[1]);
    assert_eq!((long.position_side.clone(), long.position_amt, long.leverage), (PositionSide::Long, d("20"), 10));
    assert_eq!((long.margin_type.clone(), long.is_auto_add_margin), (MarginType::Isolated, false));
    assert_eq!((long.unrealized_profit, long.liquidation_price), (d("2316.8342356"), d("5930.78")));
    assert_eq!((short.position_side.clone(), short.position_amt), (PositionSide::Short, d("-10")));
    assert!(long.is_open() && short.is_open());
    let every = Recorded::answering("[]");
    block_on(positions(&every, None)).unwrap();
    assert!(every.only().fields.is_empty());
}

#[test]
fn the_commission_rate_is_a_fraction() {
    let transport = Recorded::answering(COMMISSION);
    let rate = block_on(commission_rate(&transport, "BTCUSDT")).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.signed, call.weight), ("/fapi/v3/commissionRate", true, 20));
    assert_eq!((rate.maker_commission_rate, rate.taker_commission_rate), (d("0.0002"), d("0.0004")));
}

#[test]
fn brackets_read_as_a_list_or_as_one() {
    let transport = Recorded::answering(BRACKETS);
    let all = block_on(leverage_brackets(&transport, None)).unwrap();
    assert_eq!((transport.only().path, transport.only().signed), ("/fapi/v3/leverageBrackets", true));
    let eth = &all[0];
    let first = &eth.brackets[0];
    assert_eq!((first.initial_leverage, first.notional_cap, first.maint_margin_ratio), (75, d("10000"), d("0.0065")));
    assert_eq!(eth.max_leverage(), 75);
    assert_eq!(eth.for_notional(d("-500")).map(|b| b.bracket), Some(1));
    assert!(eth.for_notional(d("10000")).is_none());
    let one = serde_json::to_string(&serde_json::from_str::<serde_json::Value>(BRACKETS).unwrap()[0]).unwrap();
    let transport = Recorded::answering(&one);
    let single = block_on(leverage_brackets(&transport, Some("ETHUSDT"))).unwrap();
    assert_eq!((single.len(), transport.only().fields.clone()), (1, fields(&[("symbol", "ETHUSDT")])));
}

#[test]
fn adl_quantiles_keep_each_side() {
    let transport = Recorded::answering(ADL);
    let rows = block_on(adl_quantiles(&transport, None)).unwrap();
    assert_eq!((transport.only().path, transport.only().weight), ("/fapi/v3/adlQuantile", 5));
    assert_eq!(rows[0].adl_quantile, AdlQuantile { long: Some(3), short: Some(3), both: None, hedge: Some(0) });
    assert_eq!(rows[1].adl_quantile, AdlQuantile { long: Some(1), short: Some(2), both: Some(0), hedge: None });
}

#[test]
fn the_open_interest_cap_is_public_and_minus_one_is_none() {
    let transport = Recorded::answering(REMAINING);
    let cap = block_on(remaining_openable_notional(&transport, "BTCUSDT", 20)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.signed, call.weight), ("/fapi/v3/remainingOpenableNotionalValue", false, 50));
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("leverage", "20")]));
    assert_eq!(cap, Some(d("200000")));
    let none = Recorded::answering(r#"{"remainingOpenableNotionalValue":"-1"}"#);
    assert_eq!(block_on(remaining_openable_notional(&none, "BTCUSDT", 20)).unwrap(), None);
}
