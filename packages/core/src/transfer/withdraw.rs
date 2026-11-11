//! Withdrawals from the futures account to a chain, the account's limits and history, and the fee
//! estimates.
//!
//! A withdrawal to an EVM chain carries two signatures:
//!
//! 1. `userSignature`: the main wallet's EIP-712 signature of an `Action` under the domain
//!    `{Aster, "1", <the destination chain's id>, 0x0}`:
//!    `Action(string type,address destination,string destination Chain,string token,string amount,string fee,uint256 nonce,string aster chain)`,
//!    `type` being `Withdraw` and `aster chain` `Mainnet`. The amount and fee are written without
//!    trailing zeros, and the nonce is microseconds (`userNonce`).
//! 2. The call's own: through Aster Chain (`POST /aster-chain/v3/perp/user-withdraw`) the main wallet
//!    signs that too, since Aster refuses an agent there; through the futures API
//!    (`POST /fapi/v3/aster/user-withdraw`) the agent signs it.
//!
//! A Solana withdrawal goes through the futures API (`POST /fapi/v3/aster/user-solana-withdraw`),
//! agent-signed, with the Solana wallet's Ed25519 signature of [`solana_message`] as
//! `userSignature`, which the caller makes.
//!
//! Reads:
//!
//! - `GET /aster-chain/v3/perp/user-withdraw-info`: the daily limits and what each asset may
//!   withdraw on each chain, agent-signed.
//! - `GET /aster-chain/v3/perp/deposit-withdraw-history`: deposits and withdrawals, agent-signed.
//! - `GET /aster-chain/v3/withdraw/estimateFee` and the web app's
//!   `GET /bapi/futures/v1/public/future/aster/estimate-withdraw-fee`: the fee in the token
//!   (`gasCost`). Public.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::client::hosts::ASTER_CHAIN;
use crate::client::{ApiError, Call, Host, Transport, read};
use crate::transfer::{WebAnswer, figure};
use crate::utils::lenient;
use crate::utils::sign::{Domain, Field, Signature, Struct, TypedData};
use crate::utils::{Address, Dec};
#[cfg(feature = "sign")]
use crate::{
    client::{Client, Credentials, Http},
    transfer::{TransferError, sign_with_wallet},
    utils::sign::{Wallet, sign_checked},
};

/// The chain id a Solana withdrawal names.
pub const SOLANA_CHAIN_ID: u64 = 101;

/// The name a withdrawal's `destination Chain` gives `chain_id`, from the docs' table.
#[must_use]
pub const fn destination_chain(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        1 => Some("ETH"),
        56 => Some("BSC"),
        42161 => Some("Arbitrum"),
        131 => Some("SUI"),
        _ => None,
    }
}

/// A withdrawal to an EVM address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawal {
    /// The chain it is paid out on: 1, 56 or 42161.
    pub chain_id: u64,
    /// The asset's name, as the withdraw asset list writes it (`USDT`).
    pub asset: String,
    pub amount: Dec,
    /// The fee, from [`estimate_fee`] or [`WithdrawInfo`]; it is taken from the amount.
    pub fee: Dec,
    /// The address paid: the account's own registered address.
    pub receiver: Address,
    /// Microseconds, unique: `Client::next_nonce`.
    pub user_nonce: u64,
}

impl Withdrawal {
    /// Why the venue would refuse it before it is signed, if it would.
    #[must_use]
    pub fn fault(&self) -> Option<String> {
        if destination_chain(self.chain_id).is_none() {
            return Some(format!("Aster pays out no withdrawal on chain {}", self.chain_id));
        }
        if !self.amount.is_positive() {
            return Some(format!("a withdrawal's amount is positive, not {}", self.amount));
        }
        if self.fee.is_negative() || self.fee >= self.amount {
            return Some(format!("the fee {} must be below the amount {}", self.fee, self.amount));
        }
        None
    }

    /// The `Action` the main wallet signs.
    #[must_use]
    pub fn action(&self) -> TypedData {
        let chain = destination_chain(self.chain_id).map_or_else(|| self.chain_id.to_string(), str::to_owned);
        TypedData::new(
            Domain::new("Aster", "1", self.chain_id),
            Struct::new("Action")
                .field("type", Field::String("Withdraw".to_owned()))
                .field("destination", Field::Address(self.receiver))
                .field("destination Chain", Field::String(chain))
                .field("token", Field::String(self.asset.clone()))
                .field("amount", Field::String(self.amount.to_string()))
                .field("fee", Field::String(self.fee.to_string()))
                .field("nonce", Field::Uint256(u128::from(self.user_nonce)))
                .field("aster chain", Field::String(ASTER_CHAIN.to_owned())),
        )
    }

    fn fields(&self, call: Call) -> Call {
        call.field("chainId", self.chain_id)
            .field("asset", &self.asset)
            .field("amount", self.amount)
            .field("fee", self.fee)
            .field("receiver", self.receiver.to_checksum())
            .field("userNonce", self.user_nonce)
    }
}

/// `POST /aster-chain/v3/perp/user-withdraw` with the `Action`'s signature, before the main wallet
/// signs the call itself.
#[must_use]
pub fn withdraw_call(withdrawal: &Withdrawal, user_signature: &Signature) -> Call {
    withdrawal
        .fields(Call::post("/aster-chain/v3/perp/user-withdraw").on(Host::Chain))
        .field("signatureType", "EOA")
        .field("userSignature", user_signature.to_hex())
}

/// `POST /fapi/v3/aster/user-withdraw` with the `Action`'s signature, signed by the agent.
#[must_use]
pub fn withdraw_by_agent_call(withdrawal: &Withdrawal, user_signature: &Signature) -> Call {
    withdrawal
        .fields(Call::post("/fapi/v3/aster/user-withdraw"))
        .field("userSignature", user_signature.to_hex())
        .signed()
}

/// A withdrawal the venue took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Withdrawn {
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub withdraw_id: String,
    /// The payout's transaction, once there is one.
    #[serde(default, deserialize_with = "lenient::text")]
    pub hash: String,
}

/// Has the main wallet sign the `Action` and the call, through Aster Chain, and sends it once.
/// Both signatures must recover to the wallet.
#[cfg(feature = "sign")]
pub async fn withdraw<H: Http, K: Credentials>(
    client: &Client<H, K>,
    wallet: &impl Wallet,
    withdrawal: &Withdrawal,
) -> Result<Withdrawn, TransferError> {
    if let Some(fault) = withdrawal.fault() {
        return Err(TransferError::Invalid(fault));
    }
    let user_signature = sign_checked(wallet, &withdrawal.action()).await?;
    let call = sign_with_wallet(withdraw_call(withdrawal, &user_signature), client.next_nonce(), wallet).await?;
    Ok(read(client, call).await?)
}

/// Has the main wallet sign the `Action`, then sends it through the futures API, signed by the
/// client's agent, once.
#[cfg(feature = "sign")]
pub async fn withdraw_by_agent(
    transport: &impl Transport,
    wallet: &impl Wallet,
    withdrawal: &Withdrawal,
) -> Result<Withdrawn, TransferError> {
    if let Some(fault) = withdrawal.fault() {
        return Err(TransferError::Invalid(fault));
    }
    let user_signature = sign_checked(wallet, &withdrawal.action()).await?;
    Ok(read(transport, withdraw_by_agent_call(withdrawal, &user_signature)).await?)
}

/// A withdrawal to a Solana address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolanaWithdrawal {
    pub asset: String,
    pub amount: Dec,
    pub fee: Dec,
    /// The base58 address paid.
    pub receiver: String,
    /// Microseconds, unique.
    pub user_nonce: u64,
}

/// What the Solana wallet signs (Ed25519, base58): the fields joined by commas, the amount and fee
/// without trailing zeros.
#[must_use]
pub fn solana_message(withdrawal: &SolanaWithdrawal) -> String {
    format!(
        "PrimaryType=Withdraw,AsterChain={ASTER_CHAIN},Destination={},DestinationChain=Solana,Token={},Amount={},Fee={},Nonce={}",
        withdrawal.receiver, withdrawal.asset, withdrawal.amount, withdrawal.fee, withdrawal.user_nonce
    )
}

/// `POST /fapi/v3/aster/user-solana-withdraw`, signed by the agent. `user_signature` is the Solana
/// wallet's base58 signature of [`solana_message`].
#[must_use]
pub fn solana_withdraw_by_agent_call(withdrawal: &SolanaWithdrawal, user_signature: &str) -> Call {
    Call::post("/fapi/v3/aster/user-solana-withdraw")
        .field("chainId", SOLANA_CHAIN_ID)
        .field("asset", &withdrawal.asset)
        .field("amount", withdrawal.amount)
        .field("fee", withdrawal.fee)
        .field("receiver", &withdrawal.receiver)
        .field("userNonce", withdrawal.user_nonce)
        .field("userSignature", user_signature)
        .signed()
}

pub async fn solana_withdraw_by_agent(
    transport: &impl Transport,
    withdrawal: &SolanaWithdrawal,
    user_signature: &str,
) -> Result<Withdrawn, ApiError> {
    read(transport, solana_withdraw_by_agent_call(withdrawal, user_signature)).await
}

/// The account's withdrawal limits, in USD, and what each asset may withdraw.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawInfo {
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub user_daily_limit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub user_remaining_daily_limit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_daily_limit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_remaining_daily_limit: Dec,
    /// By asset name; only assets with a balance.
    #[serde(default)]
    pub balances: BTreeMap<String, AssetLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLimits {
    pub currency: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub spot_total_withdraw_amount: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub perp_total_withdraw_amount: Dec,
    /// What is left of the asset's daily limit, in USD.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub daily_limit: Dec,
    /// By chain id, as text.
    #[serde(default)]
    pub chain_balances: BTreeMap<String, ChainLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainLimits {
    pub chain_id: u64,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub spot_max_withdraw_amount: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub perp_max_withdraw_amount: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub chain_limit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub withdraw_fee: Dec,
}

#[must_use]
pub fn withdraw_info_call() -> Call {
    Call::get("/aster-chain/v3/perp/user-withdraw-info").on(Host::Chain).signed()
}

pub async fn withdraw_info(transport: &impl Transport) -> Result<WithdrawInfo, ApiError> {
    read(transport, withdraw_info_call()).await
}

/// A deposit or a withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Movement {
    #[serde(deserialize_with = "lenient::id_text")]
    pub id: String,
    /// `DEPOSIT` or `WITHDRAW`.
    #[serde(rename = "type")]
    pub kind: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    /// `PROCESSING`, `SUCCESS` (or `COMPLETED`), `FAILED`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub tx_hash: String,
    #[serde(default)]
    pub time: Option<u64>,
    #[serde(default)]
    pub chain_id: Option<u64>,
    /// `perp` or `spot`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub account_type: String,
}

/// The futures account's deposits and withdrawals, on one chain when given.
#[must_use]
pub fn history_call(chain_id: Option<u64>) -> Call {
    Call::get("/aster-chain/v3/perp/deposit-withdraw-history").on(Host::Chain).maybe("chainId", chain_id).signed()
}

pub async fn history(transport: &impl Transport, chain_id: Option<u64>) -> Result<Vec<Movement>, ApiError> {
    read(transport, history_call(chain_id)).await
}

/// A fee estimate. `gas_cost` is the fee in the token, which a withdrawal names as its `fee`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GasEstimate {
    #[serde(default, deserialize_with = "figure")]
    pub gas_price: Option<Dec>,
    #[serde(default, deserialize_with = "figure")]
    pub gas_limit: Option<Dec>,
    #[serde(default, deserialize_with = "figure")]
    pub native_price: Option<Dec>,
    #[serde(default, deserialize_with = "figure")]
    pub token_price: Option<Dec>,
    #[serde(default, deserialize_with = "figure")]
    pub gas_cost: Option<Dec>,
    #[serde(default, deserialize_with = "figure")]
    pub gas_usd_value: Option<Dec>,
}

/// Aster Chain's estimate for `asset` paid out on `chain_id`.
#[must_use]
pub fn estimate_fee_call(chain_id: u64, asset: &str) -> Call {
    Call::get("/aster-chain/v3/withdraw/estimateFee").on(Host::Chain).field("chainId", chain_id).field("asset", asset)
}

pub async fn estimate_fee(transport: &impl Transport, chain_id: u64, asset: &str) -> Result<GasEstimate, ApiError> {
    read(transport, estimate_fee_call(chain_id, asset)).await
}

/// The web app's estimate: `network` is `EVM` or `SOL`, `account_type` `perp` or `spot`.
#[must_use]
pub fn web_estimate_fee_call(chain_id: u64, network: &str, currency: &str, account_type: &str) -> Call {
    Call::get("/bapi/futures/v1/public/future/aster/estimate-withdraw-fee")
        .on(Host::Web)
        .field("chainId", chain_id)
        .field("network", network)
        .field("currency", currency)
        .field("accountType", account_type)
}

pub async fn web_estimate_fee(
    transport: &impl Transport,
    chain_id: u64,
    network: &str,
    currency: &str,
    account_type: &str,
) -> Result<GasEstimate, ApiError> {
    let answer: WebAnswer<GasEstimate> =
        read(transport, web_estimate_fee_call(chain_id, network, currency, account_type)).await?;
    Ok(answer.data)
}

#[cfg(test)]
#[path = "tests/withdraw.rs"]
mod tests;
