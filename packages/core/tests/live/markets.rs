//! Every public market read, once, against its recording of 2026-11-11, then read into its type.

use df_aster_core::Client;
use df_aster_core::client::native::{Native, clock};
use df_aster_core::client::{Call, Host, Transport};
use df_aster_core::markets::book::{DepthLimit, depth};
use df_aster_core::markets::funding::{FundingQuery, funding_info, funding_rates};
use df_aster_core::markets::info::markets;
use df_aster_core::markets::klines::{KlineQuery, klines, mark_price_klines};
use df_aster_core::markets::prices::{index_references, mark_price, mark_prices};
use df_aster_core::markets::system::{ping, server_time};
use df_aster_core::markets::tickers::{book_ticker, last_price, ticker_24hr};
use df_aster_core::markets::trades::{AggTradeQuery, agg_trades, recent_trades};
use df_aster_core::utils::Interval;
use serde_json::Value;

use crate::shape::holds;

const BTC: &str = "BTCUSDT";

/// Each read's call and its recording.
fn reads() -> Vec<(Call, &'static str)> {
    let symbol = |call: Call| call.field("symbol", BTC);
    vec![
        (Call::get("/fapi/v3/time"), include_str!("../../fixtures/aster/markets_time.json")),
        (Call::get("/fapi/v3/exchangeInfo"), include_str!("../../fixtures/aster/markets_exchangeInfo.json")),
        (
            symbol(Call::get("/fapi/v3/depth")).field("limit", 5),
            include_str!("../../fixtures/aster/markets_depth_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/trades")).field("limit", 5),
            include_str!("../../fixtures/aster/markets_trades_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/aggTrades")).field("limit", 5),
            include_str!("../../fixtures/aster/markets_aggTrades_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/klines")).field("interval", "1h").field("limit", 3),
            include_str!("../../fixtures/aster/markets_klines_btcusdt_1h.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/premiumIndex")),
            include_str!("../../fixtures/aster/markets_premiumIndex_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/fundingRate")).field("limit", 3),
            include_str!("../../fixtures/aster/markets_fundingRate_btcusdt.json"),
        ),
        (Call::get("/fapi/v3/fundingInfo"), include_str!("../../fixtures/aster/markets_fundingInfo.json")),
        (
            symbol(Call::get("/fapi/v3/ticker/24hr")),
            include_str!("../../fixtures/aster/markets_ticker24hr_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/ticker/price")),
            include_str!("../../fixtures/aster/markets_tickerPrice_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/ticker/bookTicker")),
            include_str!("../../fixtures/aster/markets_bookTicker_btcusdt.json"),
        ),
        (
            symbol(Call::get("/fapi/v3/indexreferences")),
            include_str!("../../fixtures/aster/markets_indexreferences_btcusdt.json"),
        ),
        (
            Call::get("/aster-chain/v3/withdraw/estimateFee")
                .on(Host::Chain)
                .field("chainId", 56)
                .field("asset", "USDT"),
            include_str!("../../fixtures/aster/transfer_chain_estimate_fee.json"),
        ),
    ]
}

#[tokio::test]
#[ignore = "reads Aster"]
async fn every_public_answer_keeps_its_recorded_shape() {
    let client = Client::reader(Native::new(), clock);
    let mut broken = Vec::new();
    for (call, recorded) in reads() {
        let path = call.path;
        let text = client.call(call).await.unwrap_or_else(|error| panic!("{path}: {error}"));
        let live: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let recorded: Value = serde_json::from_str(recorded).unwrap_or_else(|error| panic!("{path}: {error}"));
        if let Err(why) = holds(&recorded, &live, path) {
            broken.push(why);
        }
    }
    assert!(broken.is_empty(), "{broken:#?}");
}

#[tokio::test]
#[ignore = "reads Aster"]
async fn every_public_read_reads_into_its_type() {
    let client = Client::reader(Native::new(), clock);
    ping(&client).await.unwrap();
    let now = server_time(&client).await.unwrap();
    assert!(now > 1_790_000_000_000);
    let listed = markets(&client).await.unwrap();
    let btc = listed.get(BTC).unwrap();
    assert!(btc.grid().tick.is_positive() && btc.grid().step.is_positive());
    let book = depth(&client, BTC, DepthLimit::at_least(5)).await.unwrap();
    assert!(book.best_bid().unwrap().price < book.best_ask().unwrap().price);
    assert!(!recent_trades(&client, BTC, Some(5)).await.unwrap().is_empty());
    let query = AggTradeQuery { limit: Some(5), ..AggTradeQuery::default() };
    assert!(!agg_trades(&client, BTC, &query).await.unwrap().is_empty());
    assert_eq!(klines(&client, BTC, &KlineQuery::latest(Interval::H1).limit(3)).await.unwrap().len(), 3);
    assert_eq!(mark_price_klines(&client, BTC, &KlineQuery::latest(Interval::H1).limit(3)).await.unwrap().len(), 3);
    assert!(mark_price(&client, BTC).await.unwrap().mark_price.is_positive());
    assert!(mark_prices(&client).await.unwrap().len() > 100);
    assert!(!funding_rates(&client, &FundingQuery::of(BTC)).await.unwrap().is_empty());
    assert!(!funding_info(&client, None).await.unwrap().is_empty());
    assert!(ticker_24hr(&client, BTC).await.unwrap().last_price.is_positive());
    assert!(last_price(&client, BTC).await.unwrap().price.is_positive());
    assert!(book_ticker(&client, BTC).await.unwrap().bid_price.is_positive());
    index_references(&client, BTC).await.unwrap();
    let spent = client.spent();
    assert!(spent.minute < spent.budget / 4, "{spent:?}");
}
