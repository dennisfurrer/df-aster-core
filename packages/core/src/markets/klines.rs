//! Candles: trade candles (`klines`), index price candles (`indexPriceKlines`, by pair) and mark
//! price candles (`markPriceKlines`). Each is identified by its open time; the last one is still
//! open. The venue writes a candle as an array, prices as strings and times and counts as
//! integers.

use serde::{Deserialize, Deserializer};

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::{Dec, Interval};

/// The most candles one read answers; the venue's default is 500.
pub const MOST_KLINES: u32 = 1_500;

/// What to read: a length, and a range or the latest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KlineQuery {
    pub interval: Interval,
    /// Unix ms, inclusive. Neither: the latest candles.
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    /// At most [`MOST_KLINES`]; the venue's default is 500.
    pub limit: Option<u32>,
}

impl KlineQuery {
    /// The latest candles of `interval`, the venue's default count.
    #[must_use]
    pub const fn latest(interval: Interval) -> Self {
        KlineQuery { interval, start_time: None, end_time: None, limit: None }
    }

    #[must_use]
    pub const fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Candles that open from `start` to `end`, unix ms.
    #[must_use]
    pub const fn between(mut self, start: u64, end: u64) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }
}

/// A candle read's weight, from the docs: 1 below 100 candles, 2 below 500, 5 to 1000, 10 above.
/// No limit is the venue's default, 500.
#[must_use]
pub const fn kline_weight(limit: Option<u32>) -> u32 {
    match limit {
        Some(0..=99) => 1,
        Some(100..=499) => 2,
        None | Some(500..=1_000) => 5,
        Some(_) => 10,
    }
}

/// A trade candle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kline {
    /// Unix ms.
    pub open_time: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
    /// Base traded.
    pub volume: Dec,
    /// The candle's last millisecond.
    pub close_time: u64,
    /// Quote traded.
    pub quote_volume: Dec,
    pub trades: u64,
    /// What takers bought, in base and in quote.
    pub taker_buy_volume: Dec,
    pub taker_buy_quote_volume: Dec,
}

/// An index or mark price candle: prices only, and how many prices it was made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceKline {
    pub open_time: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
    pub close_time: u64,
    /// The prices sampled into it.
    pub points: u64,
}

/// A candle's array, its cells read by position.
struct Cells(Vec<serde_json::Value>);

impl Cells {
    fn read<'de, D: Deserializer<'de>>(deserializer: D, least: usize) -> Result<Cells, D::Error> {
        let cells = Vec::<serde_json::Value>::deserialize(deserializer)?;
        match cells.len() >= least {
            true => Ok(Cells(cells)),
            false => Err(serde::de::Error::custom(format!("a candle with {} cells, not {least}", cells.len()))),
        }
    }

    fn dec<E: serde::de::Error>(&self, at: usize) -> Result<Dec, E> {
        match self.0.get(at) {
            Some(serde_json::Value::String(text)) => Dec::parse(text.trim()).map_err(E::custom),
            Some(serde_json::Value::Number(number)) if number.is_i64() || number.is_u64() => {
                Dec::parse(&number.to_string()).map_err(E::custom)
            }
            other => Err(E::custom(format!("cell {at} is not a decimal: {other:?}"))),
        }
    }

    fn int<E: serde::de::Error>(&self, at: usize) -> Result<u64, E> {
        match self.0.get(at) {
            Some(serde_json::Value::Number(number)) => {
                number.as_u64().ok_or_else(|| E::custom(format!("cell {at} is not a whole number")))
            }
            Some(serde_json::Value::String(text)) => text.trim().parse().map_err(E::custom),
            other => Err(E::custom(format!("cell {at} is not a whole number: {other:?}"))),
        }
    }
}

impl<'de> Deserialize<'de> for Kline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let cells = Cells::read(deserializer, 11)?;
        Ok(Kline {
            open_time: cells.int(0)?,
            open: cells.dec(1)?,
            high: cells.dec(2)?,
            low: cells.dec(3)?,
            close: cells.dec(4)?,
            volume: cells.dec(5)?,
            close_time: cells.int(6)?,
            quote_volume: cells.dec(7)?,
            trades: cells.int(8)?,
            taker_buy_volume: cells.dec(9)?,
            taker_buy_quote_volume: cells.dec(10)?,
        })
    }
}

impl<'de> Deserialize<'de> for PriceKline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let cells = Cells::read(deserializer, 9)?;
        Ok(PriceKline {
            open_time: cells.int(0)?,
            open: cells.dec(1)?,
            high: cells.dec(2)?,
            low: cells.dec(3)?,
            close: cells.dec(4)?,
            close_time: cells.int(6)?,
            points: cells.int(8)?,
        })
    }
}

fn kline_call(path: &'static str, key: &'static str, name: &str, query: &KlineQuery) -> Call {
    let limit = query.limit.map(|limit| limit.min(MOST_KLINES));
    Call::get(path)
        .field(key, name)
        .field("interval", &query.interval)
        .maybe("startTime", query.start_time)
        .maybe("endTime", query.end_time)
        .maybe("limit", limit)
        .weigh(kline_weight(limit))
}

/// `GET /fapi/v3/klines`.
#[must_use]
pub fn klines_call(symbol: &str, query: &KlineQuery) -> Call {
    kline_call("/fapi/v3/klines", "symbol", symbol, query)
}

/// `symbol`'s trade candles, oldest first.
pub async fn klines(transport: &impl Transport, symbol: &str, query: &KlineQuery) -> Result<Vec<Kline>, ApiError> {
    read(transport, klines_call(symbol, query)).await
}

/// `GET /fapi/v3/indexPriceKlines`, by pair.
#[must_use]
pub fn index_price_klines_call(pair: &str, query: &KlineQuery) -> Call {
    kline_call("/fapi/v3/indexPriceKlines", "pair", pair, query)
}

/// `pair`'s index price candles, oldest first.
pub async fn index_price_klines(
    transport: &impl Transport,
    pair: &str,
    query: &KlineQuery,
) -> Result<Vec<PriceKline>, ApiError> {
    read(transport, index_price_klines_call(pair, query)).await
}

/// `GET /fapi/v3/markPriceKlines`.
#[must_use]
pub fn mark_price_klines_call(symbol: &str, query: &KlineQuery) -> Call {
    kline_call("/fapi/v3/markPriceKlines", "symbol", symbol, query)
}

/// `symbol`'s mark price candles, oldest first.
pub async fn mark_price_klines(
    transport: &impl Transport,
    symbol: &str,
    query: &KlineQuery,
) -> Result<Vec<PriceKline>, ApiError> {
    read(transport, mark_price_klines_call(symbol, query)).await
}

#[cfg(test)]
#[path = "tests/klines.rs"]
mod tests;
