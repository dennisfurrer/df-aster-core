//! Connectivity (`ping`), the venue's clock (`time`) and every asset's logo
//! (`common/asset/all-asset-logo`).

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::Done;

/// The venue's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTime {
    /// Unix milliseconds.
    pub server_time: u64,
}

/// An asset's logo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLogo {
    pub asset_code: String,
    pub logo_url: String,
}

/// `GET /fapi/v3/ping`.
#[must_use]
pub fn ping_call() -> Call {
    Call::get("/fapi/v3/ping")
}

/// Whether the REST API answers: an empty object when it does.
pub async fn ping(transport: &impl Transport) -> Result<(), ApiError> {
    read::<Done>(transport, ping_call()).await.map(|_| ())
}

/// `GET /fapi/v3/time`.
#[must_use]
pub fn server_time_call() -> Call {
    Call::get("/fapi/v3/time")
}

/// The venue's clock, unix milliseconds. [`crate::Client::sync_clock`] keeps the difference
/// from this machine's.
pub async fn server_time(transport: &impl Transport) -> Result<u64, ApiError> {
    read::<ServerTime>(transport, server_time_call()).await.map(|time| time.server_time)
}

/// `GET /fapi/v3/common/asset/all-asset-logo`.
#[must_use]
pub fn asset_logos_call() -> Call {
    Call::get("/fapi/v3/common/asset/all-asset-logo")
}

/// The logo of every active asset.
pub async fn asset_logos(transport: &impl Transport) -> Result<Vec<AssetLogo>, ApiError> {
    read(transport, asset_logos_call()).await
}

#[cfg(test)]
#[path = "tests/system.rs"]
mod tests;
