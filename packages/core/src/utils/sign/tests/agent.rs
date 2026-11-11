//! Every signed call matches globe.li byte for byte: the query that is signed, its hash, the
//! signature, and what the request carries.

use super::*;
use crate::utils::sign::keccak256;
use serde_json::Value;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../../../vectors/aster_web.json")).unwrap()
}

/// The keys the vectors were made with, derived from labels: never a real wallet.
fn main_key() -> PrivateKey {
    PrivateKey::from_bytes(&keccak256(b"atlas aster main")).unwrap()
}

fn agent() -> Agent {
    Agent::new(main_key().address(), PrivateKey::from_bytes(&keccak256(b"atlas aster agent")).unwrap())
}

#[test]
fn signed_calls_match_globe_li_byte_for_byte() {
    let v = vectors();
    let agent = agent();
    assert_eq!(agent.user.to_checksum(), v["approve"]["mainAddress"].as_str().unwrap());
    assert_eq!(agent.signer().to_checksum(), v["approve"]["agentAddress"].as_str().unwrap());
    let trades = v["trades"].as_array().unwrap();
    assert_eq!(trades.len(), 8);
    for t in trades {
        let name = t["name"].as_str().unwrap();
        let msg = t["msg"].as_str().unwrap();
        let nonce: u64 = t["nonce"].as_str().unwrap().parse().unwrap();
        // the call's parameters are what precedes the nonce, decoded back
        let business = msg.split("&nonce=").next().unwrap();
        let params: Vec<(&str, String)> = if business.starts_with("nonce=") {
            Vec::new()
        } else {
            business
                .split('&')
                .map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap();
                    (k, if k == "symbol" && v.starts_with('%') { "龙虾USDT".to_owned() } else { v.to_owned() })
                })
                .collect()
        };
        let signed = sign_params(&params, nonce, &agent).unwrap();
        assert_eq!(signed.msg, msg, "{name}");
        assert_eq!(signed.signature.to_string(), t["signature"].as_str().unwrap(), "{name}");
        let req = &t["request"];
        match req["method"].as_str().unwrap() {
            "POST" => assert_eq!(signed.encoded(), req["body"].as_str().unwrap(), "{name}"),
            _ => assert_eq!(req["url"].as_str().unwrap().split_once('?').unwrap().1, signed.encoded(), "{name}"),
        }
        assert_eq!(
            crate::utils::sign::recover(&message(&signed.msg).hash(), &signed.signature).unwrap(),
            agent.signer()
        );
    }
}
