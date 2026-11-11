//! Sub-accounts: binding an existing wallet's account under the main account
//! (`sub-accounts/bind`), creating one (`createSubAccount`), listing them (`getSubAccountList`)
//! and renaming or freezing one (`updateSubAccount`).
//!
//! Binding and creating take two signatures: the sub-account's wallet signs the call's string,
//! then the main wallet signs the same string with `&childSignature=..` appended. Updating takes
//! the main wallet's alone. Each is `Message { msg }` under `{AsterSignTransaction, "1", 1666,
//! 0x0}`, the string's values written as they are; each signature is recovered before anything
//! is sent. The strings, the domain and the fields sent follow Aster's API-demo repository
//! (`futures-v3-demo/59` to `62`), which agrees with the API docs here. The list is an agent's read like any other. Binding is open only to addresses Aster
//! has whitelisted.

use serde::Deserialize;

#[cfg(feature = "sign")]
use crate::account::WalletCallError;
use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::lenient;
#[cfg(feature = "sign")]
use crate::utils::sign::agent::message;
#[cfg(feature = "sign")]
use crate::utils::sign::{Wallet, WalletError, sign_checked};
#[cfg(feature = "sign")]
use crate::utils::{Address, Done};

/// One entry of the list: the main account or a sub-account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAccount {
    pub account_id: u64,
    /// Empty for the main account.
    #[serde(default, deserialize_with = "lenient::text")]
    pub sub_account_name: String,
    /// Whether this entry is the main account.
    #[serde(default, deserialize_with = "lenient::flag")]
    pub parent_account: bool,
}

/// `GET /fapi/v3/getSubAccountList`.
#[must_use]
pub fn sub_accounts_call() -> Call {
    Call::get("/fapi/v3/getSubAccountList").signed()
}

/// The main account and its sub-accounts.
pub async fn sub_accounts(transport: &impl Transport) -> Result<Vec<SubAccount>, ApiError> {
    read(transport, sub_accounts_call()).await
}

/// A sub-account's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubAccountStatus {
    Normal,
    /// It cannot start a transfer.
    Frozen,
}

impl SubAccountStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SubAccountStatus::Normal => "NORMAL",
            SubAccountStatus::Frozen => "FROZEN",
        }
    }
}

/// `k=v&k=v`, values as they are: the strings these calls sign.
#[cfg(feature = "sign")]
fn joined(fields: &[(&'static str, String)]) -> String {
    fields.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join("&")
}

/// Has `wallet`, which must be `expected`, sign `msg`.
#[cfg(feature = "sign")]
async fn signed_by(
    wallet: &impl Wallet,
    expected: Address,
    msg: &str,
) -> Result<crate::utils::sign::Signature, WalletError> {
    if wallet.address() != expected {
        return Err(WalletError::WrongSigner { expected, signed: wallet.address() });
    }
    sign_checked(wallet, &message(msg)).await
}

/// The child's signature over `fields`, then the main wallet's over the same with
/// `childSignature` appended, as a POST to `path`.
#[cfg(feature = "sign")]
async fn twice_signed(
    path: &'static str,
    fields: Vec<(&'static str, String)>,
    main: &impl Wallet,
    user: Address,
    child: &impl Wallet,
    child_address: Address,
) -> Result<Call, WalletError> {
    let child_signature = signed_by(child, child_address, &joined(&fields)).await?.to_hex();
    let mut fields = fields;
    fields.push(("childSignature", child_signature));
    let signature = signed_by(main, user, &joined(&fields)).await?;
    let call = fields.into_iter().fold(Call::post(path), |call, (key, value)| call.field(key, value));
    Ok(call.field("signature", signature))
}

/// Binds `child`'s account under `main` as `name`. `nonce` is microseconds
/// ([`crate::Client::next_nonce`]).
#[cfg(feature = "sign")]
pub async fn bind_sub_account(
    transport: &impl Transport,
    main: &impl Wallet,
    child: &impl Wallet,
    name: &str,
    nonce: u64,
) -> Result<Done, WalletCallError> {
    let (user, child_address) = (main.address(), child.address());
    let fields = vec![
        ("childAddress", child_address.to_checksum()),
        ("name", name.to_owned()),
        ("nonce", nonce.to_string()),
        ("user", user.to_checksum()),
    ];
    let call = twice_signed("/fapi/v3/sub-accounts/bind", fields, main, user, child, child_address).await?;
    Ok(read(transport, call).await?)
}

/// Creates a sub-account for `child`'s address under `main`, as `name`. `signer` is the main
/// account's agent.
#[cfg(feature = "sign")]
pub async fn create_sub_account(
    transport: &impl Transport,
    main: &impl Wallet,
    child: &impl Wallet,
    name: &str,
    signer: Address,
    nonce: u64,
) -> Result<Done, WalletCallError> {
    let (user, child_address) = (main.address(), child.address());
    let fields = vec![
        ("subAccountName", name.to_owned()),
        ("subSourceAddr", child_address.to_checksum()),
        ("nonce", nonce.to_string()),
        ("user", user.to_checksum()),
        ("signer", signer.to_checksum()),
    ];
    let mut call = twice_signed("/fapi/v3/createSubAccount", fields, main, user, child, child_address).await?;
    // Signed with the name first; sent with the address first, as Aster's API-demo sends it.
    call.fields.swap(0, 1);
    Ok(read(transport, call).await?)
}

/// Renames or freezes the sub-account at `sub`, signed by `main`. At least one of `name` and
/// `status` must be given.
#[cfg(feature = "sign")]
pub async fn update_sub_account(
    transport: &impl Transport,
    main: &impl Wallet,
    sub: Address,
    name: Option<&str>,
    status: Option<SubAccountStatus>,
    signer: Address,
    nonce: u64,
) -> Result<Done, WalletCallError> {
    let user = main.address();
    let mut fields = vec![
        ("subSourceAddr", sub.to_checksum()),
        ("nonce", nonce.to_string()),
        ("user", user.to_checksum()),
        ("signer", signer.to_checksum()),
    ];
    if let Some(name) = name {
        fields.push(("subAccountName", name.to_owned()));
    }
    if let Some(status) = status {
        fields.push(("status", status.as_str().to_owned()));
    }
    let signature = signed_by(main, user, &joined(&fields)).await?;
    let call =
        fields.into_iter().fold(Call::post("/fapi/v3/updateSubAccount"), |call, (key, value)| call.field(key, value));
    Ok(read(transport, call.field("signature", signature)).await?)
}

#[cfg(test)]
#[path = "tests/subaccounts.rs"]
mod tests;
