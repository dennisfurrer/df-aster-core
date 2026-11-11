//! A symbol's order book (`depth`): price levels on each side, best first, and the update id a
//! kept book is chained from (see [`crate::live`]).

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::Dec;

/// How many levels a side may hold; the venue takes no other count. The deeper, the heavier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DepthLimit {
    L5,
    L10,
    L20,
    L50,
    L100,
    /// The venue's default.
    #[default]
    L500,
    L1000,
}

impl DepthLimit {
    pub const ALL: [DepthLimit; 7] = [
        DepthLimit::L5,
        DepthLimit::L10,
        DepthLimit::L20,
        DepthLimit::L50,
        DepthLimit::L100,
        DepthLimit::L500,
        DepthLimit::L1000,
    ];

    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            DepthLimit::L5 => 5,
            DepthLimit::L10 => 10,
            DepthLimit::L20 => 20,
            DepthLimit::L50 => 50,
            DepthLimit::L100 => 100,
            DepthLimit::L500 => 500,
            DepthLimit::L1000 => 1_000,
        }
    }

    /// The request weight, from the docs: 2 up to 50 levels, 5 for 100, 10 for 500, 20 for 1000.
    #[must_use]
    pub const fn weight(self) -> u32 {
        match self {
            DepthLimit::L5 | DepthLimit::L10 | DepthLimit::L20 | DepthLimit::L50 => 2,
            DepthLimit::L100 => 5,
            DepthLimit::L500 => 10,
            DepthLimit::L1000 => 20,
        }
    }

    /// The smallest limit that holds `levels` levels a side; 1000 past that.
    #[must_use]
    pub fn at_least(levels: u32) -> DepthLimit {
        DepthLimit::ALL.into_iter().find(|limit| limit.count() >= levels).unwrap_or(DepthLimit::L1000)
    }
}

/// One price level: a price and the quantity resting at it. Written by the venue as
/// `["price", "qty"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(from = "(Dec, Dec)")]
pub struct Level {
    pub price: Dec,
    pub qty: Dec,
}

impl From<(Dec, Dec)> for Level {
    fn from((price, qty): (Dec, Dec)) -> Self {
        Level { price, qty }
    }
}

/// A symbol's book.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Depth {
    /// The update this snapshot stands at: a kept book applies the diffs after it.
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    /// When the venue wrote the answer, unix ms.
    #[serde(rename = "E", default)]
    pub message_time: u64,
    /// The time of the last trade or change in it, unix ms.
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    /// Highest price first.
    pub bids: Vec<Level>,
    /// Lowest price first.
    pub asks: Vec<Level>,
}

impl Depth {
    #[must_use]
    pub fn best_bid(&self) -> Option<Level> {
        self.bids.first().copied()
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Level> {
        self.asks.first().copied()
    }

    /// The ask less the bid, when both sides have a level.
    #[must_use]
    pub fn spread(&self) -> Option<Dec> {
        self.best_ask()?.price.checked_sub(self.best_bid()?.price)
    }
}

/// `GET /fapi/v3/depth`, weighted by `limit`.
#[must_use]
pub fn depth_call(symbol: &str, limit: DepthLimit) -> Call {
    Call::get("/fapi/v3/depth").field("symbol", symbol).field("limit", limit.count()).weigh(limit.weight())
}

/// `symbol`'s book, `limit` levels a side.
pub async fn depth(transport: &impl Transport, symbol: &str, limit: DepthLimit) -> Result<Depth, ApiError> {
    read(transport, depth_call(symbol, limit)).await
}

#[cfg(test)]
#[path = "tests/book.rs"]
mod tests;
