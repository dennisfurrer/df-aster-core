//! The agent's signature on every account call. The call's parameters, then `nonce`, `user` and
//! `signer`, are written as a query string (each key and value as `encodeURIComponent` writes
//! it); that string is `msg` in `Message { string msg }` under
//! `{AsterSignTransaction, "1", 1666, 0x0}`, and the request carries the same string with
//! `&signature=0x..` appended. The addresses are spelled in EIP-55, as Aster's web app signs them.

#[cfg(feature = "sign")]
use crate::utils::query::query;
use crate::utils::sign::Signature;
use crate::utils::sign::eip712::{Domain, Field, Struct, TypedData};
#[cfg(feature = "sign")]
use crate::utils::{Address, sign::PrivateKey};

pub const DOMAIN_NAME: &str = "AsterSignTransaction";
pub const DOMAIN_VERSION: &str = "1";
/// The domain chain id of every agent signature: not a real chain's.
pub const API_CHAIN_ID: u64 = 1666;

/// `Message { msg }` under the API domain: what the agent signs for one call.
#[must_use]
pub fn message(msg: &str) -> TypedData {
    message_on(msg, API_CHAIN_ID)
}

/// `Message { msg }` under the domain on `chain_id`: a main wallet's registration signs this on
/// its own chain (56 for an EVM wallet).
#[must_use]
pub fn message_on(msg: &str, chain_id: u64) -> TypedData {
    TypedData::new(
        Domain::new(DOMAIN_NAME, DOMAIN_VERSION, chain_id),
        Struct::new("Message").field("msg", Field::String(msg.to_owned())),
    )
}

/// A signed call: the query string that was signed, and the signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub msg: String,
    pub signature: Signature,
}

impl Signed {
    /// The query (GET, DELETE) or form body (POST, PUT) the request carries.
    #[must_use]
    pub fn encoded(&self) -> String {
        format!("{}&signature={}", self.msg, self.signature)
    }
}

/// An agent: the main account it acts for and the key it signs with.
#[cfg(feature = "sign")]
#[derive(Debug, Clone)]
pub struct Agent {
    pub user: Address,
    pub key: PrivateKey,
}

#[cfg(feature = "sign")]
impl Agent {
    #[must_use]
    pub const fn new(user: Address, key: PrivateKey) -> Self {
        Agent { user, key }
    }

    /// The agent's own address, the `signer` of every call.
    #[must_use]
    pub const fn signer(&self) -> Address {
        self.key.address()
    }
}

/// Signs `params` at `nonce` (microseconds) as `agent`: the parameters, then `nonce`, `user` and
/// `signer`.
#[cfg(feature = "sign")]
pub fn sign_params(
    params: &[(&str, String)],
    nonce: u64,
    agent: &Agent,
) -> Result<Signed, crate::utils::sign::BadSignature> {
    let mut all = params.to_vec();
    all.push(("nonce", nonce.to_string()));
    all.push(("user", agent.user.to_checksum()));
    all.push(("signer", agent.signer().to_checksum()));
    let msg = query(&all);
    let signature = agent.key.sign_hash(&message(&msg).hash())?;
    Ok(Signed { msg, signature })
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/agent.rs"]
mod tests;
