//! The listen key's calls, signed by the agent, and when its keepalive is due.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

#[test]
fn listen_key_calls_are_signed_on_their_methods() {
    for (call, method) in
        [(start_call(), Method::Post), (keep_alive_call(), Method::Put), (close_call(), Method::Delete)]
    {
        assert_eq!((call.path, call.method, call.signed, call.weight), ("/fapi/v3/listenKey", method, true, 1));
        assert!(call.fields.is_empty());
    }
    let key = "pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1";
    let transport = Recorded::answering(&format!(r#"{{"listenKey":"{key}"}}"#));
    let started = block_on(start(&transport)).unwrap();
    assert_eq!(started.listen_key, key);
    assert_eq!(transport.only(), start_call());
    assert_eq!(transport.sent().len(), 1);
    assert!(!format!("{started:?}").contains(key), "the key is not printed whole");
    assert!(block_on(keep_alive(&Recorded::answering("{}"))).is_ok());
    assert!(block_on(close(&Recorded::answering("{}"))).is_ok());
    let refused =
        block_on(keep_alive(&Recorded::answering(r#"{"code":-1125,"msg":"This listenKey does not exist."}"#)));
    assert_eq!(refused.unwrap_err().code(), Some(crate::client::codes::INVALID_LISTEN_KEY));
}

#[test]
fn a_keepalive_is_due_every_half_hour() {
    let mut timer = KeepAlive::new();
    assert!(!timer.due(u64::MAX / 2), "nothing before the key started");
    timer.kept(1_000);
    assert_eq!(timer.due_at(), Some(1_000 + KEEP_ALIVE_MS));
    assert!(!timer.due(1_000 + KEEP_ALIVE_MS - 1));
    assert!(timer.due(1_000 + KEEP_ALIVE_MS));
    assert!(!timer.lapsed(1_000 + KEEP_ALIVE_MS));
    assert!(timer.lapsed(1_000 + LISTEN_KEY_LIFETIME_MS));
    assert_eq!(KeepAlive::every(3 * LISTEN_KEY_LIFETIME_MS).due_at(), None);
    let mut capped = KeepAlive::every(3 * LISTEN_KEY_LIFETIME_MS);
    capped.kept(0);
    assert_eq!(capped.due_at(), Some(50 * MINUTE_MS));
}
