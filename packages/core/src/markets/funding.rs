//! Funding: the rates settled so far (`fundingRate`), and each symbol's interval, cap and floor
//! (`fundingInfo`). A symbol settles every 1, 2, 4 or 8 hours, its own interval; rates are
//! fractions of the notional for that interval, positive when longs pay.
//!
//! `fundingInfo` writes the cap and floor as bare JSON numbers (`0.003`); they are read exactly
//! through their shortest spelling ([`crate::markets`]).

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::markets::exact;
use crate::utils::Dec;
use crate::utils::lenient;
use crate::utils::time::HOUR_MS;

/// The most settlements one `fundingRate` read answers; the venue's default is 100.
pub const MOST_FUNDING_RATES: u32 = 1_000;

/// One settlement.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRate {
    pub symbol: String,
    /// A fraction of the notional, positive when longs paid.
    pub funding_rate: Dec,
    /// Unix ms.
    pub funding_time: u64,
}

/// A symbol's funding rules.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingConfig {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub interest_rate: Dec,
    /// Unix ms.
    #[serde(default)]
    pub time: u64,
    /// Hours between settlements.
    pub funding_interval_hours: u32,
    /// The highest and lowest rate one settlement may have, fractions.
    #[serde(deserialize_with = "exact")]
    pub funding_fee_cap: Dec,
    #[serde(deserialize_with = "exact")]
    pub funding_fee_floor: Dec,
}

impl FundingConfig {
    /// Milliseconds between settlements.
    #[must_use]
    pub const fn interval_ms(&self) -> u64 {
        (self.funding_interval_hours as u64).saturating_mul(HOUR_MS)
    }
}

/// Which settlements to read. Without a range, the latest `limit`; with a range larger than
/// `limit`, the first `limit` from the start.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FundingQuery {
    /// Every symbol when none is given.
    pub symbol: Option<String>,
    /// Unix ms, inclusive.
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    /// At most [`MOST_FUNDING_RATES`].
    pub limit: Option<u32>,
}

impl FundingQuery {
    /// `symbol`'s latest settlements.
    #[must_use]
    pub fn of(symbol: &str) -> Self {
        FundingQuery { symbol: Some(symbol.to_owned()), ..FundingQuery::default() }
    }
}

/// `GET /fapi/v3/fundingRate`.
#[must_use]
pub fn funding_rates_call(query: &FundingQuery) -> Call {
    Call::get("/fapi/v3/fundingRate")
        .maybe("symbol", query.symbol.as_deref())
        .maybe("startTime", query.start_time)
        .maybe("endTime", query.end_time)
        .maybe("limit", query.limit.map(|limit| limit.min(MOST_FUNDING_RATES)))
}

/// Settled funding rates, oldest first.
pub async fn funding_rates(transport: &impl Transport, query: &FundingQuery) -> Result<Vec<FundingRate>, ApiError> {
    read(transport, funding_rates_call(query)).await
}

/// `GET /fapi/v3/fundingInfo`, for one symbol or every one.
#[must_use]
pub fn funding_info_call(symbol: Option<&str>) -> Call {
    Call::get("/fapi/v3/fundingInfo").maybe("symbol", symbol)
}

/// Funding rules, `symbol`'s alone (still a list) or every symbol's.
pub async fn funding_info(transport: &impl Transport, symbol: Option<&str>) -> Result<Vec<FundingConfig>, ApiError> {
    read(transport, funding_info_call(symbol)).await
}

#[cfg(test)]
#[path = "tests/funding.rs"]
mod tests;
