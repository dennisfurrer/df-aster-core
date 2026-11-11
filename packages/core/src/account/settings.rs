//! An account's settings, each on every symbol at once: position mode (hedge or one-way,
//! `positionSide/dual`), self-trade prevention (`stpMode`), multi-assets margin
//! (`multiAssetsMargin`), market maker protection per symbol (`mmp`, `mmpReset`), and whether an
//! address joined through the account's referral (`referral/inviteeInfo`).

use serde::Deserialize;

use crate::account::maybe_number_dec;
use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::lenient;
use crate::utils::{Dec, Done, StpMode};

/// `GET /fapi/v3/positionSide/dual`, weight 30.
#[must_use]
pub fn position_mode_call() -> Call {
    Call::get("/fapi/v3/positionSide/dual").signed()
}

/// Whether the account is in hedge mode (`true`) or one-way mode (`false`).
pub async fn position_mode(transport: &impl Transport) -> Result<bool, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Mode {
        #[serde(deserialize_with = "lenient::flag")]
        dual_side_position: bool,
    }
    let mode: Mode = read(transport, position_mode_call()).await?;
    Ok(mode.dual_side_position)
}

/// `POST /fapi/v3/positionSide/dual`.
#[must_use]
pub fn set_position_mode_call(hedge: bool) -> Call {
    Call::post("/fapi/v3/positionSide/dual").field("dualSidePosition", hedge).signed()
}

/// Hedge mode (`true`) or one-way mode (`false`). Refused while orders are open or positions
/// held, and when it is already so (`-4059`).
pub async fn set_position_mode(transport: &impl Transport, hedge: bool) -> Result<Done, ApiError> {
    read(transport, set_position_mode_call(hedge)).await
}

/// `GET /fapi/v3/stpMode`, weight 30.
#[must_use]
pub fn stp_mode_call() -> Call {
    Call::get("/fapi/v3/stpMode").signed()
}

/// The account's self-trade prevention mode.
pub async fn stp_mode(transport: &impl Transport) -> Result<StpMode, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Mode {
        stp_mode: StpMode,
    }
    let mode: Mode = read(transport, stp_mode_call()).await?;
    Ok(mode.stp_mode)
}

/// `POST /fapi/v3/stpMode`.
#[must_use]
pub fn set_stp_mode_call(mode: &StpMode) -> Call {
    Call::post("/fapi/v3/stpMode").field("stpMode", mode).signed()
}

/// Which side of a self-trade is cancelled, unless an order says otherwise.
pub async fn set_stp_mode(transport: &impl Transport, mode: &StpMode) -> Result<Done, ApiError> {
    read(transport, set_stp_mode_call(mode)).await
}

/// `GET /fapi/v3/multiAssetsMargin`, weight 30.
#[must_use]
pub fn multi_assets_mode_call() -> Call {
    Call::get("/fapi/v3/multiAssetsMargin").signed()
}

/// Whether the account is in multi-assets mode (`true`) or single-asset mode (`false`).
pub async fn multi_assets_mode(transport: &impl Transport) -> Result<bool, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Mode {
        #[serde(deserialize_with = "lenient::flag")]
        multi_assets_margin: bool,
    }
    let mode: Mode = read(transport, multi_assets_mode_call()).await?;
    Ok(mode.multi_assets_margin)
}

/// `POST /fapi/v3/multiAssetsMargin`.
#[must_use]
pub fn set_multi_assets_mode_call(on: bool) -> Call {
    Call::post("/fapi/v3/multiAssetsMargin").field("multiAssetsMargin", on).signed()
}

pub async fn set_multi_assets_mode(transport: &impl Transport, on: bool) -> Result<Done, ApiError> {
    read(transport, set_multi_assets_mode_call(on)).await
}

/// A symbol's market maker protection: within each window, filling past any limit freezes the
/// account's MMP orders on the symbol for the frozen time.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mmp {
    pub symbol: String,
    #[serde(rename = "windowTimeInMilliseconds")]
    pub window_ms: u64,
    #[serde(rename = "frozenTimeInMilliseconds")]
    pub frozen_ms: u64,
    /// Filled quantity, both sides together.
    #[serde(default, deserialize_with = "maybe_number_dec")]
    pub qty_limit: Option<Dec>,
    /// Filled notional.
    #[serde(default, deserialize_with = "maybe_number_dec")]
    pub value_limit: Option<Dec>,
    /// Net position change: buys less sells.
    #[serde(default, deserialize_with = "maybe_number_dec")]
    pub delta_limit: Option<Dec>,
}

/// `GET /fapi/v3/mmp`.
#[must_use]
pub fn mmp_call(symbol: Option<&str>) -> Call {
    Call::get("/fapi/v3/mmp").maybe("symbol", symbol).signed()
}

/// The protection set on one symbol, or on every one.
pub async fn mmp(transport: &impl Transport, symbol: Option<&str>) -> Result<Vec<Mmp>, ApiError> {
    read(transport, mmp_call(symbol)).await
}

/// `POST /fapi/v3/mmp`.
#[must_use]
pub fn update_mmp_call(protection: &Mmp) -> Call {
    let call = Call::post("/fapi/v3/mmp").field("symbol", &protection.symbol);
    let call = call.field("windowTimeInMilliseconds", protection.window_ms);
    let call = call.field("frozenTimeInMilliseconds", protection.frozen_ms);
    let call = call.maybe("qtyLimit", protection.qty_limit).maybe("valueLimit", protection.value_limit);
    call.maybe("deltaLimit", protection.delta_limit).signed()
}

/// Sets a symbol's protection. The venue answers `true`.
pub async fn update_mmp(transport: &impl Transport, protection: &Mmp) -> Result<bool, ApiError> {
    read(transport, update_mmp_call(protection)).await
}

/// `DELETE /fapi/v3/mmp`.
#[must_use]
pub fn delete_mmp_call(symbol: &str) -> Call {
    Call::delete("/fapi/v3/mmp").field("symbol", symbol).signed()
}

pub async fn delete_mmp(transport: &impl Transport, symbol: &str) -> Result<bool, ApiError> {
    read(transport, delete_mmp_call(symbol)).await
}

/// `POST /fapi/v3/mmpReset`.
#[must_use]
pub fn reset_mmp_call(symbol: &str) -> Call {
    Call::post("/fapi/v3/mmpReset").field("symbol", symbol).signed()
}

/// Lifts a frozen symbol before its frozen time is over.
pub async fn reset_mmp(transport: &impl Transport, symbol: &str) -> Result<bool, ApiError> {
    read(transport, reset_mmp_call(symbol)).await
}

/// Whether an address joined through the account's referral, and what it brought.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteeInfo {
    #[serde(deserialize_with = "lenient::flag")]
    pub is_invitee: bool,
    /// The address as it was sent, trimmed.
    pub wallet_address: String,
    /// When it joined, unix ms. Only for an invitee, as are the rest.
    #[serde(default)]
    pub date_joined: Option<u64>,
    /// The trading fee it has paid in all. Aggregated now and then, not live.
    #[serde(default, deserialize_with = "maybe_number_dec")]
    pub fee_paid: Option<Dec>,
    /// What the account has earned from it in all.
    #[serde(default, deserialize_with = "maybe_number_dec")]
    pub your_rewards: Option<Dec>,
    /// The relationship lasts 365 days.
    #[serde(default)]
    pub expired_status: Option<bool>,
}

/// `GET /fapi/v3/referral/inviteeInfo`. The address may be EVM or Solana, so it is text.
#[must_use]
pub fn invitee_info_call(address: &str) -> Call {
    Call::get("/fapi/v3/referral/inviteeInfo").field("address", address).signed()
}

pub async fn invitee_info(transport: &impl Transport, address: &str) -> Result<InviteeInfo, ApiError> {
    read(transport, invitee_info_call(address)).await
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
