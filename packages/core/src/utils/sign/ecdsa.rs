//! secp256k1 for Ethereum keys: a private key and its address, a 65-byte recoverable signature
//! (`r, s, v`, `v` 27 or 28, as wallets write it), signing a 32-byte hash deterministically
//! (RFC 6979, as viem and eth_account do, so the same key and hash give the same bytes), and
//! recovering the signer. Keys and recovery need the `sign` feature; a [`Signature`] reads and
//! writes without it.

use std::fmt;
use std::str::FromStr;

use crate::utils::hex::{self, BadHex};
#[cfg(feature = "sign")]
use crate::utils::{Address, sign::keccak::keccak256};

/// A recoverable signature: `r`, `s` and `v` (27 or 28).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSignature {
    #[error(transparent)]
    Hex(#[from] BadHex),
    #[error("a signature's v is 0, 1, 27 or 28, not {0}")]
    V(u8),
    #[error("the signature does not recover to a key")]
    Unrecoverable,
}

impl Signature {
    /// The 65 bytes, `v` as 27 or 28.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[..32].copy_from_slice(&self.r);
        bytes[32..64].copy_from_slice(&self.s);
        bytes[64] = self.v;
        bytes
    }

    /// From 65 bytes; a `v` of 0 or 1 is read as 27 or 28.
    pub fn from_bytes(bytes: &[u8; 65]) -> Result<Self, BadSignature> {
        let v = match bytes[64] {
            0 | 1 => bytes[64] + 27,
            27 | 28 => bytes[64],
            other => return Err(BadSignature::V(other)),
        };
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..64]);
        Ok(Signature { r, s, v })
    }

    /// `0x` and 130 lowercase hex digits: what every Aster `signature` field carries.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode_prefixed(&self.to_bytes())
    }
}

impl FromStr for Signature {
    type Err = BadSignature;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Signature::from_bytes(&hex::decode_array::<65>(text.trim())?)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", self.to_hex())
    }
}

/// The signer's address: who signed `hash` to make `signature`.
#[cfg(feature = "sign")]
pub fn recover(hash: &[u8; 32], signature: &Signature) -> Result<Address, BadSignature> {
    use k256::ecdsa::{RecoveryId, Signature as Rs, VerifyingKey};
    let rs = Rs::from_scalars(signature.r, signature.s).map_err(|_| BadSignature::Unrecoverable)?;
    let id = RecoveryId::from_byte(signature.v.wrapping_sub(27)).ok_or(BadSignature::V(signature.v))?;
    let key = VerifyingKey::recover_from_prehash(hash, &rs, id).map_err(|_| BadSignature::Unrecoverable)?;
    Ok(address_of(&key))
}

#[cfg(feature = "sign")]
fn address_of(key: &k256::ecdsa::VerifyingKey) -> Address {
    let point = key.to_encoded_point(false);
    let digest = keccak256(&point.as_bytes()[1..]);
    let mut bytes = [0u8; 20];
    bytes.copy_from_slice(&digest[12..]);
    Address::from_bytes(bytes)
}

/// An Ethereum private key: an agent's, or a main wallet's in a program that holds it. Never
/// printed; its memory is wiped when it is dropped.
#[cfg(feature = "sign")]
#[derive(Clone)]
pub struct PrivateKey {
    key: k256::ecdsa::SigningKey,
    address: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadKey {
    #[error(transparent)]
    Hex(#[from] BadHex),
    #[error("not a secp256k1 private key: zero, or past the curve's order")]
    OutOfRange,
    #[error("the system gave no randomness for a new key")]
    NoRandomness,
}

#[cfg(feature = "sign")]
impl PrivateKey {
    /// From its 32 bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, BadKey> {
        let key = k256::ecdsa::SigningKey::from_slice(bytes).map_err(|_| BadKey::OutOfRange)?;
        let address = address_of(key.verifying_key());
        Ok(PrivateKey { key, address })
    }

    /// From hex, with or without `0x`.
    pub fn from_hex(text: &str) -> Result<Self, BadKey> {
        let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
        hex::decode_into(text.trim(), bytes.as_mut())?;
        PrivateKey::from_bytes(&bytes)
    }

    /// A new key from the system's randomness: a fresh agent.
    pub fn random() -> Result<Self, BadKey> {
        let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
        loop {
            getrandom02::getrandom(bytes.as_mut()).map_err(|_| BadKey::NoRandomness)?;
            if let Ok(key) = PrivateKey::from_bytes(&bytes) {
                return Ok(key);
            }
        }
    }

    #[must_use]
    pub const fn address(&self) -> Address {
        self.address
    }

    /// The 32 bytes, for a program that stores the key itself.
    #[must_use]
    pub fn to_bytes(&self) -> zeroize::Zeroizing<[u8; 32]> {
        let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
        bytes.copy_from_slice(&self.key.to_bytes());
        bytes
    }

    /// The key as `0x` hex, for a program that stores it itself.
    #[must_use]
    pub fn to_hex(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(hex::encode_prefixed(self.to_bytes().as_ref()))
    }

    /// Signs a 32-byte hash, deterministically (RFC 6979), `s` in the lower half.
    pub fn sign_hash(&self, hash: &[u8; 32]) -> Result<Signature, BadSignature> {
        let (rs, id) = self.key.sign_prehash_recoverable(hash).map_err(|_| BadSignature::Unrecoverable)?;
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&rs.r().to_bytes());
        s.copy_from_slice(&rs.s().to_bytes());
        Ok(Signature { r, s, v: 27 + id.to_byte() })
    }
}

#[cfg(feature = "sign")]
impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrivateKey({})", self.address.to_checksum())
    }
}

#[cfg(test)]
#[path = "tests/ecdsa.rs"]
mod tests;
