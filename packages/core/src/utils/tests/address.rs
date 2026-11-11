//! Addresses read in any case, written lowercase or in EIP-55, and nothing else read as one.

use super::*;

#[test]
fn any_case_reads_to_the_same_address_and_writes_back_lowercase() {
    let lower = "0xbaff87940cb148c5e2529c0a5944b0a3386fb015";
    let address: Address = lower.parse().unwrap();
    let json = serde_json::to_string(&address).unwrap();
    assert_eq!(json, format!("\"{lower}\""));
    assert_eq!(serde_json::from_str::<Address>(&json).unwrap(), address);
    let upper: Address = serde_json::from_str("\"0xBAFF87940CB148C5E2529C0A5944B0A3386FB015\"").unwrap();
    assert_eq!((upper, upper.to_string()), (address, lower.to_owned()));
    let mixed: Address = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831".parse().unwrap();
    assert_eq!(mixed.to_hex(), "0xaf88d065e77c8cc2239327c5edb3a432268e5831");
    assert_eq!(" 0X14791697260E4c9A71f18484C9f997B308e59325 ".parse::<Address>().unwrap().bytes()[0], 0x14);
}

#[test]
fn anything_else_is_refused_rather_than_zeroed() {
    for bad in [
        "0xdeadbeef",
        "baff87940cb148c5e2529c0a5944b0a3386fb015",
        "0xzzff87940cb148c5e2529c0a5944b0a3386fb015",
        "0xbaff87940cb148c5e2529c0a5944b0a3386fb0155",
        "",
        "0x",
    ] {
        assert!(bad.parse::<Address>().is_err(), "{bad} should be refused");
    }
    for said in ["\"0x00\"", "\"nonsense\"", "\"\"", "42"] {
        assert!(serde_json::from_str::<Address>(said).is_err(), "{said}");
    }
}

#[test]
fn checksum_is_eip55() {
    // The EIP-55 test vectors.
    for spelled in [
        "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
        "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
        "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
        "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        "0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e",
    ] {
        assert_eq!(spelled.parse::<Address>().unwrap().to_checksum(), spelled);
    }
}
