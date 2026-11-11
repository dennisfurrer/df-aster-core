//! Calls signed at a chosen nonce: an order whose nonce is kept, guarded cancels at the order's
//! nonce, and a noop; each carries exactly what the agent signed. Nothing leaves the process.

use super::*;
use crate::client::Method;
use crate::test_support::{Wire, agent, block_on, ok, signing_client, status};
use crate::trade::orders::NewOrder;
use crate::utils::sign::{message, recover};
use crate::utils::{Dec, Side};

const PLACED: &str = include_str!("../../../fixtures/aster/trade_order.json");
const CANCELLED: &str = include_str!("../../../fixtures/aster/trade_cancel.json");
const CANCELLED_BATCH: &str = include_str!("../../../fixtures/aster/trade_cancel_batch.json");
const NOOP: &str = include_str!("../../../fixtures/aster/trade_noop.json");

/// The query or body a request carried, and who signed it.
fn signed_by(text: &str) -> (String, crate::utils::Address) {
    let (msg, signature) = text.rsplit_once("&signature=").unwrap();
    (msg.to_owned(), recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap())
}

#[test]
fn a_tracked_order_gives_back_its_nonce_even_when_refused() {
    let client =
        signing_client(Wire::answering([ok(PLACED), status(400, r#"{"code":-2019,"msg":"Margin is insufficient."}"#)]));
    let order = NewOrder::market("BTCUSDT", Side::Buy, Dec::parse("0.01").unwrap());
    let first = block_on(place_tracked(&client, &order)).unwrap();
    assert_eq!(first.nonce, 1_790_000_000_000_000);
    assert_eq!(first.answer.unwrap().order_id, 22_542_179);
    let second = block_on(place_tracked(&client, &order)).unwrap();
    assert_eq!((second.nonce, second.answer.unwrap_err().code()), (1_790_000_000_000_001, Some(-2019)));
    let bodies = client.http().bodies();
    let (msg, signer) = signed_by(&bodies[0].1);
    assert_eq!(signer, agent().signer());
    assert!(msg.starts_with("symbol=BTCUSDT&side=BUY&type=MARKET&quantity=0.01&nonce=1790000000000000&user="), "{msg}");
    assert!(block_on(place_tracked(&client, &NewOrder::market("X", Side::Buy, Dec::ZERO))).is_err());
}

#[test]
fn a_guarded_cancel_is_signed_at_the_orders_nonce() {
    let client = signing_client(Wire::answering([ok(CANCELLED), ok(CANCELLED_BATCH)]));
    let placed_at = 1_789_999_990_000_123;
    let order = block_on(guarded_cancel(&client, "BTCUSDT", &OrderRef::Id(283_194_212), placed_at)).unwrap();
    assert_eq!(order.order_id, 283_194_212);
    let sent = client.http().sent();
    assert_eq!(sent[0].method, Method::Delete);
    let (path, query) = sent[0].url.split_once('?').unwrap();
    assert_eq!(path, "https://fapi.asterdex.com/fapi/v3/guardedCancelOrder");
    let (msg, signer) = signed_by(query);
    assert!(msg.starts_with("symbol=BTCUSDT&orderId=283194212&nonce=1789999990000123&user="), "{msg}");
    assert_eq!(signer, agent().signer());

    let rows =
        block_on(guarded_cancel_batch(&client, "BTCUSDT", &[OrderRef::Id(283_194_212), OrderRef::Id(5)], placed_at))
            .unwrap();
    assert_eq!(rows[1].as_ref().unwrap_err().code(), Some(-2011));
    let url = &client.http().sent()[1].url;
    assert!(
        url.contains(
            "/fapi/v3/guardedBatchOrders?symbol=BTCUSDT&orderIdList=%5B283194212%2C5%5D&nonce=1789999990000123&"
        ),
        "{url}"
    );
    assert_eq!(client.next_nonce(), 1_790_000_000_000_000, "a guarded cancel takes no new nonce");
}

#[test]
fn a_noop_takes_the_nonce_it_is_given() {
    let client = signing_client(Wire::answering([ok(NOOP)]));
    block_on(noop_at(&client, 1_790_000_000_000_777)).unwrap();
    let bodies = client.http().bodies();
    assert_eq!(bodies[0].0, "https://fapi.asterdex.com/fapi/v3/noop");
    let (msg, signer) = signed_by(&bodies[0].1);
    assert_eq!(
        msg,
        format!("nonce=1790000000000777&user={}&signer={}", agent().user.to_checksum(), agent().signer().to_checksum())
    );
    assert_eq!(signer, agent().signer());
}

#[test]
fn signed_at_writes_exactly_what_was_signed() {
    let call = Call::delete("/fapi/v3/order").field("symbol", "龙虾USDT").field("orderId", 1);
    let at = signed_at(call.clone(), 42, &agent()).unwrap();
    assert!(!at.signed);
    let signed = sign_params(&call.fields, 42, &agent()).unwrap();
    assert_eq!(crate::utils::query(&at.fields), signed.encoded());
}
