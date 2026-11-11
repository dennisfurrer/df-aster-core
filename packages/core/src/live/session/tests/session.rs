//! A session keeps what is wanted, subscribes it on each socket in one frame, holds frames to
//! the rate, matches answers to what it asked, swaps a listen key, and tells a diff book's broken
//! chain.

use super::*;
use crate::live::fixtures::DIFFS;
use crate::live::{Levels, Speed, list_subscriptions, subscribe, unsubscribe};

fn names(subscriptions: &[Subscription]) -> Vec<String> {
    subscriptions.iter().map(Subscription::name).collect()
}

fn trades() -> Subscription {
    Subscription::agg_trade("BTCUSDT")
}

fn depth() -> Subscription {
    Subscription::depth("ETHUSDT", Speed::Ms100)
}

#[test]
fn what_is_wanted_goes_out_in_one_frame_on_each_open() {
    let mut session = Session::new();
    assert_eq!(session.url(), "wss://fstream.asterdex.com/stream");
    session.subscribe(trades()).unwrap();
    session.subscribe(depth()).unwrap();
    session.subscribe(trades()).unwrap();
    assert_eq!(session.wanted().len(), 2);
    assert!(session.owed(0).is_empty(), "nothing goes out while closed");
    let wanted = names(&[trades(), depth()]);
    assert_eq!(session.opened(1_000), vec![subscribe(1, &wanted)]);
    assert_eq!(session.waiting().get(&1), Some(&Asked::Subscribe(wanted.clone())));
    let answer = session.received(1_010, r#"{"result":null,"id":1}"#).unwrap();
    assert_eq!(
        answer,
        Frame::Answer { id: 1, result: serde_json::Value::Null, asked: Some(Asked::Subscribe(wanted.clone())) }
    );
    assert!(session.waiting().is_empty());
    session.closed(2_000, 0);
    assert_eq!(session.opened(3_000), vec![subscribe(2, &wanted)], "the next socket subscribes again, a new id");
}

#[test]
fn changes_on_an_open_socket_go_out_together_unsubscribes_first() {
    let mut session = Session::new();
    session.subscribe(trades()).unwrap();
    session.opened(0);
    let book = Subscription::book_ticker("BTCUSDT");
    session.subscribe(book.clone()).unwrap();
    session.subscribe(depth()).unwrap();
    assert!(session.unsubscribe(&trades()));
    assert!(!session.unsubscribe(&trades()));
    assert_eq!(
        session.owed(10),
        vec![unsubscribe(2, &names(&[trades()])), subscribe(3, &names(&[book.clone(), depth()]))]
    );
    // Wanted again before its unsubscribe went out: nothing to send.
    session.subscribe(Subscription::mini_ticker("BTCUSDT")).unwrap();
    assert!(session.unsubscribe(&Subscription::mini_ticker("BTCUSDT")));
    session.unsubscribe(&book);
    session.subscribe(book).unwrap();
    assert!(session.owed(20).is_empty());
}

#[test]
fn frames_are_held_to_five_a_second() {
    let mut session = Session::new();
    session.opened(0);
    let mut sent = 0;
    for symbol in ["a", "b", "c", "d", "e", "f", "g"] {
        session.subscribe(Subscription::agg_trade(symbol)).unwrap();
        sent += session.owed(100).len();
    }
    assert_eq!(sent, 5);
    assert_eq!(session.tick(500), Tick::Wait, "no room until a second after the first");
    assert_eq!(session.due(), Some(1_100));
    assert_eq!(session.tick(1_100), Tick::Send);
    let rest = session.owed(1_100);
    assert_eq!(rest, vec![subscribe(6, &["f@aggTrade".to_owned(), "g@aggTrade".to_owned()])]);
}

#[test]
fn past_the_streams_limit_a_subscription_is_refused_and_not_kept() {
    let mut session = Session::new().with_limits(Limits { streams: 2, messages_per_second: 5 });
    session.subscribe(trades()).unwrap();
    session.subscribe(depth()).unwrap();
    assert_eq!(session.subscribe(Subscription::AllTickers), Err(Refused::Streams { limit: 2 }));
    assert_eq!(session.wanted().len(), 2);
    let bad = Subscription::Kline { symbol: "btcusdt".into(), interval: crate::utils::Interval::parse("2w") };
    assert!(matches!(session.subscribe(bad), Err(Refused::Bad(_))));
    assert_eq!(MAX_STREAMS_PER_CONNECTION, Limits::default().streams);
}

#[test]
fn a_new_listen_key_takes_the_old_ones_place() {
    let mut session = Session::new();
    session.set_listen_key("old").unwrap();
    session.opened(0);
    session.set_listen_key("new").unwrap();
    assert_eq!(session.owed(1), vec![unsubscribe(2, &["old".to_owned()]), subscribe(3, &["new".to_owned()])]);
    assert_eq!(session.wanted(), &[Subscription::account("new")]);
    assert!(session.set_listen_key("").is_err());
    session.set_listen_key("new").unwrap();
    assert!(session.owed(2).is_empty(), "the same key again changes nothing");
}

#[test]
fn a_refusal_names_what_it_refused() {
    let mut session = Session::new();
    session.subscribe(trades()).unwrap();
    session.opened(0);
    let refused = session.received(5, r#"{"id": 1, "error": {"code": 2, "msg": "Invalid request"}}"#).unwrap();
    assert_eq!(
        refused,
        Frame::Refusal {
            id: Some(1),
            code: 2,
            message: "Invalid request".into(),
            asked: Some(Asked::Subscribe(names(&[trades()])))
        }
    );
    session.ask_subscriptions();
    assert_eq!(session.owed(6), vec![list_subscriptions(2)]);
}

#[test]
fn a_diff_book_chain_that_breaks_is_a_gap_and_the_next_update_carries_on() {
    let mut session = Session::new();
    session.subscribe(depth()).unwrap();
    session.subscribe(Subscription::partial_depth("ETHUSDT", Levels::Five, Speed::Ms100)).unwrap();
    session.opened(0);
    let lines: Vec<&str> = DIFFS.lines().collect();
    for line in &lines[..3] {
        assert!(matches!(session.received(1, line).unwrap(), Frame::Data { data: Data::Depth(_), .. }));
    }
    // One update lost.
    match session.received(2, lines[4]).unwrap() {
        Frame::Gap { gap, update } => {
            assert_eq!(gap.stream, "ethusdt@depth@100ms");
            assert_eq!((gap.expected, gap.got), (648_146_749_749, 648_146_753_712));
            assert_eq!(update.final_id, 648_146_756_372);
        }
        other => panic!("expected a gap, got {other:?}"),
    }
    assert!(matches!(session.received(3, lines[5]).unwrap(), Frame::Data { .. }), "the chain carries on from the gap");
    // A partial book's frames are the top as it stands, never a chain.
    let partial = lines[7].replace("ethusdt@depth@100ms", "ethusdt@depth5@100ms");
    assert!(matches!(session.received(4, &partial).unwrap(), Frame::Data { .. }));
}

#[test]
fn a_frame_that_does_not_read_still_shows_the_socket_alive() {
    let mut session = Session::new().with_timing(Timing { quiet_ms: 100, silent_ms: 200, ..Timing::DEFAULT });
    session.opened(0);
    assert!(session.received(150, "garbage").is_err());
    assert_eq!(session.tick(249), Tick::Wait);
    session.heard(300);
    assert_eq!(session.tick(449), Tick::Send, "quiet: asked for the subscriptions");
    assert_eq!(session.tick(500), Tick::Reconnect);
}
