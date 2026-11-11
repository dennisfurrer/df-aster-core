//! The main wallet: the account's own address, which signs what an agent may not (approving an
//! agent, a withdrawal). A browser app wraps the user's wallet (`eth_signTypedData_v4`); a native
//! program may hold the key itself ([`LocalWallet`]). Whatever it signs is recovered before it is
//! sent, so a wallet that signed as someone else is caught before the venue sees it.

use std::future::Future;

use crate::utils::Address;
use crate::utils::sign::ecdsa::Signature;
use crate::utils::sign::eip712::TypedData;

/// Why the wallet gave no signature, or one that does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalletError {
    /// The person said no, or the wallet failed.
    #[error("the wallet did not sign: {0}")]
    Declined(String),
    /// It signed, but as another address: nothing was sent.
    #[error("the wallet signed as {signed}, not as {expected}")]
    WrongSigner { expected: Address, signed: Address },
    #[error("the wallet's signature does not recover: {0}")]
    Unrecoverable(String),
}

/// Something that signs typed data as one address.
pub trait Wallet {
    fn address(&self) -> Address;

    /// `eth_signTypedData_v4` of `data` ([`TypedData::to_json`]).
    fn sign_typed_data(&self, data: &TypedData) -> impl Future<Output = Result<Signature, WalletError>>;
}

/// A wallet whose key this program holds.
#[cfg(feature = "sign")]
#[derive(Debug, Clone)]
pub struct LocalWallet(pub crate::utils::sign::PrivateKey);

#[cfg(feature = "sign")]
impl Wallet for LocalWallet {
    fn address(&self) -> Address {
        self.0.address()
    }

    fn sign_typed_data(&self, data: &TypedData) -> impl Future<Output = Result<Signature, WalletError>> {
        let signed = self.0.sign_hash(&data.hash()).map_err(|error| WalletError::Unrecoverable(error.to_string()));
        std::future::ready(signed)
    }
}

/// Has `wallet` sign `data` and checks the signature recovers to it.
#[cfg(feature = "sign")]
pub async fn sign_checked(wallet: &impl Wallet, data: &TypedData) -> Result<Signature, WalletError> {
    let signature = wallet.sign_typed_data(data).await?;
    let signed = crate::utils::sign::recover(&data.hash(), &signature)
        .map_err(|error| WalletError::Unrecoverable(error.to_string()))?;
    match signed == wallet.address() {
        true => Ok(signature),
        false => Err(WalletError::WrongSigner { expected: wallet.address(), signed }),
    }
}
