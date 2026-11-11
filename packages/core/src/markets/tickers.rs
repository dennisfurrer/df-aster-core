//! Tickers, for one symbol or every one: the rolling day's statistics (`ticker/24hr`), the last
//! price (`ticker/price`) and the best bid and ask (`ticker/bookTicker`). Reading every symbol
//! weighs more: 40 for the day's statistics, 2 for the others.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::Dec;
use crate::utils::lenient;

/// The weight of `ticker/24hr` for every symbol.
pub const ALL_TICKERS_24HR_WEIGHT: u32 = 40;

/// The weight of `ticker/price` and `ticker/bookTicker` for every symbol.
pub const ALL_TICKERS_WEIGHT: u32 = 2;

/// A symbol's rolling day.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker24h {
    pub symbol: String,
    pub price_change: Dec,
    /// In percent: `-0.583` is -0.583%.
    pub price_change_percent: Dec,
    pub weighted_avg_price: Dec,
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub prev_close_price: Option<Dec>,
    pub last_price: Dec,
    pub last_qty: Dec,
    pub open_price: Dec,
    pub high_price: Dec,
    pub low_price: Dec,
    /// Base traded.
    pub volume: Dec,
    /// Quote traded.
    pub quote_volume: Dec,
    /// Unix ms.
    pub open_time: u64,
    pub close_time: u64,
    /// The first and last trade ids, and the count.
    #[serde(default, deserialize_with = "lenient::int")]
    pub first_id: i64,
    #[serde(default, deserialize_with = "lenient::int")]
    pub last_id: i64,
    #[serde(default)]
    pub count: u64,
}

/// A symbol's last price.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PriceTicker {
    pub symbol: String,
    pub price: Dec,
    /// Unix ms.
    #[serde(default)]
    pub time: u64,
}

/// A symbol's best bid and ask.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTicker {
    pub symbol: String,
    pub bid_price: Dec,
    pub bid_qty: Dec,
    pub ask_price: Dec,
    pub ask_qty: Dec,
    /// Unix ms.
    #[serde(default)]
    pub time: u64,
    /// The book update it stands at; not in the docs, sent by the venue.
    #[serde(default)]
    pub last_update_id: Option<u64>,
}

/// `GET /fapi/v3/ticker/24hr` for one symbol.
#[must_use]
pub fn ticker_24hr_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/ticker/24hr").field("symbol", symbol)
}

/// `symbol`'s rolling day.
pub async fn ticker_24hr(transport: &impl Transport, symbol: &str) -> Result<Ticker24h, ApiError> {
    read(transport, ticker_24hr_call(symbol)).await
}

/// `GET /fapi/v3/ticker/24hr` for every symbol: weight 40.
#[must_use]
pub fn tickers_24hr_call() -> Call {
    Call::get("/fapi/v3/ticker/24hr").weigh(ALL_TICKERS_24HR_WEIGHT)
}

/// Every symbol's rolling day.
pub async fn tickers_24hr(transport: &impl Transport) -> Result<Vec<Ticker24h>, ApiError> {
    read(transport, tickers_24hr_call()).await
}

/// `GET /fapi/v3/ticker/price` for one symbol.
#[must_use]
pub fn last_price_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/ticker/price").field("symbol", symbol)
}

/// `symbol`'s last price.
pub async fn last_price(transport: &impl Transport, symbol: &str) -> Result<PriceTicker, ApiError> {
    read(transport, last_price_call(symbol)).await
}

/// `GET /fapi/v3/ticker/price` for every symbol: weight 2.
#[must_use]
pub fn last_prices_call() -> Call {
    Call::get("/fapi/v3/ticker/price").weigh(ALL_TICKERS_WEIGHT)
}

/// Every symbol's last price.
pub async fn last_prices(transport: &impl Transport) -> Result<Vec<PriceTicker>, ApiError> {
    read(transport, last_prices_call()).await
}

/// `GET /fapi/v3/ticker/bookTicker` for one symbol.
#[must_use]
pub fn book_ticker_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/ticker/bookTicker").field("symbol", symbol)
}

/// `symbol`'s best bid and ask.
pub async fn book_ticker(transport: &impl Transport, symbol: &str) -> Result<BookTicker, ApiError> {
    read(transport, book_ticker_call(symbol)).await
}

/// `GET /fapi/v3/ticker/bookTicker` for every symbol: weight 2.
#[must_use]
pub fn book_tickers_call() -> Call {
    Call::get("/fapi/v3/ticker/bookTicker").weigh(ALL_TICKERS_WEIGHT)
}

/// Every symbol's best bid and ask.
pub async fn book_tickers(transport: &impl Transport) -> Result<Vec<BookTicker>, ApiError> {
    read(transport, book_tickers_call()).await
}

#[cfg(test)]
#[path = "tests/tickers.rs"]
mod tests;
