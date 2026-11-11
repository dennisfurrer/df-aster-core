//! `registerAndApproveAgent`: the string the main wallet signs and its signature, as the demo
//! signs it (chain 1666, no empty `ipWhitelist`) and as the docs describe it (chain 56).

use df_aster_core::account::agents::{ApproveAgent, RegisterAgent};

use crate::vector::{address, hex, main_key, python};

#[test]
fn registration_signs_as_eth_account_does() {
    let vectors = python();
    let user = address(&vectors["mainAddress"]);
    let agent = address(&vectors["agentAddress"]);
    for case in vectors["registerAndApproveAgent"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut approval = ApproveAgent::perps_only("bot", agent, 1_792_592_000_000);
        approval.ip_whitelist = case["ipWhitelist"].as_str().unwrap().to_owned();
        let mut register = RegisterAgent::new(user, 1_790_000_000_000_000, approval, None);
        register.sign_chain_id = case["chainId"].as_u64().unwrap();
        assert_eq!(register.msg(), case["msg"].as_str().unwrap(), "{name}");
        let hash = register.typed_data().hash();
        assert_eq!(hex(&hash), case["signingHash"].as_str().unwrap(), "{name}");
        assert_eq!(main_key().sign_hash(&hash).unwrap().to_hex(), case["signature"].as_str().unwrap(), "{name}");
    }
}
