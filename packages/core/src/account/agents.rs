//! What the main wallet signs itself, never an agent: approving, updating and removing an agent
//! or a builder, registering an agent in one call, and moving another account's assets here;
//! and the lists of agents and builders, which an agent reads.
//!
//! **A main wallet action** ([`MainAction`]) is signed as Aster's web app and its Python demo
//! (`demo/aster-code.py`) sign it: the action's parameters in order, then `asterChain`
//! (`Mainnet`), `user` and `nonce` (microseconds), become a struct whose type is the action's name
//! (`ApproveAgent`, `DelBuilder`, ...), each key with its first letter in upper case, typed from
//! its value (a flag `bool`, a whole number `uint256`, anything else `string`), under
//! `{AsterSignTransaction, "1", 56, 0x0}`. The request carries the parameters with their own
//! keys, then `signature` and `signatureChainId=56`, in the query string, as the web app sends
//! them.
//!
//! **Registering an agent** (`registerAndApproveAgent`) and **moving another account's assets
//! here** (`asset/migrateUser`) sign one string instead, `user=..&nonce=..&...`, as
//! `Message { msg }`, as Aster's API-demo repository does.
//!
//! Before anything is sent the wallet's signature is recovered and must be the account's own.

use serde::Deserialize;

#[cfg(feature = "sign")]
use crate::account::WalletCallError;
use crate::client::hosts::{ASTER_CHAIN, WALLET_CHAIN_ID};
use crate::client::http::{ApiError, Call, Method, Transport, read};
#[cfg(feature = "sign")]
use crate::utils::Done;
use crate::utils::lenient;
use crate::utils::sign::agent::message_on;
use crate::utils::sign::{DOMAIN_NAME, DOMAIN_VERSION, Domain, Field, Signature, Struct, TypedData};
#[cfg(feature = "sign")]
use crate::utils::sign::{Wallet, WalletError, sign_checked};
use crate::utils::time::DAY_MS;
use crate::utils::{Address, Dec};

/// How long an approval lasts unless asked otherwise: thirty days, as Aster's web app asks.
pub const AGENT_TTL_MS: u64 = 30 * DAY_MS;

/// When an approval made at `now_ms` for `ttl_ms` lapses: the `expired` an approval carries.
#[must_use]
pub const fn agent_expiry(now_ms: u64, ttl_ms: u64) -> u64 {
    now_ms.saturating_add(ttl_ms)
}

/// A fresh agent key from the system's randomness. Approve its address before it signs.
#[cfg(feature = "sign")]
pub fn new_agent() -> Result<crate::utils::sign::PrivateKey, crate::utils::sign::BadKey> {
    crate::utils::sign::PrivateKey::random()
}

/// An action the main wallet signs as typed data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainAction {
    /// The struct's name, which names the action: `ApproveAgent`, `UpdateAgent`, `DelAgent`,
    /// `ApproveBuilder`, `UpdateBuilder`, `DelBuilder`.
    pub primary_type: &'static str,
    pub method: Method,
    pub path: &'static str,
    /// The parameters with the keys the request carries, in the order they are signed.
    pub params: Vec<(&'static str, Field)>,
    /// The main account, which signs.
    pub user: Address,
    /// Microseconds, like every Aster nonce.
    pub nonce: u64,
}

/// What an approval of an agent grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveAgent {
    /// The name the account's agent list shows. An approval under a name replaces the agent
    /// that held it.
    pub agent_name: String,
    pub agent: Address,
    /// IPs or CIDR ranges the agent may call from; empty for any.
    pub ip_whitelist: String,
    /// When the approval lapses, unix ms ([`agent_expiry`]).
    pub expired_ms: u64,
    pub can_spot_trade: bool,
    pub can_perp_trade: bool,
    pub can_withdraw: bool,
}

impl ApproveAgent {
    /// An agent that trades perps only, from any IP, until `expired_ms`.
    #[must_use]
    pub fn perps_only(agent_name: &str, agent: Address, expired_ms: u64) -> Self {
        ApproveAgent {
            agent_name: agent_name.to_owned(),
            agent,
            ip_whitelist: String::new(),
            expired_ms,
            can_spot_trade: false,
            can_perp_trade: true,
            can_withdraw: false,
        }
    }
}

impl MainAction {
    fn new(primary_type: &'static str, method: Method, path: &'static str, user: Address, nonce: u64) -> Self {
        MainAction { primary_type, method, path, params: Vec::new(), user, nonce }
    }

    fn param(mut self, key: &'static str, value: Field) -> Self {
        self.params.push((key, value));
        self
    }

    /// `POST /fapi/v3/approveAgent`: lets `approval.agent` act for the account.
    #[must_use]
    pub fn approve_agent(user: Address, nonce: u64, approval: &ApproveAgent) -> Self {
        MainAction::new("ApproveAgent", Method::Post, "/fapi/v3/approveAgent", user, nonce)
            .param("agentName", Field::String(approval.agent_name.clone()))
            .param("agentAddress", Field::String(approval.agent.to_checksum()))
            .param("ipWhitelist", Field::String(approval.ip_whitelist.clone()))
            .param("expired", Field::Uint256(u128::from(approval.expired_ms)))
            .param("canSpotTrade", Field::Bool(approval.can_spot_trade))
            .param("canPerpTrade", Field::Bool(approval.can_perp_trade))
            .param("canWithdraw", Field::Bool(approval.can_withdraw))
    }

    /// `POST /fapi/v3/updateAgent`: an approved agent's IPs and permissions.
    #[must_use]
    pub fn update_agent(
        user: Address,
        nonce: u64,
        agent: Address,
        ip_whitelist: &str,
        can_spot_trade: bool,
        can_perp_trade: bool,
        can_withdraw: bool,
    ) -> Self {
        MainAction::new("UpdateAgent", Method::Post, "/fapi/v3/updateAgent", user, nonce)
            .param("agentAddress", Field::String(agent.to_checksum()))
            .param("ipWhitelist", Field::String(ip_whitelist.to_owned()))
            .param("canSpotTrade", Field::Bool(can_spot_trade))
            .param("canPerpTrade", Field::Bool(can_perp_trade))
            .param("canWithdraw", Field::Bool(can_withdraw))
    }

    /// `DELETE /fapi/v3/agent`: the agent may no longer act for the account.
    #[must_use]
    pub fn delete_agent(user: Address, nonce: u64, agent: Address) -> Self {
        MainAction::new("DelAgent", Method::Delete, "/fapi/v3/agent", user, nonce)
            .param("agentAddress", Field::String(agent.to_checksum()))
    }

    /// `POST /fapi/v3/approveBuilder`: lets `builder` charge up to `max_fee_rate` (a fraction of
    /// the notional) on the orders that name it.
    #[must_use]
    pub fn approve_builder(user: Address, nonce: u64, builder: Address, max_fee_rate: Dec, builder_name: &str) -> Self {
        MainAction::new("ApproveBuilder", Method::Post, "/fapi/v3/approveBuilder", user, nonce)
            .param("builder", Field::String(builder.to_checksum()))
            .param("maxFeeRate", Field::String(max_fee_rate.to_wire()))
            .param("builderName", Field::String(builder_name.to_owned()))
    }

    /// `POST /fapi/v3/updateBuilder`: a builder's highest fee rate.
    #[must_use]
    pub fn update_builder(user: Address, nonce: u64, builder: Address, max_fee_rate: Dec) -> Self {
        MainAction::new("UpdateBuilder", Method::Post, "/fapi/v3/updateBuilder", user, nonce)
            .param("builder", Field::String(builder.to_checksum()))
            .param("maxFeeRate", Field::String(max_fee_rate.to_wire()))
    }

    /// `DELETE /fapi/v3/builder`: the builder may no longer charge the account.
    #[must_use]
    pub fn delete_builder(user: Address, nonce: u64, builder: Address) -> Self {
        MainAction::new("DelBuilder", Method::Delete, "/fapi/v3/builder", user, nonce)
            .param("builder", Field::String(builder.to_checksum()))
    }

    /// Every field that is signed: the parameters, then `asterChain`, `user` and `nonce`.
    fn signed_fields(&self) -> Vec<(&'static str, Field)> {
        let mut fields = self.params.clone();
        fields.push(("asterChain", Field::String(ASTER_CHAIN.to_owned())));
        fields.push(("user", Field::String(self.user.to_checksum())));
        fields.push(("nonce", Field::Uint256(u128::from(self.nonce))));
        fields
    }

    /// What the main wallet signs, and is shown.
    #[must_use]
    pub fn typed_data(&self) -> TypedData {
        let message = self
            .signed_fields()
            .into_iter()
            .fold(Struct::new(self.primary_type), |message, (key, value)| message.field(&capitalised(key), value));
        TypedData::new(Domain::new(DOMAIN_NAME, DOMAIN_VERSION, WALLET_CHAIN_ID), message)
    }

    /// The request, carrying `signature`: every signed field with its own key, then the
    /// signature and `signatureChainId`, in the query string.
    #[must_use]
    pub fn call(&self, signature: &Signature) -> Call {
        let call = self
            .signed_fields()
            .into_iter()
            .fold(Call::new(self.method, self.path), |call, (key, value)| call.field(key, wire(&value)));
        call.field("signature", signature).field("signatureChainId", WALLET_CHAIN_ID).query_only()
    }
}

/// `agentName` as `AgentName`: what the typed data calls a key.
fn capitalised(key: &str) -> String {
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A field as the request writes it.
fn wire(value: &Field) -> String {
    match value {
        Field::String(text) => text.clone(),
        Field::Uint256(number) => number.to_string(),
        Field::Bool(flag) => flag.to_string(),
        Field::Address(address) => address.to_checksum(),
    }
}

/// Has `wallet` sign `action` and sends it. The wallet must be the action's user, and its
/// signature must recover to it: otherwise nothing is sent.
#[cfg(feature = "sign")]
pub async fn send_main_action<R: serde::de::DeserializeOwned>(
    transport: &impl Transport,
    wallet: &impl Wallet,
    action: &MainAction,
) -> Result<R, WalletCallError> {
    if wallet.address() != action.user {
        return Err(WalletError::WrongSigner { expected: action.user, signed: wallet.address() }.into());
    }
    let signature = sign_checked(wallet, &action.typed_data()).await?;
    Ok(read(transport, action.call(&signature)).await?)
}

/// The chain id Aster's API-demo signs a registration and a migration under (its
/// `EIP712_DOMAIN`). The API docs say a registration is signed under `signatureChainId`, 56; the
/// demo signs under 1666 while still sending `signatureChainId=56`. This follows the demo;
/// [`RegisterAgent::sign_chain_id`] takes the other.
pub const DEMO_SIGN_CHAIN_ID: u64 = crate::utils::sign::API_CHAIN_ID;

/// `registerAndApproveAgent`: an agent registered and approved in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterAgent {
    pub user: Address,
    /// Microseconds; it may be at most ten seconds from the venue's clock.
    pub nonce: u64,
    pub approval: ApproveAgent,
    /// A referral code, if any.
    pub agent_code: Option<String>,
    /// The chain id of the domain the wallet signs under: [`DEMO_SIGN_CHAIN_ID`] by
    /// [`RegisterAgent::new`].
    pub sign_chain_id: u64,
}

impl RegisterAgent {
    #[must_use]
    pub fn new(user: Address, nonce: u64, approval: ApproveAgent, agent_code: Option<String>) -> Self {
        RegisterAgent { user, nonce, approval, agent_code, sign_chain_id: DEMO_SIGN_CHAIN_ID }
    }

    /// The string the main wallet signs, its values as they are, never encoded. `ipWhitelist` is
    /// in it only when there is one, as in the demo.
    #[must_use]
    pub fn msg(&self) -> String {
        let approval = &self.approval;
        let mut msg = format!(
            "user={}&nonce={}&agentName={}&agentAddress={}&expired={}&signatureChainId={}&canSpotTrade={}\
             &canPerpTrade={}&canWithdraw={}",
            self.user.to_checksum(),
            self.nonce,
            approval.agent_name,
            approval.agent.to_checksum(),
            approval.expired_ms,
            WALLET_CHAIN_ID,
            approval.can_spot_trade,
            approval.can_perp_trade,
            approval.can_withdraw,
        );
        if !approval.ip_whitelist.is_empty() {
            msg.push_str("&ipWhitelist=");
            msg.push_str(&approval.ip_whitelist);
        }
        msg
    }

    /// `Message { msg }` under the signing chain id: what the main wallet signs.
    #[must_use]
    pub fn typed_data(&self) -> TypedData {
        message_on(&self.msg(), self.sign_chain_id)
    }

    /// `POST /fapi/v3/registerAndApproveAgent`, weight 50, with `signature`, in a form body, in
    /// the demo's order.
    #[must_use]
    pub fn call(&self, signature: &Signature) -> Call {
        let approval = &self.approval;
        let ip_whitelist = Some(approval.ip_whitelist.as_str()).filter(|ips| !ips.is_empty());
        Call::post("/fapi/v3/registerAndApproveAgent")
            .field("user", self.user.to_checksum())
            .field("nonce", self.nonce)
            .field("agentName", &approval.agent_name)
            .field("agentAddress", approval.agent.to_checksum())
            .field("expired", approval.expired_ms)
            .field("signatureChainId", WALLET_CHAIN_ID)
            .field("signature", signature)
            .field("canSpotTrade", approval.can_spot_trade)
            .field("canPerpTrade", approval.can_perp_trade)
            .field("canWithdraw", approval.can_withdraw)
            .maybe("ipWhitelist", ip_whitelist)
            .maybe("agentCode", self.agent_code.as_deref().filter(|code| !code.is_empty()))
    }
}

/// Registers and approves an agent, signed by `wallet`, which must be the user. A withdrawal
/// permission needs an IP whitelist.
#[cfg(feature = "sign")]
pub async fn register_and_approve_agent(
    transport: &impl Transport,
    wallet: &impl Wallet,
    register: &RegisterAgent,
) -> Result<Done, WalletCallError> {
    if wallet.address() != register.user {
        return Err(WalletError::WrongSigner { expected: register.user, signed: wallet.address() }.into());
    }
    let signature = sign_checked(wallet, &register.typed_data()).await?;
    Ok(read(transport, register.call(&signature)).await?)
}

/// `GET /fapi/v3/agent`.
#[must_use]
pub fn agents_call() -> Call {
    Call::get("/fapi/v3/agent").signed()
}

/// The account's agents. The docs do not describe the answer, so it comes back as JSON.
pub async fn agents(transport: &impl Transport) -> Result<serde_json::Value, ApiError> {
    read(transport, agents_call()).await
}

/// `GET /fapi/v3/builder`.
#[must_use]
pub fn builders_call() -> Call {
    Call::get("/fapi/v3/builder").signed()
}

/// The builders the account approved. The docs do not describe the answer, so it comes back as
/// JSON.
pub async fn builders(transport: &impl Transport) -> Result<serde_json::Value, ApiError> {
    read(transport, builders_call()).await
}

/// `asset/migrateUser`, weight 50: every asset with a balance moves from `user`, the account
/// whose wallet signs, to the account the request is authenticated as. The source may hold no
/// position and no open order; at most 300 assets go in a batch. Aster's Python demo signs it as
/// a typed `UserAssetMigrate` struct; its API-demo and the API docs sign `user=..&nonce=..` as
/// `Message { msg }` and send `user`, `nonce` and `signature` alone, which this follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrateUser {
    /// The source account.
    pub user: Address,
    /// Microseconds.
    pub nonce: u64,
}

impl MigrateUser {
    #[must_use]
    pub fn msg(&self) -> String {
        format!("user={}&nonce={}", self.user.to_checksum(), self.nonce)
    }

    /// `Message { msg }` on chain 1666, the demo's domain.
    #[must_use]
    pub fn typed_data(&self) -> TypedData {
        message_on(&self.msg(), DEMO_SIGN_CHAIN_ID)
    }

    /// `POST /fapi/v3/asset/migrateUser` with `signature`, in a form body.
    #[must_use]
    pub fn call(&self, signature: &Signature) -> Call {
        Call::post("/fapi/v3/asset/migrateUser")
            .field("user", self.user.to_checksum())
            .field("nonce", self.nonce)
            .field("signature", signature)
    }
}

/// Moves the assets of the account `wallet` holds (`migrate.user`) to this client's account.
#[cfg(feature = "sign")]
pub async fn migrate_user(
    transport: &impl Transport,
    wallet: &impl Wallet,
    migrate: &MigrateUser,
) -> Result<Migrated, WalletCallError> {
    if wallet.address() != migrate.user {
        return Err(WalletError::WrongSigner { expected: migrate.user, signed: wallet.address() }.into());
    }
    let signature = sign_checked(wallet, &migrate.typed_data()).await?;
    Ok(read(transport, migrate.call(&signature)).await?)
}

/// What a migration answers: the batch to follow, or none when the source held nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Migrated {
    #[serde(default)]
    pub batch_id: Option<String>,
}

/// A migration batch and each asset in it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateHistory {
    pub batch_id: String,
    #[serde(default)]
    pub total_count: u64,
    #[serde(default)]
    pub success_count: u64,
    #[serde(default)]
    pub processing_count: u64,
    #[serde(default)]
    pub fail_count: u64,
    /// Waiting to start.
    #[serde(default)]
    pub init_count: u64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub details: Vec<MigrateDetail>,
}

/// One asset's move.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateDetail {
    pub id: u64,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    /// None until it is processed.
    #[serde(default)]
    pub tran_id: Option<u64>,
    /// `I` waiting, `S` done, `F` failed.
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
    /// Each leg: `S` done, `F` failed, `P` in progress; empty before it starts.
    #[serde(default, deserialize_with = "lenient::text")]
    pub from_status: String,
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub from_error_code: String,
    #[serde(default)]
    pub from_response: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub to_status: String,
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub to_error_code: String,
    #[serde(default)]
    pub to_response: Option<serde_json::Value>,
}

impl MigrateDetail {
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.status == "S"
    }

    #[must_use]
    pub fn has_failed(&self) -> bool {
        self.status == "F"
    }
}

/// `GET /fapi/v3/asset/migrateUser/history`, weight 50.
#[must_use]
pub fn migrate_history_call(batch_id: &str) -> Call {
    Call::get("/fapi/v3/asset/migrateUser/history").field("batchId", batch_id).signed()
}

pub async fn migrate_history(transport: &impl Transport, batch_id: &str) -> Result<MigrateHistory, ApiError> {
    read(transport, migrate_history_call(batch_id)).await
}

#[cfg(test)]
#[path = "tests/agents.rs"]
mod tests;
