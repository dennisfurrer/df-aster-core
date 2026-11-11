//! `exchangeInfo` recorded from fapi.asterdex.com on 2026-11-11, cut to seven symbols: BTC, ETH
//! and ASTER perps, a CJK-named one (龙虾USDT), a Hong Kong stock (XIAOMIUSDT), one settling
//! (TONUSDT) and one not yet trading with an empty contract type (MBLUSDT). Every asset and rate
//! limit is kept.

use super::*;
use crate::test_support::{Recorded, block_on};

const INFO: &str = include_str!("../../../fixtures/aster/markets_exchangeInfo.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn the_listing_reads_whole() {
    let transport = Recorded::answering(INFO);
    let info = block_on(exchange_info(&transport)).unwrap();
    assert_eq!((transport.only().path, transport.only().weight), ("/fapi/v3/exchangeInfo", 1));
    assert_eq!((info.timezone.as_str(), info.futures_type.as_str(), info.symbols.len()), ("UTC", "U_MARGINED", 7));
    let weight = &info.rate_limits[0];
    assert_eq!(
        (weight.rate_limit_type.as_str(), weight.limit, weight.window_ms()),
        ("REQUEST_WEIGHT", 2_400, Some(60_000))
    );
    assert_eq!(info.rate_limits[2].window_ms(), Some(10_000));
    let usdt = info.assets.iter().find(|asset| asset.asset == "USDT").unwrap();
    assert!(usdt.margin_available);
    assert_eq!((usdt.auto_asset_exchange, usdt.asset_type.as_str()), (Some(d("-10000")), "stable_coin"));
}

#[test]
fn a_symbol_reads_into_its_grid() {
    let markets: Markets = serde_json::from_str::<ExchangeInfo>(INFO).unwrap().into();
    let btc = markets.get("BTCUSDT").unwrap();
    assert!(btc.is_trading());
    assert_eq!((btc.contract_type.as_str(), btc.margin_asset.as_str()), ("PERPETUAL", "USDT"));
    let grid = btc.grid();
    assert!(grid.tick.is_positive() && grid.step.is_positive() && grid.min_notional.is_positive());
    assert!(grid.market_max_qty.is_positive() && grid.market_max_qty <= grid.max_qty);
    assert_eq!(btc.max_orders(), Some(200));
    assert!(btc.max_algo_orders().is_some() && btc.percent_price().is_some());
    assert!(btc.takes(&crate::utils::TimeInForce::Gtx) && !btc.takes(&crate::utils::TimeInForce::Fok));
    assert!(btc.order_types.contains(&crate::utils::OrderType::TrailingStopMarket));
    assert!(btc.trigger_protect.is_positive() && btc.liquidation_fee.is_positive());

    let aster = markets.get("ASTERUSDT").unwrap().grid();
    assert_eq!((aster.tick, aster.step, aster.min_notional), (d("0.0001"), d("0.01"), d("5")));
    let price = aster.price(d("0.123456"), crate::utils::Round::Down).unwrap();
    assert_eq!(price, d("0.1234"));
}

#[test]
fn every_kind_of_symbol_reads() {
    let markets: Markets = serde_json::from_str::<ExchangeInfo>(INFO).unwrap().into();
    assert_eq!(markets.len(), 7);
    let pending = markets.get("MBLUSDT").unwrap();
    assert_eq!(
        (pending.status.clone(), pending.contract_type.as_str(), pending.imn),
        (ContractStatus::PendingTrading, "", None)
    );
    assert_eq!(markets.get("TONUSDT").unwrap().status, ContractStatus::Settling);
    let stock = markets.get("XIAOMIUSDT").unwrap();
    assert_eq!(stock.channel, "hkstock");
    assert!(!stock.tags.is_empty());
    assert!(markets.get("龙虾USDT").is_some());
    assert_eq!(markets.trading().count(), markets.all().iter().filter(|symbol| symbol.is_trading()).count());
    assert!(markets.trading().all(|symbol| symbol.status == ContractStatus::Trading));
    assert!(markets.get("btcusdt").is_none());
}

#[test]
fn an_unknown_filter_is_kept_as_other() {
    let filter: Filter = serde_json::from_str(r#"{"filterType":"SOMETHING_NEW","x":"1"}"#).unwrap();
    assert_eq!(filter, Filter::Other);
    let docs: Filter = serde_json::from_str(
        r#"{"filterType":"PERCENT_PRICE","multiplierUp":"1.1500","multiplierDown":"0.8500","multiplierDecimal":4}"#,
    )
    .unwrap();
    assert!(matches!(docs, Filter::PercentPrice { multiplier_decimal: 4, lt_multiplier_up: None, .. }));
}
