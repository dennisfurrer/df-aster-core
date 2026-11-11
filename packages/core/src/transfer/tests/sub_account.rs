//! A sub-account transfer signed in the docs' order, `fromAccountAddress` after `signer`, and
//! sent with the signature last.

use super::*;
use crate::test_support::{Wire, agent, block_on, signing_client};
use crate::utils::sign::{message, recover};

fn address(text: &str) -> Address {
    text.parse().unwrap()
}

fn transfer(from: Option<Address>) -> SubAccountTransfer {
    SubAccountTransfer {
        to: address("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"),
        asset: "USDT".into(),
        amount: Dec::parse("10.0").unwrap(),
        direction: Direction::FutureFuture,
        from,
    }
}

#[test]
fn the_message_follows_the_docs_order() {
    let agent = agent();
    let sub = address("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359");
    assert_eq!(
        sub_account_message(&transfer(Some(sub)), 7, agent.user, agent.signer()),
        "toAccountAddress=0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed&asset=USDT&amount=10&kindType=FUTURE_FUTURE\
         &nonce=7&user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&signer=0xe0a5193404A0f6d94c087169c539eB0180FCf828\
         &fromAccountAddress=0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359"
    );
    assert!(!sub_account_message(&transfer(None), 7, agent.user, agent.signer()).contains("fromAccountAddress"));
}

#[test]
fn the_agent_signs_it_and_it_goes_out_once_with_the_signature_last() {
    let client = signing_client(Wire::always(r#"{"code":200,"msg":"success"}"#));
    let sub = address("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359");
    block_on(sub_account_transfer(&client, &transfer(Some(sub)))).unwrap();
    let bodies = client.http().bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0].0, "https://fapi.asterdex.com/fapi/v3/subAccountTransfer");
    let (msg, signature) = bodies[0].1.rsplit_once("&signature=").unwrap();
    assert_eq!(msg, sub_account_message(&transfer(Some(sub)), 1_790_000_000_000_000, agent().user, agent().signer()));
    assert_eq!(recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap(), agent().signer());
}

#[test]
fn nothing_is_signed_for_an_amount_that_is_not_positive() {
    let mut zero = transfer(None);
    zero.amount = Dec::ZERO;
    assert!(matches!(sub_account_transfer_call(&zero, 1, &agent()), Err(TransferError::Invalid(_))));
}
