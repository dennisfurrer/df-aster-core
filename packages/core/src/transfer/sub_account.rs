//! Between a main account and its sub-accounts: `POST /fapi/v3/subAccountTransfer`, weight 5.
//!
//! The docs fix the order of what is signed, and it differs from every other call's: the fields,
//! then `nonce`, `user` and `signer`, and only then `fromAccountAddress` when there is one
//! (`toAccountAddress=..&asset=..&amount=..&kindType=..&nonce=..&user=..&signer=..&fromAccountAddress=..`).
//! So the agent signs it here, in that order, rather than through the client's usual signing.
//!
//! Who may move what:
//!
//! | From | To | `user` | `from` |
//! |---|---|---|---|
//! | main | sub | main | none |
//! | sub | main | main, or the sub | the sub, or none when `user` is the sub |
//! | sub A | sub B | main, or sub A | sub A, or none when `user` is sub A |
//!
//! The sub-account must not be frozen, and no address outside the main account's family is taken.

use crate::transfer::Direction;
use crate::utils::{Address, Dec};
#[cfg(feature = "sign")]
use crate::{
    client::{ApiError, Call, Client, Http, read},
    transfer::TransferError,
    utils::{Done, sign::Agent},
};

/// One transfer within an account family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAccountTransfer {
    pub to: Address,
    pub asset: String,
    pub amount: Dec,
    /// Any of the four: futures or spot, on each side.
    pub direction: Direction,
    /// The source, when it is not the agent's own account.
    pub from: Option<Address>,
}

/// The query string the agent signs for `transfer` at `nonce`, as the docs order it.
#[must_use]
pub fn sub_account_message(transfer: &SubAccountTransfer, nonce: u64, user: Address, signer: Address) -> String {
    crate::utils::query(&fields(transfer, nonce, user, signer))
}

fn fields(transfer: &SubAccountTransfer, nonce: u64, user: Address, signer: Address) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("toAccountAddress", transfer.to.to_checksum()),
        ("asset", transfer.asset.clone()),
        ("amount", transfer.amount.to_string()),
        ("kindType", transfer.direction.as_str().to_owned()),
        ("nonce", nonce.to_string()),
        ("user", user.to_checksum()),
        ("signer", signer.to_checksum()),
    ];
    if let Some(from) = transfer.from {
        fields.push(("fromAccountAddress", from.to_checksum()));
    }
    fields
}

/// The call for `transfer`, signed by `agent` at `nonce`: every field, then `signature`.
#[cfg(feature = "sign")]
pub fn sub_account_transfer_call(
    transfer: &SubAccountTransfer,
    nonce: u64,
    agent: &Agent,
) -> Result<Call, TransferError> {
    if !transfer.amount.is_positive() {
        return Err(TransferError::Invalid(format!("a transfer's amount is positive, not {}", transfer.amount)));
    }
    let mut fields = fields(transfer, nonce, agent.user, agent.signer());
    let msg = crate::utils::query(&fields);
    let signature = agent
        .key
        .sign_hash(&crate::utils::sign::message(&msg).hash())
        .map_err(|error| TransferError::Invalid(format!("the agent could not sign: {error}")))?;
    fields.push(("signature", signature.to_hex()));
    let mut call = Call::post("/fapi/v3/subAccountTransfer");
    call.fields = fields;
    Ok(call)
}

/// Signs `transfer` with the client's agent at its next nonce and sends it once.
#[cfg(feature = "sign")]
pub async fn sub_account_transfer<H: Http>(
    client: &Client<H, Agent>,
    transfer: &SubAccountTransfer,
) -> Result<(), TransferError> {
    let call = sub_account_transfer_call(transfer, client.next_nonce(), client.key())?;
    let _: Done = read(client, call).await.map_err(|error: ApiError| TransferError::Api(error))?;
    Ok(())
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/sub_account.rs"]
mod tests;
