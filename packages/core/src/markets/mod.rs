//! What is listed and what it trades at: every public read of Aster's futures API, as plain rows
//! and free functions over any [`crate::client::Transport`]. Nothing is signed and nothing is
//! cached here; a program keeps what it read.
//!
//! - [`system`]: connectivity (`ping`), the venue's clock (`time`) and every asset's logo
//!   (`common/asset/all-asset-logo`).
//! - [`info`]: the listing (`exchangeInfo`): each symbol with its filters, read into a
//!   [`crate::utils::Grid`] by [`Symbol::grid`], the margin assets and the rate limits, and a
//!   [`Markets`] snapshot looked up by symbol.
//! - [`book`]: a symbol's order book (`depth`).
//! - [`trades`](mod@trades): the tape (`trades`), older trades by id (`historicalTrades`) and
//!   aggregate trades (`aggTrades`).
//! - [`klines`](mod@klines): trade candles (`klines`), index price candles (`indexPriceKlines`) and mark price
//!   candles (`markPriceKlines`).
//! - [`prices`]: mark and index prices with the current funding rate (`premiumIndex`), and the
//!   exchanges behind a symbol's index price (`indexreferences`).
//! - [`funding`]: settled funding rates (`fundingRate`) and each symbol's funding interval, cap
//!   and floor (`fundingInfo`).
//! - [`tickers`]: the day's statistics (`ticker/24hr`), the last price (`ticker/price`) and the
//!   best bid and ask (`ticker/bookTicker`), for one symbol or all.
//!
//! The venue names a market by its symbol (`BTCUSDT`). Prices and quantities are exact decimals.
//! A few figures come as bare JSON numbers (`fundingInfo`'s cap and floor); they are read through
//! their shortest decimal spelling, never through arithmetic on a float.

pub mod book;
pub mod funding;
pub mod info;
pub mod klines;
pub mod prices;
pub mod system;
pub mod tickers;
pub mod trades;

pub use book::{Depth, DepthLimit, Level, depth, depth_call};
pub use funding::{
    FundingConfig, FundingQuery, FundingRate, MOST_FUNDING_RATES, funding_info, funding_info_call, funding_rates,
    funding_rates_call,
};
pub use info::{
    AssetInfo, ExchangeInfo, Filter, Markets, RateLimit, Symbol, exchange_info, exchange_info_call, markets,
};
pub use klines::{
    Kline, KlineQuery, MOST_KLINES, PriceKline, index_price_klines, index_price_klines_call, kline_weight, klines,
    klines_call, mark_price_klines, mark_price_klines_call,
};
pub use prices::{
    IndexReference, IndexReferences, MarkPrice, index_references, index_references_call, mark_price, mark_price_call,
    mark_prices, mark_prices_call,
};
pub use system::{
    AssetLogo, ServerTime, asset_logos, asset_logos_call, ping, ping_call, server_time, server_time_call,
};
pub use tickers::{
    ALL_TICKERS_24HR_WEIGHT, ALL_TICKERS_WEIGHT, BookTicker, PriceTicker, Ticker24h, book_ticker, book_ticker_call,
    book_tickers, book_tickers_call, last_price, last_price_call, last_prices, last_prices_call, ticker_24hr,
    ticker_24hr_call, tickers_24hr, tickers_24hr_call,
};
pub use trades::{
    AggTrade, AggTradeQuery, MOST_TRADES, Trade, agg_trades, agg_trades_call, historical_trades,
    historical_trades_call, recent_trades, recent_trades_call,
};

use serde::{Deserialize, Deserializer};

use crate::utils::Dec;

/// A figure the venue writes as a bare JSON number (`0.003`, `-0.02`, or a string in places),
/// read exactly through its shortest decimal spelling. An exponent (`1e-7`) is moved into the
/// decimal by shifting, never by float arithmetic.
pub(crate) fn exact<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    let text = match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => text,
        serde_json::Value::Null => return Ok(Dec::ZERO),
        other => return Err(serde::de::Error::custom(format!("not a number: {other}"))),
    };
    scientific(&text).ok_or_else(|| serde::de::Error::custom(format!("not a decimal: {text}")))
}

/// A decimal in plain or scientific spelling.
pub(crate) fn scientific(text: &str) -> Option<Dec> {
    let text = text.trim();
    let Some((mantissa, exponent)) = text.split_once(['e', 'E']) else {
        return Dec::parse(text).ok();
    };
    let mantissa = Dec::parse(mantissa).ok()?;
    let exponent: i32 = exponent.parse().ok()?;
    match exponent < 0 {
        true => mantissa.checked_mul(Dec::new(1, exponent.unsigned_abs())?),
        false => {
            let power = 10i64.checked_pow(exponent.unsigned_abs())?;
            mantissa.checked_mul(Dec::from_int(power))
        }
    }
}

#[cfg(test)]
#[path = "tests/exact.rs"]
mod tests;
