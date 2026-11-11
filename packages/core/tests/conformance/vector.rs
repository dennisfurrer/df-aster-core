//! The vector files and the keys they were made with.

use df_aster_core::Address;
use df_aster_core::utils::sign::{PrivateKey, keccak256};
use serde_json::Value;

pub fn python() -> Value {
    serde_json::from_str(include_str!("../../vectors/aster_python.json")).unwrap()
}

/// keccak256("atlas aster main"): never a real wallet.
pub fn main_key() -> PrivateKey {
    PrivateKey::from_bytes(&keccak256(b"atlas aster main")).unwrap()
}

/// The example agent key Aster's API docs publish.
pub fn docs_agent_key() -> PrivateKey {
    PrivateKey::from_hex("0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1").unwrap()
}

pub fn address(text: &Value) -> Address {
    text.as_str().unwrap().parse().unwrap()
}

pub fn hex(bytes: &[u8; 32]) -> String {
    df_aster_core::utils::hex::encode_prefixed(bytes)
}
