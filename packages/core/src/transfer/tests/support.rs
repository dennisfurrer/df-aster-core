//! What the main wallet signs to authorise a call, held to eth_account; the fee figures read as
//! written; an empty answer read as none.

use super::*;
use crate::test_support::{Recorded, block_on};

/// The main wallet's signature of a transfer to an address at nonce 1790000000000000, made by
/// eth_account 0.13.7 (`encode_typed_data` with `Message { msg }` under
/// `{AsterSignTransaction, "1", 1666, 0x0}`, then `Account.sign_message`) with the key
/// keccak256("atlas aster main").
#[cfg(feature = "sign")]
const ETH_ACCOUNT_AUTH: &str = "0xd4ac526e14e87f7a7a9c648408971262067ddc9938b4cba26c9881a52938e43c39d7075e0dc5e307a078ec41ac4b814212e301ef3ab5aa2c0297726d59cd77d21c";

#[cfg(feature = "sign")]
#[test]
fn the_main_wallet_signs_the_fields_then_nonce_and_user_as_eth_account_does() {
    use crate::test_support::{agent, main_key};
    use crate::utils::sign::LocalWallet;
    let transfer = ChainTransfer {
        asset: "USDT".into(),
        amount: Dec::from_int(10),
        to: agent().signer(),
        client_tran_id: Some("t1".into()),
    };
    let signed = block_on(sign_with_wallet(transfer.call(), 1_790_000_000_000_000, &LocalWallet(main_key()))).unwrap();
    let text: Vec<String> = signed.fields.iter().map(|(key, value)| format!("{key}={value}")).collect();
    assert_eq!(
        text.join("&"),
        format!(
            "asset=USDT&amount=10&toAddress=0xe0a5193404A0f6d94c087169c539eB0180FCf828&clientTranId=t1\
             &nonce=1790000000000000&user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&signature={ETH_ACCOUNT_AUTH}"
        )
    );
    assert!(!signed.signed, "an agent adds nothing to it");
    assert_eq!(signed.host, crate::client::Host::Chain);
}

#[test]
fn directions_are_spelled_as_the_venue_spells_them() {
    let all = [Direction::FutureSpot, Direction::SpotFuture, Direction::FutureFuture, Direction::SpotSpot];
    assert_eq!(all.map(Direction::as_str), ["FUTURE_SPOT", "SPOT_FUTURE", "FUTURE_FUTURE", "SPOT_SPOT"]);
}

#[test]
fn figures_read_as_written_whatever_their_json_type() {
    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(default, deserialize_with = "figure")]
        a: Option<Dec>,
    }
    let read = |json: &str| serde_json::from_str::<Row>(json).unwrap().a;
    assert_eq!(read(r#"{"a":0.99900000}"#), Some(Dec::parse("0.999").unwrap()));
    assert_eq!(read(r#"{"a":"1.50"}"#), Some(Dec::parse("1.5").unwrap()));
    assert_eq!(read(r#"{"a":200000}"#), Some(Dec::from_int(200_000)));
    assert_eq!((read(r#"{"a":null}"#), read("{}")), (None, None));
}

#[test]
fn an_empty_answer_reads_as_none_and_a_refusal_as_itself() {
    let empty = Recorded::answering("");
    assert_eq!(block_on(read_maybe::<serde_json::Value>(&empty, Call::post("/x"))).unwrap(), None);
    let refused = Recorded::failing(TransportError::Status {
        status: 400,
        body: r#"{"code":-4212,"msg":"User can not asset exchange while not in joint margin mode"}"#.into(),
    });
    let error = block_on(read_maybe::<serde_json::Value>(&refused, Call::post("/x"))).unwrap_err();
    assert_eq!(error.code(), Some(crate::client::codes::NOT_JOINT_MARGIN));
}
