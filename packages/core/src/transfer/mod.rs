//! Moving money: between an account's futures and spot wallets, between a main account and its
//! sub-accounts, to other Aster Chain addresses, withdrawals to a chain, and deposits through the
//! vault contracts.
//!
//! - [`wallet`](mod@wallet): futures and spot (`/fapi/v3/asset/wallet/transfer`, and Aster
//!   Chain's `/aster-chain/v3/perp/wallet/transfer`), and an asset exchange in multi-assets mode
//!   (`/fapi/v3/assetExchange`).
//! - [`sub_account`]: between a main account and its sub-accounts (`/fapi/v3/subAccountTransfer`).
//! - [`chain`]: to another Aster Chain address, one (`/aster-chain/v3/transfer`) or up to ten
//!   (`/aster-chain/v3/batch-transfer`) with their history, and the account's privacy status.
//! - [`withdraw`](mod@withdraw): a withdrawal to an EVM chain, signed by the main wallet as an
//!   EIP-712 `Action` and sent through Aster Chain or the futures API; a Solana withdrawal through
//!   the futures API; the account's limits, its history, and the fee estimates.
//! - [`deposit`](mod@deposit): the vault on each EVM chain, the transactions a deposit takes
//!   (`approve`, then `depositFor`), and the assets each chain takes.
//!
//! Who signs: calls marked agent-signed go out as `.signed()` through the
//! [`crate::client::Client`]'s agent. Calls Aster accepts only from the main wallet (a transfer to
//! another address, a withdrawal through Aster Chain) are signed by a [`Wallet`] here: their fields,
//! then `nonce` and `user`, as `Message { msg }` under the API domain, and the signature must
//! recover to the wallet before anything is sent ([`sign_with_wallet`]).
//!
//! Amounts are exact decimals, written without trailing zeros, as the withdrawal signature needs.
//! A deposit is counted in the token's smallest unit; an amount finer than that is refused.
//!
//! Not here: Aster Chain staking, spot-only withdrawals and transfers, Solana deposits (a program
//! instruction, not EVM calldata), withdrawals through Aster Chain from a Solana main wallet (its
//! Ed25519 signature is the caller's), and moving assets between accounts
//! (`/fapi/v3/asset/migrateUser`, with the account calls).

pub mod chain;
pub mod deposit;
pub mod sub_account;
pub mod wallet;
pub mod withdraw;

pub use chain::{
    AcceptedItem, BatchAccepted, BatchItem, BatchPage, BatchQuery, BatchRow, BatchRowItem, ChainTransfer,
    ChainTransferred, MOST_IN_BATCH, Privacy, PrivacyStatus, account_status, account_status_call, batch_history,
    batch_history_call, batch_transfer_call, modify_account_status, modify_account_status_call,
};
#[cfg(feature = "sign")]
pub use chain::{batch_transfer, transfer_to_address};
pub use deposit::{
    BROKER_FUTURES, BROKER_SPOT, BadDeposit, ChainAsset, Deposit, DepositAddress, DepositPlan, NATIVE_TOKEN, VAULTS,
    Vault, WalletTx, approve_calldata, deposit_assets, deposit_assets_call, deposit_for_calldata, selector,
    user_deposit_address, user_deposit_address_call, vault, withdraw_assets, withdraw_assets_call,
};
pub use sub_account::{SubAccountTransfer, sub_account_message};
#[cfg(feature = "sign")]
pub use sub_account::{sub_account_transfer, sub_account_transfer_call};
pub use wallet::{
    AssetExchanged, WalletTransferred, asset_exchange, asset_exchange_call, chain_wallet_transfer,
    chain_wallet_transfer_call, wallet_transfer, wallet_transfer_call,
};
pub use withdraw::{
    AssetLimits, ChainLimits, GasEstimate, Movement, SOLANA_CHAIN_ID, SolanaWithdrawal, WithdrawInfo, Withdrawal,
    Withdrawn, destination_chain, estimate_fee, estimate_fee_call, history, history_call, solana_message,
    solana_withdraw_by_agent, solana_withdraw_by_agent_call, web_estimate_fee, web_estimate_fee_call,
    withdraw_by_agent_call, withdraw_call, withdraw_info, withdraw_info_call,
};
#[cfg(feature = "sign")]
pub use withdraw::{withdraw, withdraw_by_agent};

use serde::{Deserialize, Deserializer};

use crate::client::http::calls::refusal;
use crate::client::{ApiError, Call, Transport, TransportError, decode};
use crate::utils::Dec;
use crate::utils::sign::{TypedData, WalletError, message};
#[cfg(feature = "sign")]
use crate::utils::sign::{Wallet, sign_checked};

/// Which wallets a transfer moves between: `FUTURE_SPOT` is futures to spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    FutureSpot,
    SpotFuture,
    /// Only between a main account and its sub-accounts.
    FutureFuture,
    /// Only between a main account and its sub-accounts.
    SpotSpot,
}

impl Direction {
    /// As the venue writes it (`kindType`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::FutureSpot => "FUTURE_SPOT",
            Direction::SpotFuture => "SPOT_FUTURE",
            Direction::FutureFuture => "FUTURE_FUTURE",
            Direction::SpotSpot => "SPOT_SPOT",
        }
    }
}

/// Why a transfer or a withdrawal was not taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    /// Refused before anything was signed or sent.
    #[error("{0}")]
    Invalid(String),
    /// The main wallet did not sign, or signed as someone else: nothing was sent.
    #[error(transparent)]
    Wallet(#[from] WalletError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

/// What the main wallet signs to authorise `call` at `nonce`: the call's fields, then `nonce` and
/// `user`, as the query string the request carries, in `Message { msg }` under the API domain
/// (chain id 1666). Gives the fields the request carries before its `signature`, and the typed
/// data.
#[must_use]
pub fn wallet_message(
    call: &Call,
    nonce: u64,
    user: crate::utils::Address,
) -> (Vec<(&'static str, String)>, TypedData) {
    let mut fields = call.fields.clone();
    fields.push(("nonce", nonce.to_string()));
    fields.push(("user", user.to_checksum()));
    let msg = crate::utils::query(&fields);
    (fields, message(&msg))
}

/// `call` signed by the main wallet at `nonce` ([`wallet_message`]): the same call, unsigned by any
/// agent, carrying `nonce`, `user` and the wallet's `signature`. The signature is recovered and
/// must be the wallet's.
#[cfg(feature = "sign")]
pub async fn sign_with_wallet(call: Call, nonce: u64, wallet: &impl Wallet) -> Result<Call, WalletError> {
    let (mut fields, data) = wallet_message(&call, nonce, wallet.address());
    let signature = sign_checked(wallet, &data).await?;
    fields.push(("signature", signature.to_hex()));
    Ok(Call { fields, signed: false, ..call })
}

/// The web app's answers come wrapped: `{"code":"000000","data":...,"success":true}`.
#[derive(Deserialize)]
pub(crate) struct WebAnswer<T> {
    pub(crate) data: T,
}

/// An answer that may be an empty body: `None` then.
pub(crate) async fn read_maybe<T: serde::de::DeserializeOwned>(
    transport: &impl Transport,
    call: Call,
) -> Result<Option<T>, ApiError> {
    let path = call.path;
    match transport.call(call).await {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) => decode(path, &text).map(Some),
        Err(TransportError::Status { status, body }) => {
            Err(refusal(&body).unwrap_or(ApiError::Transport(TransportError::Status { status, body })))
        }
        Err(error) => Err(ApiError::Transport(error)),
    }
}

/// A figure the venue writes as a decimal string, an integer, or a bare JSON fraction (the fee
/// estimates do): read through its shortest spelling, which is the one the venue wrote. `null` or
/// absent is `None`, as is a figure that does not read as a decimal.
pub(crate) fn figure<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Dec>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(deserializer)? {
        Some(serde_json::Value::Number(number)) => Dec::parse(&number.to_string()).ok(),
        Some(serde_json::Value::String(text)) => Dec::parse(&text).ok(),
        _ => None,
    })
}

#[cfg(test)]
#[path = "tests/support.rs"]
mod tests;
