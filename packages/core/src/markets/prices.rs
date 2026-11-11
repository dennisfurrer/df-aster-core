//! Mark and index prices with the current funding rate (`premiumIndex`), and the exchanges and
//! weights a symbol's index price is made from (`indexreferences`).

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::Dec;
use crate::utils::lenient;

/// A symbol's mark price and funding.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkPrice {
    pub symbol: String,
    pub mark_price: Dec,
    pub index_price: Dec,
    /// Only meaningful in the last hour before a settlement.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub estimated_settle_price: Dec,
    /// The rate for the current funding interval, a fraction.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub last_funding_rate: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub interest_rate: Dec,
    /// Unix ms.
    #[serde(default)]
    pub next_funding_time: u64,
    /// Unix ms.
    #[serde(default)]
    pub time: u64,
}

/// One exchange a symbol's index price is taken from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IndexReference {
    pub exchange: String,
    /// The symbol there (`BTC-USDT`).
    pub symbol: String,
    /// Its share of the index, a fraction.
    pub weight: Dec,
}

/// A symbol's index components.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IndexReferences {
    pub symbol: String,
    /// Unix ms.
    #[serde(default)]
    pub time: u64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub references: Vec<IndexReference>,
}

/// `GET /fapi/v3/premiumIndex` for one symbol.
#[must_use]
pub fn mark_price_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/premiumIndex").field("symbol", symbol)
}

/// `symbol`'s mark price and funding.
pub async fn mark_price(transport: &impl Transport, symbol: &str) -> Result<MarkPrice, ApiError> {
    read(transport, mark_price_call(symbol)).await
}

/// `GET /fapi/v3/premiumIndex` for every symbol.
#[must_use]
pub fn mark_prices_call() -> Call {
    Call::get("/fapi/v3/premiumIndex")
}

/// Every symbol's mark price and funding.
pub async fn mark_prices(transport: &impl Transport) -> Result<Vec<MarkPrice>, ApiError> {
    read(transport, mark_prices_call()).await
}

/// `GET /fapi/v3/indexreferences`.
#[must_use]
pub fn index_references_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/indexreferences").field("symbol", symbol)
}

/// The exchanges and weights `symbol`'s index price is made from.
pub async fn index_references(transport: &impl Transport, symbol: &str) -> Result<IndexReferences, ApiError> {
    read(transport, index_references_call(symbol)).await
}

#[cfg(test)]
#[path = "tests/prices.rs"]
mod tests;
