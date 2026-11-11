//! Between an account's futures and spot wallets, and an asset exchange in multi-assets mode. All
//! three are signed by the agent.
//!
//! - `POST /fapi/v3/asset/wallet/transfer`: futures to spot or back, weight 50.
//! - `POST /aster-chain/v3/perp/wallet/transfer`: the same through Aster Chain's API, weight 5.
//! - `POST /fapi/v3/assetExchange`: exchange the account's assets as an automatic exchange at a
//!   threshold of 0 would. Only in multi-assets mode ([`crate::client::codes::NOT_JOINT_MARGIN`]
//!   otherwise); an empty answer means nothing was exchanged.

use serde::Deserialize;

use crate::client::{ApiError, Call, Host, Transport, read};
use crate::transfer::{Direction, read_maybe};
use crate::utils::Dec;
use crate::utils::lenient;

/// A wallet transfer the venue took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletTransferred {
    #[serde(deserialize_with = "lenient::id_text")]
    pub tran_id: String,
    /// `SUCCESS` when done.
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
}

/// `POST /fapi/v3/asset/wallet/transfer`: `amount` of `asset` between futures and spot.
/// `client_tran_id` is the caller's id for it. Only [`Direction::FutureSpot`] and
/// [`Direction::SpotFuture`] are taken.
#[must_use]
pub fn wallet_transfer_call(asset: &str, amount: Dec, client_tran_id: &str, direction: Direction) -> Call {
    Call::post("/fapi/v3/asset/wallet/transfer")
        .field("amount", amount)
        .field("asset", asset)
        .field("clientTranId", client_tran_id)
        .field("kindType", direction.as_str())
        .signed()
}

pub async fn wallet_transfer(
    transport: &impl Transport,
    asset: &str,
    amount: Dec,
    client_tran_id: &str,
    direction: Direction,
) -> Result<WalletTransferred, ApiError> {
    read(transport, wallet_transfer_call(asset, amount, client_tran_id, direction)).await
}

/// `POST /aster-chain/v3/perp/wallet/transfer`: the same move through Aster Chain's API.
#[must_use]
pub fn chain_wallet_transfer_call(asset: &str, amount: Dec, client_tran_id: &str, direction: Direction) -> Call {
    Call::post("/aster-chain/v3/perp/wallet/transfer")
        .on(Host::Chain)
        .field("asset", asset)
        .field("amount", amount)
        .field("clientTranId", client_tran_id)
        .field("kindType", direction.as_str())
        .signed()
}

pub async fn chain_wallet_transfer(
    transport: &impl Transport,
    asset: &str,
    amount: Dec,
    client_tran_id: &str,
    direction: Direction,
) -> Result<WalletTransferred, ApiError> {
    read(transport, chain_wallet_transfer_call(asset, amount, client_tran_id, direction)).await
}

/// An asset exchange's result: the asset whose balance changed and its balance after.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetExchanged {
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub account_id: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub balance: Dec,
    #[serde(default)]
    pub update_time: Option<u64>,
}

/// `POST /fapi/v3/assetExchange`: no fields but the agent's.
#[must_use]
pub fn asset_exchange_call() -> Call {
    Call::post("/fapi/v3/assetExchange").signed()
}

/// `None` when nothing was exchanged.
pub async fn asset_exchange(transport: &impl Transport) -> Result<Option<AssetExchanged>, ApiError> {
    read_maybe(transport, asset_exchange_call()).await
}

#[cfg(test)]
#[path = "tests/wallet.rs"]
mod tests;
