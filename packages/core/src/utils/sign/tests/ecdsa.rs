//! Keys to addresses, deterministic signatures, recovery, and the 65-byte form both ways.

use super::*;
#[cfg(feature = "sign")]
use crate::utils::sign::keccak256;

#[test]
fn signature_reads_and_writes_65_bytes() {
    let text = format!("0x{}{}1b", "11".repeat(32), "22".repeat(32));
    let signature: Signature = text.parse().unwrap();
    assert_eq!((signature.r[0], signature.s[31], signature.v), (0x11, 0x22, 27));
    assert_eq!(signature.to_string(), text);
    let zero_v: Signature = format!("0x{}{}01", "11".repeat(32), "22".repeat(32)).parse().unwrap();
    assert_eq!(zero_v.v, 28);
    assert!(format!("0x{}{}05", "11".repeat(32), "22".repeat(32)).parse::<Signature>().is_err());
    assert!("0x1234".parse::<Signature>().is_err());
}

#[cfg(feature = "sign")]
#[test]
fn keys_give_their_addresses() {
    let one = PrivateKey::from_hex(&format!("0x{}01", "00".repeat(31))).unwrap();
    assert_eq!(one.address().to_checksum(), "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf");
    // The agent in Aster's own API docs.
    let docs = PrivateKey::from_hex("0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1").unwrap();
    assert_eq!(docs.address().to_checksum(), "0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0");
    assert!(PrivateKey::from_bytes(&[0u8; 32]).is_err());
    assert!(PrivateKey::from_bytes(&[0xffu8; 32]).is_err());
    assert!(!format!("{docs:?}").contains("4fd0"));
    assert_eq!(PrivateKey::from_hex(&docs.to_hex()).unwrap().address(), docs.address());
}

#[cfg(feature = "sign")]
#[test]
fn signs_deterministically_and_recovers() {
    let key = PrivateKey::from_bytes(&keccak256(b"df-aster-core test")).unwrap();
    let hash = keccak256(b"a message");
    let first = key.sign_hash(&hash).unwrap();
    assert_eq!(first, key.sign_hash(&hash).unwrap());
    assert!(first.v == 27 || first.v == 28);
    // low s
    assert!(first.s[0] < 0x80);
    assert_eq!(recover(&hash, &first).unwrap(), key.address());
    let other = keccak256(b"another message");
    assert_ne!(recover(&other, &first).unwrap(), key.address());
    let fresh = PrivateKey::random().unwrap();
    assert_ne!(fresh.address(), key.address());
}
