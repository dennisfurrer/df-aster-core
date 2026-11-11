//! BTCUSDT's last three settlements, and the funding rules of four symbols (every symbol's
//! answer cut down) and of BTCUSDT alone, recorded from fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn settlements_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_fundingRate_btcusdt.json"));
    let query = FundingQuery { limit: Some(5_000), ..FundingQuery::of("BTCUSDT") };
    let rates = block_on(funding_rates(&transport, &query)).unwrap();
    let call = transport.only();
    assert_eq!(call.path, "/fapi/v3/fundingRate");
    assert_eq!(call.fields, vec![("symbol", "BTCUSDT".to_owned()), ("limit", "1000".to_owned())]);
    assert_eq!(rates.len(), 3);
    assert_eq!((rates[1].funding_rate, rates[1].funding_time), (d("0.00007016"), 1_794_355_200_000));
    assert!(funding_rates_call(&FundingQuery::default()).fields.is_empty());
}

#[test]
fn funding_rules_read_their_bare_numbers_exactly() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_fundingInfo.json"));
    let rules = block_on(funding_info(&transport, None)).unwrap();
    assert!(transport.only().fields.is_empty());
    let zora = rules.iter().find(|rule| rule.symbol == "ZORAUSDT").unwrap();
    assert_eq!((zora.funding_interval_hours, zora.interval_ms()), (4, 4 * 3_600_000));
    assert_eq!((zora.funding_fee_cap, zora.funding_fee_floor), (d("0.02"), d("-0.02")));

    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_fundingInfo_btcusdt.json"));
    let btc = block_on(funding_info(&transport, Some("BTCUSDT"))).unwrap();
    assert_eq!(transport.only().value("symbol"), Some("BTCUSDT"));
    assert_eq!((btc.len(), btc[0].funding_fee_cap, btc[0].funding_fee_floor), (1, d("0.003"), d("-0.003")));
    assert_eq!(btc[0].interest_rate, d("0.0001"));
}
