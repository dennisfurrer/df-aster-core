//! The main wallet's actions (approve, update and delete an agent; approve, update and delete a
//! builder): the typed data a wallet is shown, its hash and its signature, as the demo's
//! `sign_v3_eip712` makes them.

use df_aster_core::Dec;
use df_aster_core::account::agents::{ApproveAgent, MainAction};
use serde_json::Value;

use crate::vector::{address, hex, main_key, python};

fn action(primary: &str, params: &Value) -> MainAction {
    let user = address(&params["user"]);
    let nonce = params["nonce"].as_u64().unwrap();
    let text = |key: &str| params[key].as_str().unwrap();
    let flag = |key: &str| params[key].as_bool().unwrap();
    match primary {
        "ApproveAgent" => {
            let approval = ApproveAgent {
                agent_name: text("agentName").to_owned(),
                agent: address(&params["agentAddress"]),
                ip_whitelist: text("ipWhitelist").to_owned(),
                expired_ms: params["expired"].as_u64().unwrap(),
                can_spot_trade: flag("canSpotTrade"),
                can_perp_trade: flag("canPerpTrade"),
                can_withdraw: flag("canWithdraw"),
            };
            MainAction::approve_agent(user, nonce, &approval)
        }
        "UpdateAgent" => MainAction::update_agent(
            user,
            nonce,
            address(&params["agentAddress"]),
            text("ipWhitelist"),
            flag("canSpotTrade"),
            flag("canPerpTrade"),
            flag("canWithdraw"),
        ),
        "DelAgent" => MainAction::delete_agent(user, nonce, address(&params["agentAddress"])),
        "ApproveBuilder" => MainAction::approve_builder(
            user,
            nonce,
            address(&params["builder"]),
            Dec::parse(text("maxFeeRate")).unwrap(),
            text("builderName"),
        ),
        "UpdateBuilder" => MainAction::update_builder(
            user,
            nonce,
            address(&params["builder"]),
            Dec::parse(text("maxFeeRate")).unwrap(),
        ),
        "DelBuilder" => MainAction::delete_builder(user, nonce, address(&params["builder"])),
        other => panic!("no action {other}"),
    }
}

#[test]
fn main_wallet_actions_sign_as_eth_account_does() {
    let vectors = python();
    let cases = vectors["mainActions"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let primary = case["primaryType"].as_str().unwrap();
        let ours = action(primary, &case["params"]);
        let data = ours.typed_data();
        let shown = data.to_json();
        let given = &case["typedData"];
        assert_eq!(shown["types"][primary], given["types"][primary], "{primary}");
        assert_eq!(shown["message"], given["message"], "{primary}");
        assert_eq!(shown["primaryType"], given["primaryType"], "{primary}");
        assert_eq!(hex(&data.hash()), case["signingHash"].as_str().unwrap(), "{primary}");
        let signature = main_key().sign_hash(&data.hash()).unwrap();
        assert_eq!(signature.to_hex(), case["signature"].as_str().unwrap(), "{primary}");
        // The request carries every signed field under its own key, then the signature.
        let call = ours.call(&signature);
        assert_eq!(call.value("signature"), Some(signature.to_hex().as_str()), "{primary}");
        assert_eq!(call.value("signatureChainId"), Some("56"), "{primary}");
        assert_eq!(call.value("asterChain"), Some("Mainnet"), "{primary}");
    }
}
