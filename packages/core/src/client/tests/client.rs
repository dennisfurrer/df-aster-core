//! A client signs what is signed, carries the signature where Aster reads it, keeps nonces
//! moving, signs an expired nonce again once, and is held by the gate.

use super::*;
use crate::test_support::{Wire, agent, block_on, ok, signing_client, status};
use crate::utils::sign::{message, recover};

fn query_of(url: &str) -> &str {
    url.split_once('?').map_or("", |(_, query)| query)
}

#[test]
fn a_signed_get_carries_its_signature_in_the_query() {
    let client = signing_client(Wire::always("[]"));
    let call = Call::get("/fapi/v3/userTrades").field("symbol", "ETHUSDT").field("limit", 100).signed();
    let _: serde_json::Value = block_on(read(&client, call)).unwrap();
    let sent = client.http().sent();
    let url = &sent[0].url;
    assert!(url.starts_with("https://fapi.asterdex.com/fapi/v3/userTrades?symbol=ETHUSDT&limit=100&nonce=1790000000000000&user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&signer=0xe0a5193404A0f6d94c087169c539eB0180FCf828&signature=0x"), "{url}");
    let (msg, signature) = query_of(url).rsplit_once("&signature=").unwrap();
    let signer = recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap();
    assert_eq!(signer, agent().signer());
    assert!(sent[0].body.is_empty());
}

#[test]
fn a_signed_post_carries_it_in_a_form_body_and_nonces_move_on() {
    let client = signing_client(Wire::always(r#"{"orderId":1}"#));
    for _ in 0..2 {
        let call = Call::post("/fapi/v3/order").field("symbol", "BTCUSDT").signed();
        let _: serde_json::Value = block_on(read(&client, call)).unwrap();
    }
    let bodies = client.http().bodies();
    assert_eq!(bodies[0].0, "https://fapi.asterdex.com/fapi/v3/order");
    assert!(bodies[0].1.starts_with("symbol=BTCUSDT&nonce=1790000000000000&"), "{}", bodies[0].1);
    assert!(bodies[1].1.starts_with("symbol=BTCUSDT&nonce=1790000000000001&"), "{}", bodies[1].1);
    assert_eq!(client.http().sent()[0].header("content-type"), Some("application/x-www-form-urlencoded"));
}

#[test]
fn a_nonce_window_goes_on_what_changes_the_account_only() {
    let client = signing_client(Wire::always("{}")).with_nonce_window(5_000);
    let _: serde_json::Value =
        block_on(read(&client, Call::post("/fapi/v3/order").field("symbol", "X").signed())).unwrap();
    let _: serde_json::Value = block_on(read(&client, Call::get("/fapi/v3/balance").signed())).unwrap();
    let bodies = client.http().bodies();
    assert!(bodies[0].1.starts_with("symbol=X&nonceWindow=5000&nonce="), "{}", bodies[0].1);
    assert!(!bodies[1].0.contains("nonceWindow"));
}

#[test]
fn an_expired_nonce_is_signed_again_once() {
    let expired = r#"{"code":-4225,"msg":"Nonce Expired"}"#;
    let client = signing_client(Wire::answering([status(400, expired), ok(r#"{"orderId":2}"#)]));
    let order: serde_json::Value = block_on(read(&client, Call::post("/fapi/v3/order").signed())).unwrap();
    assert_eq!(order["orderId"], 2);
    let bodies = client.http().bodies();
    assert_eq!(bodies.len(), 2);
    assert_ne!(bodies[0].1, bodies[1].1);
    let twice = signing_client(Wire::answering([status(400, expired), status(400, expired), ok("{}")]));
    let error = block_on(read::<serde_json::Value>(&twice, Call::post("/fapi/v3/order").signed())).unwrap_err();
    assert!(error.is_nonce());
    assert_eq!(twice.http().sent().len(), 2);
}

#[test]
fn a_reader_refuses_a_signed_call_before_it_goes_out() {
    let client = Client::reader(Wire::always("{}"), crate::test_support::clock);
    let error = block_on(read::<serde_json::Value>(&client, Call::get("/fapi/v3/balance").signed())).unwrap_err();
    assert!(matches!(error, ApiError::Transport(TransportError::NoAgent(_))));
    assert!(client.http().sent().is_empty());
}

#[test]
fn a_429_closes_the_gate_for_everything() {
    let client = Client::reader(Wire::answering([status(429, "")]).then(ok("{}")), crate::test_support::clock);
    let first = block_on(read::<serde_json::Value>(&client, Call::get("/fapi/v3/ping"))).unwrap_err();
    assert!(first.is_rate_limited());
    let second = block_on(read::<serde_json::Value>(&client, Call::get("/fapi/v3/ping"))).unwrap_err();
    assert!(second.is_rate_limited());
    assert_eq!(client.http().sent().len(), 1);
    assert!(client.spent().cooldown_ms > 0);
}

#[test]
fn the_clock_follows_the_venue_once_synced() {
    let client = signing_client(Wire::always(r#"{"serverTime":1790000003000}"#));
    assert_eq!(block_on(client.sync_clock()).unwrap(), 3_000);
    assert_eq!(client.now_ms(), 1_790_000_003_000);
    assert_eq!(client.next_nonce(), 1_790_000_003_000_000);
    let hosts = Hosts::all_at("http://127.0.0.1:1/");
    let moved = signing_client(Wire::always("{}")).with_hosts(hosts);
    let _: serde_json::Value = block_on(read(&moved, Call::get("/fapi/v3/ping"))).unwrap();
    assert_eq!(moved.http().sent()[0].url, "http://127.0.0.1:1/fapi/v3/ping");
}
