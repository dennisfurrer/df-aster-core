//! EIP-712 typed data for the few structs Aster signs: the agent's `Message { string msg }`, the
//! main wallet's `ApproveAgent`, and a withdrawal's `Action`. Each is a flat struct of `string`,
//! `uint256`, `bool` and `address` fields under the four-field domain, so the encoder covers
//! exactly that and refuses nothing else by construction.
//!
//! [`TypedData::hash`] is what a wallet signs (`0x1901, domainSeparator, hashStruct`), and
//! [`TypedData::to_json`] what it is shown (`eth_signTypedData_v4`).

use serde_json::{Map, Value, json};

use crate::utils::Address;
use crate::utils::sign::keccak::keccak256;

/// The domain every Aster struct is signed under: name, version, chain id and a verifying
/// contract, which is always the zero address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub name: String,
    pub version: String,
    pub chain_id: u64,
    pub verifying_contract: Address,
}

impl Domain {
    #[must_use]
    pub fn new(name: &str, version: &str, chain_id: u64) -> Self {
        Domain { name: name.to_owned(), version: version.to_owned(), chain_id, verifying_contract: Address::ZERO }
    }

    /// `keccak256(typeHash, keccak(name), keccak(version), chainId, verifyingContract)`.
    #[must_use]
    pub fn separator(&self) -> [u8; 32] {
        let mut data = Vec::with_capacity(32 * 5);
        data.extend_from_slice(&keccak256(
            b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
        ));
        data.extend_from_slice(&keccak256(self.name.as_bytes()));
        data.extend_from_slice(&keccak256(self.version.as_bytes()));
        data.extend_from_slice(&uint(u128::from(self.chain_id)));
        data.extend_from_slice(&address_word(self.verifying_contract));
        keccak256(&data)
    }
}

/// One field's value, by its Solidity type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    String(String),
    Uint256(u128),
    Bool(bool),
    Address(Address),
}

impl Field {
    #[must_use]
    pub const fn solidity_type(&self) -> &'static str {
        match self {
            Field::String(_) => "string",
            Field::Uint256(_) => "uint256",
            Field::Bool(_) => "bool",
            Field::Address(_) => "address",
        }
    }

    /// The field's 32-byte word in `hashStruct`.
    fn word(&self) -> [u8; 32] {
        match self {
            Field::String(text) => keccak256(text.as_bytes()),
            Field::Uint256(value) => uint(*value),
            Field::Bool(value) => uint(u128::from(*value)),
            Field::Address(address) => address_word(*address),
        }
    }

    /// The value as a wallet is shown it: a number as a JSON number while it fits in 2^53, else
    /// as its decimal text; an address in EIP-55.
    fn json(&self) -> Value {
        match self {
            Field::String(text) => json!(text),
            Field::Uint256(value) => match u64::try_from(*value) {
                Ok(small) if small < (1 << 53) => json!(small),
                _ => json!(value.to_string()),
            },
            Field::Bool(value) => json!(value),
            Field::Address(address) => json!(address.to_checksum()),
        }
    }
}

/// A struct of named fields, in the order its type lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Struct {
    pub name: String,
    pub fields: Vec<(String, Field)>,
}

impl Struct {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Struct { name: name.to_owned(), fields: Vec::new() }
    }

    #[must_use]
    pub fn field(mut self, name: &str, value: Field) -> Self {
        self.fields.push((name.to_owned(), value));
        self
    }

    /// `Name(type name,type name,...)`. A field name may hold a space (`destination Chain`); it
    /// is written as it is.
    #[must_use]
    pub fn encode_type(&self) -> String {
        let fields: Vec<String> =
            self.fields.iter().map(|(name, value)| format!("{} {name}", value.solidity_type())).collect();
        format!("{}({})", self.name, fields.join(","))
    }

    #[must_use]
    pub fn type_hash(&self) -> [u8; 32] {
        keccak256(self.encode_type().as_bytes())
    }

    /// `keccak256(typeHash, word, word ...)`.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        let mut data = Vec::with_capacity(32 * (1 + self.fields.len()));
        data.extend_from_slice(&self.type_hash());
        for (_, value) in &self.fields {
            data.extend_from_slice(&value.word());
        }
        keccak256(&data)
    }
}

/// A struct under its domain: what a wallet signs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedData {
    pub domain: Domain,
    pub message: Struct,
}

impl TypedData {
    #[must_use]
    pub const fn new(domain: Domain, message: Struct) -> Self {
        TypedData { domain, message }
    }

    /// `keccak256(0x1901, domainSeparator, hashStruct(message))`: the hash that is signed.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        let mut data = Vec::with_capacity(66);
        data.extend_from_slice(&[0x19, 0x01]);
        data.extend_from_slice(&self.domain.separator());
        data.extend_from_slice(&self.message.hash());
        keccak256(&data)
    }

    /// The `eth_signTypedData_v4` JSON a wallet is handed: its hash is [`TypedData::hash`].
    #[must_use]
    pub fn to_json(&self) -> Value {
        let fields: Vec<Value> = self
            .message
            .fields
            .iter()
            .map(|(name, value)| json!({ "name": name, "type": value.solidity_type() }))
            .collect();
        let message: Map<String, Value> =
            self.message.fields.iter().map(|(name, value)| (name.clone(), value.json())).collect();
        let mut types = Map::new();
        types.insert(
            "EIP712Domain".to_owned(),
            json!([
                { "name": "name", "type": "string" },
                { "name": "version", "type": "string" },
                { "name": "chainId", "type": "uint256" },
                { "name": "verifyingContract", "type": "address" },
            ]),
        );
        types.insert(self.message.name.clone(), Value::Array(fields));
        json!({
            "types": types,
            "primaryType": self.message.name,
            "domain": {
                "name": self.domain.name,
                "version": self.domain.version,
                "chainId": self.domain.chain_id,
                "verifyingContract": self.domain.verifying_contract.to_hex(),
            },
            "message": message,
        })
    }
}

/// A `uint256` word, big-endian.
fn uint(value: u128) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&value.to_be_bytes());
    word
}

fn address_word(address: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(address.bytes());
    word
}

#[cfg(test)]
#[path = "tests/eip712.rs"]
mod tests;
