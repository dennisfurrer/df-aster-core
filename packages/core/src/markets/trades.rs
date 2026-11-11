//! Trades on the book: the tape (`trades`), older trades from an id (`historicalTrades`), and
//! aggregate trades (`aggTrades`), where fills of one taker order at one price are summed. Only
//! trades on the book are listed; insurance fund and auto-deleverage trades are not.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::Dec;

/// The most trades one read answers; the venue's default is 500.
pub const MOST_TRADES: u32 = 1_000;

/// One trade.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: u64,
    pub price: Dec,
    pub qty: Dec,
    /// Price times quantity.
    pub quote_qty: Dec,
    /// Unix ms.
    pub time: u64,
    /// The buyer's order was resting: the taker sold.
    pub is_buyer_maker: bool,
}

/// An aggregate trade: one taker order's fills at one price.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AggTrade {
    #[serde(rename = "a")]
    pub id: u64,
    #[serde(rename = "p")]
    pub price: Dec,
    #[serde(rename = "q")]
    pub qty: Dec,
    /// The first and last trade ids it sums.
    #[serde(rename = "f")]
    pub first_trade_id: u64,
    #[serde(rename = "l")]
    pub last_trade_id: u64,
    /// Unix ms.
    #[serde(rename = "T")]
    pub time: u64,
    /// The buyer's order was resting: the taker sold.
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Which aggregate trades to read: from an id, or between two times (inclusive), or the latest
/// when neither is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AggTradeQuery {
    pub from_id: Option<u64>,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    /// At most [`MOST_TRADES`]; the venue's default is 500.
    pub limit: Option<u32>,
}

/// `GET /fapi/v3/trades`.
#[must_use]
pub fn recent_trades_call(symbol: &str, limit: Option<u32>) -> Call {
    Call::get("/fapi/v3/trades").field("symbol", symbol).maybe("limit", limit.map(|limit| limit.min(MOST_TRADES)))
}

/// `symbol`'s latest trades, oldest first.
pub async fn recent_trades(
    transport: &impl Transport,
    symbol: &str,
    limit: Option<u32>,
) -> Result<Vec<Trade>, ApiError> {
    read(transport, recent_trades_call(symbol, limit)).await
}

/// `GET /fapi/v3/historicalTrades`.
#[must_use]
pub fn historical_trades_call(symbol: &str, limit: Option<u32>, from_id: Option<u64>) -> Call {
    Call::get("/fapi/v3/historicalTrades")
        .field("symbol", symbol)
        .maybe("limit", limit.map(|limit| limit.min(MOST_TRADES)))
        .maybe("fromId", from_id)
}

/// `symbol`'s trades from `from_id` on, or the latest when none is given, oldest first.
pub async fn historical_trades(
    transport: &impl Transport,
    symbol: &str,
    limit: Option<u32>,
    from_id: Option<u64>,
) -> Result<Vec<Trade>, ApiError> {
    read(transport, historical_trades_call(symbol, limit, from_id)).await
}

/// `GET /fapi/v3/aggTrades`.
#[must_use]
pub fn agg_trades_call(symbol: &str, query: &AggTradeQuery) -> Call {
    Call::get("/fapi/v3/aggTrades")
        .field("symbol", symbol)
        .maybe("fromId", query.from_id)
        .maybe("startTime", query.start_time)
        .maybe("endTime", query.end_time)
        .maybe("limit", query.limit.map(|limit| limit.min(MOST_TRADES)))
}

/// `symbol`'s aggregate trades, oldest first.
pub async fn agg_trades(
    transport: &impl Transport,
    symbol: &str,
    query: &AggTradeQuery,
) -> Result<Vec<AggTrade>, ApiError> {
    read(transport, agg_trades_call(symbol, query)).await
}

#[cfg(test)]
#[path = "tests/trades.rs"]
mod tests;
