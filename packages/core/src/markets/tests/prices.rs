//! BTCUSDT's mark price, three symbols' from the every-symbol answer, and BTCUSDT's index
//! components, recorded from fapi.asterdex.com on 2026-11-11.

use super::*;
use crate::test_support::{Recorded, block_on};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn mark_prices_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_premiumIndex_btcusdt.json"));
    let btc = block_on(mark_price(&transport, "BTCUSDT")).unwrap();
    assert_eq!((transport.only().path, transport.only().value("symbol")), ("/fapi/v3/premiumIndex", Some("BTCUSDT")));
    assert_eq!(
        (btc.mark_price, btc.index_price, btc.last_funding_rate),
        (d("73877.80157609"), d("73911.68304348"), d("0.00006708"))
    );
    assert_eq!(
        (btc.next_funding_time, btc.time, btc.interest_rate),
        (1_794_412_800_000, 1_794_400_050_001, d("0.0001"))
    );

    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_premiumIndex_some.json"));
    let all = block_on(mark_prices(&transport)).unwrap();
    assert!(transport.only().fields.is_empty());
    assert_eq!(all.len(), 3);
    assert!(all.iter().all(|price| price.mark_price.is_positive()));
}

#[test]
fn index_references_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_indexreferences_btcusdt.json"));
    let index = block_on(index_references(&transport, "BTCUSDT")).unwrap();
    assert_eq!(transport.only().path, "/fapi/v3/indexreferences");
    assert_eq!((index.symbol.as_str(), index.references.len()), ("BTCUSDT", 8));
    assert_eq!((index.references[1].exchange.as_str(), index.references[1].symbol.as_str()), ("okex", "BTC-USDT"));
    let total = Dec::checked_sum(index.references.iter().map(|reference| reference.weight)).unwrap();
    assert!(total > d("0.99") && total <= d("1.01"), "{total}");
}
