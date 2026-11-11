//! Deposits from an EVM chain: the vault contract on Ethereum, BNB Smart Chain and Arbitrum, the
//! transactions a deposit takes, and the assets each chain takes. Nothing here sends a
//! transaction: a [`DepositPlan`] is what the user's wallet sends, in order.
//!
//! - **ERC-20:** `approve(vault, amount)` on the token, then
//!   `depositFor(token, forAddress, amount, broker)` on the vault with no value.
//! - **The chain's own coin** (ETH, BNB): `depositFor(NATIVE_TOKEN, forAddress, amount, broker)`
//!   with the amount as the transaction's value.
//! - `broker` 1000 credits the spot account; anything else, the futures account ([`BROKER_FUTURES`]).
//!
//! Reads:
//!
//! - `GET /bapi/futures/v1/public/future/aster/deposit/assets` and `.../withdraw/assets` on the
//!   web app's host: each chain's assets, their contracts and decimals. Public.
//! - `GET /aster-chain/v3/spot/user-deposit-address`: the account's own deposit address on a
//!   network that has one (SUI, for the spot account). Agent-signed.

use serde::Deserialize;

use crate::client::{ApiError, Call, Host, Transport, read};
use crate::transfer::{WebAnswer, figure};
use crate::utils::dec::Round;
use crate::utils::lenient;
use crate::utils::sign::keccak256;
use crate::utils::{Address, Dec};

/// A chain's vault, which `depositFor` is called on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Vault {
    pub chain_id: u64,
    /// The chain's name in a withdrawal (`destination Chain`).
    pub name: &'static str,
    pub address: Address,
}

const fn address(text: &str) -> Address {
    match Address::parse_const(text) {
        Some(address) => address,
        None => Address::ZERO,
    }
}

/// The vaults, from the docs' "Mainnet contract addresses".
pub const VAULTS: [Vault; 3] = [
    Vault { chain_id: 1, name: "ETH", address: address("0x604DD02d620633Ae427888d41bfd15e38483736E") },
    Vault { chain_id: 56, name: "BSC", address: address("0x128463A60784c4D3f46c23Af3f65Ed859Ba87974") },
    Vault { chain_id: 42161, name: "Arbitrum", address: address("0x9E36CB86a159d479cEd94Fa05036f235Ac40E1d5") },
];

/// The vault on `chain_id`, if Aster has one there.
#[must_use]
pub fn vault(chain_id: u64) -> Option<Vault> {
    VAULTS.iter().copied().find(|vault| vault.chain_id == chain_id)
}

/// What `depositFor` takes as `currency` for the chain's own coin.
pub const NATIVE_TOKEN: Address = address("0xfdAE1bA7C826aBDc4c99903c8056f82a1A04a615");

/// `broker` for a deposit to the spot account.
pub const BROKER_SPOT: u64 = 1_000;

/// `broker` for a deposit to the futures account: any value but 1000.
pub const BROKER_FUTURES: u64 = 0;

/// The first four bytes of the keccak of a function's signature, `name(type,type)`.
#[must_use]
pub fn selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

fn word_address(address: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.bytes());
    word
}

fn word_uint(value: u128) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
    word
}

/// `depositFor(address currency, address forAddress, uint256 amount, uint256 broker)`.
#[must_use]
pub fn deposit_for_calldata(currency: Address, for_address: Address, amount: u128, broker: u64) -> Vec<u8> {
    let mut data = selector("depositFor(address,address,uint256,uint256)").to_vec();
    data.extend_from_slice(&word_address(currency));
    data.extend_from_slice(&word_address(for_address));
    data.extend_from_slice(&word_uint(amount));
    data.extend_from_slice(&word_uint(u128::from(broker)));
    data
}

/// `approve(address spender, uint256 amount)` on an ERC-20.
#[must_use]
pub fn approve_calldata(spender: Address, amount: u128) -> Vec<u8> {
    let mut data = selector("approve(address,uint256)").to_vec();
    data.extend_from_slice(&word_address(spender));
    data.extend_from_slice(&word_uint(amount));
    data
}

/// A transaction for the user's wallet to send: no nonce, gas or signature, which the wallet
/// adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletTx {
    pub chain_id: u64,
    pub to: Address,
    pub data: Vec<u8>,
    /// In the chain's smallest unit (wei).
    pub value: u128,
}

impl WalletTx {
    /// `data` as `0x` hex, as `eth_sendTransaction` takes it.
    #[must_use]
    pub fn data_hex(&self) -> String {
        crate::utils::hex::encode_prefixed(&self.data)
    }
}

/// A deposit to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deposit {
    pub chain_id: u64,
    /// The token's contract, or [`NATIVE_TOKEN`] for the chain's own coin.
    pub currency: Address,
    /// The token's decimals on that chain ([`ChainAsset::decimals`]).
    pub decimals: u32,
    /// The account credited.
    pub for_address: Address,
    pub amount: Dec,
    /// To the spot account rather than futures.
    pub to_spot: bool,
}

/// Why a deposit was not planned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadDeposit {
    #[error("Aster has no vault on chain {0}")]
    NoVault(u64),
    #[error("a deposit's amount is positive, not {0}")]
    NotPositive(Dec),
    #[error("{amount} is finer than the token's {decimals} decimals")]
    TooFine { amount: Dec, decimals: u32 },
    #[error("{0} is more than a deposit can carry")]
    TooLarge(Dec),
}

/// The transactions a deposit takes, in order: the approval when it is a token, then the deposit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositPlan {
    pub approve: Option<WalletTx>,
    pub deposit: WalletTx,
    /// The amount in the token's smallest unit.
    pub units: u128,
}

impl Deposit {
    /// The amount in the token's smallest unit, exact.
    pub fn units(&self) -> Result<u128, BadDeposit> {
        if !self.amount.is_positive() {
            return Err(BadDeposit::NotPositive(self.amount));
        }
        if self.amount.places() > self.decimals {
            return Err(BadDeposit::TooFine { amount: self.amount, decimals: self.decimals });
        }
        let units = self.amount.units_at(self.decimals, Round::Down).ok_or(BadDeposit::TooLarge(self.amount))?;
        u128::try_from(units).map_err(|_| BadDeposit::TooLarge(self.amount))
    }

    /// What the wallet sends.
    pub fn plan(&self) -> Result<DepositPlan, BadDeposit> {
        let vault = vault(self.chain_id).ok_or(BadDeposit::NoVault(self.chain_id))?;
        let units = self.units()?;
        let broker = if self.to_spot { BROKER_SPOT } else { BROKER_FUTURES };
        let native = self.currency == NATIVE_TOKEN;
        let approve = (!native).then(|| WalletTx {
            chain_id: self.chain_id,
            to: self.currency,
            data: approve_calldata(vault.address, units),
            value: 0,
        });
        let deposit = WalletTx {
            chain_id: self.chain_id,
            to: vault.address,
            data: deposit_for_calldata(self.currency, self.for_address, units, broker),
            value: if native { units } else { 0 },
        };
        Ok(DepositPlan { approve, deposit, units })
    }
}

/// An asset a chain takes for deposits, or pays out in withdrawals.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainAsset {
    pub name: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub display_name: String,
    /// The token's contract: a hex address on an EVM chain ([`NATIVE_TOKEN`] for its own coin),
    /// a mint on Solana.
    #[serde(default, deserialize_with = "lenient::text")]
    pub contract_address: String,
    pub decimals: u32,
    /// `EVM` or `SOLANA`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub network: String,
    pub chain_id: u64,
    #[serde(default)]
    pub deposit_type: Option<String>,
    #[serde(default)]
    pub withdraw_type: Option<String>,
    /// Where the web app lists it; lower first.
    #[serde(default, deserialize_with = "lenient::int")]
    pub rank: i64,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub is_native: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub is_stock: bool,
    #[serde(default, deserialize_with = "figure")]
    pub min_deposit: Option<Dec>,
}

impl ChainAsset {
    /// The contract as an EVM address, when it is one.
    #[must_use]
    pub fn evm_address(&self) -> Option<Address> {
        self.contract_address.parse().ok()
    }
}

fn assets_call(path: &'static str, chain_ids: &[u64], networks: &[&str], account_type: &str) -> Call {
    let ids: Vec<String> = chain_ids.iter().map(u64::to_string).collect();
    Call::get(path)
        .on(Host::Web)
        .field("chainIds", ids.join(","))
        .maybe("networks", (!networks.is_empty()).then(|| networks.join(",")))
        .field("accountType", account_type)
}

/// The assets the chains take for deposits to `account_type` (`perp` or `spot`); `networks`
/// (`EVM`, `SOLANA`) narrows it when given.
#[must_use]
pub fn deposit_assets_call(chain_ids: &[u64], networks: &[&str], account_type: &str) -> Call {
    assets_call("/bapi/futures/v1/public/future/aster/deposit/assets", chain_ids, networks, account_type)
}

pub async fn deposit_assets(
    transport: &impl Transport,
    chain_ids: &[u64],
    networks: &[&str],
    account_type: &str,
) -> Result<Vec<ChainAsset>, ApiError> {
    let answer: WebAnswer<Vec<ChainAsset>> =
        read(transport, deposit_assets_call(chain_ids, networks, account_type)).await?;
    Ok(answer.data)
}

/// The assets the chains pay out from `account_type`.
#[must_use]
pub fn withdraw_assets_call(chain_ids: &[u64], networks: &[&str], account_type: &str) -> Call {
    assets_call("/bapi/futures/v1/public/future/aster/withdraw/assets", chain_ids, networks, account_type)
}

pub async fn withdraw_assets(
    transport: &impl Transport,
    chain_ids: &[u64],
    networks: &[&str],
    account_type: &str,
) -> Result<Vec<ChainAsset>, ApiError> {
    let answer: WebAnswer<Vec<ChainAsset>> =
        read(transport, withdraw_assets_call(chain_ids, networks, account_type)).await?;
    Ok(answer.data)
}

/// The account's own deposit address on a network.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DepositAddress {
    pub network: String,
    pub address: String,
}

/// `network` is `SUI` when not given.
#[must_use]
pub fn user_deposit_address_call(network: Option<&str>) -> Call {
    Call::get("/aster-chain/v3/spot/user-deposit-address").on(Host::Chain).maybe("network", network).signed()
}

pub async fn user_deposit_address(
    transport: &impl Transport,
    network: Option<&str>,
) -> Result<DepositAddress, ApiError> {
    read(transport, user_deposit_address_call(network)).await
}

#[cfg(test)]
#[path = "tests/deposit.rs"]
mod tests;
