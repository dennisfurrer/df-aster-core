use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Twenty bytes. Read in either case and written lowercase; what Aster signs and sends spells it
/// in its EIP-55 mixed case ([`Address::to_checksum`]), as its web app and wallets do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Address([u8; 20]);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a 20-byte hex address")]
pub struct BadAddress(String);

impl Address {
    pub const ZERO: Address = Address([0u8; 20]);

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 20]) -> Self {
        Address(bytes)
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; 20] {
        &self.0
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Address::ZERO
    }

    /// For constants: `0x` and forty hex digits in either case, or `None`.
    #[must_use]
    pub const fn parse_const(text: &str) -> Option<Address> {
        let raw = text.as_bytes();
        if raw.len() != 42 || raw[0] != b'0' || (raw[1] != b'x' && raw[1] != b'X') {
            return None;
        }
        let mut bytes = [0u8; 20];
        let mut at = 0;
        while at < 20 {
            let (Some(high), Some(low)) = (hex_digit(raw[2 + at * 2]), hex_digit(raw[3 + at * 2])) else {
                return None;
            };
            bytes[at] = (high << 4) | low;
            at += 1;
        }
        Some(Address(bytes))
    }

    /// The lowercase `0x` form.
    #[must_use]
    pub fn to_hex(self) -> String {
        crate::utils::hex::encode_prefixed(&self.0)
    }

    /// The EIP-55 form: a hex letter is upper case where the keccak of the lowercase hex has a
    /// nibble of 8 or more. Every `user`, `signer` and agent address Aster signs is written so.
    #[must_use]
    pub fn to_checksum(self) -> String {
        let lower = crate::utils::hex::encode(&self.0);
        let hash = crate::utils::sign::keccak256(lower.as_bytes());
        let mut out = String::with_capacity(42);
        out.push_str("0x");
        for (at, digit) in lower.chars().enumerate() {
            let nibble = (hash[at / 2] >> if at % 2 == 0 { 4 } else { 0 }) & 0x0f;
            out.push(if nibble >= 8 { digit.to_ascii_uppercase() } else { digit });
        }
        out
    }
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl FromStr for Address {
    type Err = BadAddress;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let raw = text.trim();
        Address::parse_const(raw).ok_or_else(|| BadAddress(raw.to_owned()))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Address {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
#[path = "tests/address.rs"]
mod tests;
