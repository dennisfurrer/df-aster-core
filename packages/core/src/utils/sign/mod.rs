//! Signing primitives: keccak, EIP-712 for the structs Aster signs, secp256k1 keys and
//! signatures, the main wallet, and the agent's signature over a call's parameters.

pub mod agent;
pub mod ecdsa;
pub mod eip712;
pub mod keccak;
pub mod wallet;

pub use agent::{API_CHAIN_ID, DOMAIN_NAME, DOMAIN_VERSION, Signed, message};
#[cfg(feature = "sign")]
pub use agent::{Agent, sign_params};
pub use ecdsa::{BadKey, BadSignature, Signature};
#[cfg(feature = "sign")]
pub use ecdsa::{PrivateKey, recover};
pub use eip712::{Domain, Field, Struct, TypedData};
pub use keccak::keccak256;
#[cfg(feature = "sign")]
pub use wallet::{LocalWallet, sign_checked};
pub use wallet::{Wallet, WalletError};
