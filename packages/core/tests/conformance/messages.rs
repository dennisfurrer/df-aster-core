//! An agent's `Message { msg }` under chain 1666: the hash and the signature, as eth_account
//! makes them with the docs' example agent key.

use df_aster_core::utils::sign::message;

use crate::vector::{docs_agent_key, hex, python};

#[test]
fn agent_messages_sign_as_eth_account_does() {
    let vectors = python();
    let key = docs_agent_key();
    assert_eq!(key.address().to_checksum(), vectors["agentAddress"].as_str().unwrap());
    let cases = vectors["messages"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let data = message(case["msg"].as_str().unwrap());
        assert_eq!(hex(&data.hash()), case["signingHash"].as_str().unwrap(), "{name}");
        let signature = key.sign_hash(&data.hash()).unwrap();
        assert_eq!(signature.to_hex(), case["signature"].as_str().unwrap(), "{name}");
    }
}
