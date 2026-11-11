//! To other Aster Chain addresses, and the account's privacy status, on Aster Chain's API.
//!
//! - `POST /aster-chain/v3/transfer`: one transfer to a registered Aster Chain address, signed by
//!   the main wallet, weight 50.
//! - `POST /aster-chain/v3/batch-transfer`: up to ten in one request, signed by the main wallet,
//!   weight 50. It is taken whole or refused whole, then carried out on its own; its
//!   `clientTranId` (20 to 64 characters) makes sending it again harmless.
//! - `GET /aster-chain/v3/batch-transfer/history`: batches and each item's result, agent-signed,
//!   weight 5.
//! - `GET /aster-chain/v3/account/status` and `POST /aster-chain/v3/account/modify-status`:
//!   whether the account is `PUBLIC` or `PRIVATE` on the chain, agent-signed. A private account
//!   cannot send a batch.

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Call, Host, Transport, read};
use crate::utils::lenient;
use crate::utils::{Address, Dec};
#[cfg(feature = "sign")]
use crate::{
    client::{Client, Credentials, Http},
    transfer::{TransferError, sign_with_wallet},
    utils::sign::Wallet,
};

/// The most items one batch carries.
pub const MOST_IN_BATCH: usize = 10;

/// Whether the account's activity is shown on Aster Chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Privacy {
    Public,
    Private,
}

impl Privacy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Privacy::Public => "PUBLIC",
            Privacy::Private => "PRIVATE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct PrivacyStatus {
    pub status: Privacy,
}

#[must_use]
pub fn account_status_call() -> Call {
    Call::get("/aster-chain/v3/account/status").on(Host::Chain).signed()
}

pub async fn account_status(transport: &impl Transport) -> Result<PrivacyStatus, ApiError> {
    read(transport, account_status_call()).await
}

#[must_use]
pub fn modify_account_status_call(status: Privacy) -> Call {
    Call::post("/aster-chain/v3/account/modify-status").on(Host::Chain).field("status", status.as_str()).signed()
}

/// Gives the status after the change, which is broadcast to the chain.
pub async fn modify_account_status(transport: &impl Transport, status: Privacy) -> Result<PrivacyStatus, ApiError> {
    read(transport, modify_account_status_call(status)).await
}

/// One transfer to another Aster Chain address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTransfer {
    pub asset: String,
    pub amount: Dec,
    pub to: Address,
    /// The caller's id for it; the venue makes one when there is none.
    pub client_tran_id: Option<String>,
}

impl ChainTransfer {
    /// `POST /aster-chain/v3/transfer` before the main wallet signs it.
    #[must_use]
    pub fn call(&self) -> Call {
        Call::post("/aster-chain/v3/transfer")
            .on(Host::Chain)
            .field("asset", &self.asset)
            .field("amount", self.amount)
            .field("toAddress", self.to.to_checksum())
            .maybe("clientTranId", self.client_tran_id.as_deref())
    }
}

/// A transfer to an address the venue took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainTransferred {
    #[serde(deserialize_with = "lenient::id_text")]
    pub transfer_id: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub to_address: String,
    #[serde(default)]
    pub timestamp: Option<u64>,
    /// `SUCCESS` or `PENDING`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
}

/// Signs `transfer` with the main wallet at the client's next nonce and sends it once.
#[cfg(feature = "sign")]
pub async fn transfer_to_address<H: Http, K: Credentials>(
    client: &Client<H, K>,
    wallet: &impl Wallet,
    transfer: &ChainTransfer,
) -> Result<ChainTransferred, TransferError> {
    if !transfer.amount.is_positive() {
        return Err(TransferError::Invalid(format!("a transfer's amount is positive, not {}", transfer.amount)));
    }
    let call = sign_with_wallet(transfer.call(), client.next_nonce(), wallet).await?;
    Ok(read(client, call).await?)
}

/// One item of a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    #[serde(serialize_with = "checksummed")]
    pub to_address: Address,
    pub asset: String,
    /// At most eight places.
    pub amount: Dec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_chain_id: Option<u64>,
}

fn checksummed<S: serde::Serializer>(address: &Address, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&address.to_checksum())
}

impl BatchItem {
    /// Why the venue would refuse the batch for this item, if it would: an amount that is not
    /// positive or has more than eight places.
    #[must_use]
    pub fn fault(&self) -> Option<String> {
        if !self.amount.is_positive() {
            return Some(format!("an item's amount is positive, not {}", self.amount));
        }
        (self.amount.places() > 8).then(|| format!("an item's amount has at most eight places, not {}", self.amount))
    }
}

/// `POST /aster-chain/v3/batch-transfer` before the main wallet signs it: `items` as the JSON
/// list the venue reads. Refuses an empty or oversized batch, a bad item, an id outside 20 to 64
/// characters, and the same address and asset twice.
pub fn batch_transfer_call(client_tran_id: &str, items: &[BatchItem]) -> Result<Call, String> {
    if !(20..=64).contains(&client_tran_id.chars().count()) {
        return Err(format!("a batch's clientTranId has 20 to 64 characters, not {}", client_tran_id.chars().count()));
    }
    if items.is_empty() || items.len() > MOST_IN_BATCH {
        return Err(format!("a batch carries 1 to {MOST_IN_BATCH} items, not {}", items.len()));
    }
    if let Some(fault) = items.iter().find_map(BatchItem::fault) {
        return Err(fault);
    }
    for (at, item) in items.iter().enumerate() {
        let twice = items[..at]
            .iter()
            .any(|seen| seen.to_address == item.to_address && seen.asset.eq_ignore_ascii_case(&item.asset));
        if twice {
            return Err(format!("{} {} appears twice in the batch", item.to_address.to_checksum(), item.asset));
        }
    }
    let list = serde_json::to_string(items).map_err(|error| error.to_string())?;
    Ok(Call::post("/aster-chain/v3/batch-transfer")
        .on(Host::Chain)
        .field("clientTranId", client_tran_id)
        .field("items", list))
}

/// A batch the venue accepted: not yet carried out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchAccepted {
    pub client_tran_id: String,
    /// `I` pending, `P` processing, `S` done, `F` failed, `M` partly done.
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
    #[serde(default)]
    pub total_count: u32,
    #[serde(default, deserialize_with = "lenient::list")]
    pub items: Vec<AcceptedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedItem {
    #[serde(default, deserialize_with = "lenient::text")]
    pub to_address: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::id_text")]
    pub transfer_id: String,
}

/// Signs the batch with the main wallet at the client's next nonce and sends it once.
#[cfg(feature = "sign")]
pub async fn batch_transfer<H: Http, K: Credentials>(
    client: &Client<H, K>,
    wallet: &impl Wallet,
    client_tran_id: &str,
    items: &[BatchItem],
) -> Result<BatchAccepted, TransferError> {
    let call = batch_transfer_call(client_tran_id, items).map_err(TransferError::Invalid)?;
    let call = sign_with_wallet(call, client.next_nonce(), wallet).await?;
    Ok(read(client, call).await?)
}

/// Which batches to read: by id, or by creation time (31 days at most; the last 7 by default).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchQuery {
    pub client_tran_id: Option<String>,
    pub status: Option<String>,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    /// From 1.
    pub page: Option<u32>,
    /// Up to 100; 20 by default.
    pub limit: Option<u32>,
}

#[must_use]
pub fn batch_history_call(query: &BatchQuery) -> Call {
    Call::get("/aster-chain/v3/batch-transfer/history")
        .on(Host::Chain)
        .maybe("clientTranId", query.client_tran_id.as_deref())
        .maybe("status", query.status.as_deref())
        .maybe("startTime", query.start_time)
        .maybe("endTime", query.end_time)
        .maybe("page", query.page)
        .maybe("limit", query.limit)
        .signed()
}

pub async fn batch_history(transport: &impl Transport, query: &BatchQuery) -> Result<BatchPage, ApiError> {
    read(transport, batch_history_call(query)).await
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BatchPage {
    #[serde(default)]
    pub total: u64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub rows: Vec<BatchRow>,
}

/// One batch and its items.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRow {
    pub client_tran_id: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
    #[serde(default)]
    pub create_time: Option<u64>,
    #[serde(default)]
    pub update_time: Option<u64>,
    #[serde(default)]
    pub total_count: u32,
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub fail_count: u32,
    #[serde(default)]
    pub partial_count: u32,
    #[serde(default)]
    pub processing_count: u32,
    #[serde(default, deserialize_with = "lenient::list")]
    pub failed_items: Vec<BatchRowItem>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub details: Vec<BatchRowItem>,
}

/// One item's result.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRowItem {
    #[serde(deserialize_with = "lenient::id_text")]
    pub transfer_id: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub to_address: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    #[serde(default)]
    pub transfer_chain_id: Option<u64>,
    /// What was moved, once it was.
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub success_amount: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub status: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub error_code: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub error_msg: String,
    #[serde(default)]
    pub create_time: Option<u64>,
    #[serde(default)]
    pub update_time: Option<u64>,
}

#[cfg(test)]
#[path = "tests/chain.rs"]
mod tests;
