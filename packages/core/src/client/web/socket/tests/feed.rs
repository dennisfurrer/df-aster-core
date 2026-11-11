//! Run as wasm in Node (`just web`), the feed on Node's own `WebSocket`, against a websocket
//! server on 127.0.0.1 started from the test (`server.js`). Its frames were recorded from Aster's
//! socket on 2026-11-11 (`fixtures/aster/live_*`).

use std::{
    future::poll_fn,
    pin::{Pin, pin},
    task::Poll,
};

use crate::live::fixtures::{DIFFS, frame};
use crate::live::{Data, Frame, Limits, Refused, Speed, Timing, list_subscriptions, subscribe, unsubscribe};
use js_sys::Promise;
use wasm_bindgen::{JsCast, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

use super::*;

#[wasm_bindgen(module = "/fixtures/web/server.js")]
extern "C" {
    type Server;
    #[wasm_bindgen(static_method_of = Server)]
    fn listen() -> Promise;
    #[wasm_bindgen(method)]
    fn url(this: &Server) -> String;
    #[wasm_bindgen(method)]
    fn accept(this: &Server, ms: u32) -> Promise;
    #[wasm_bindgen(method)]
    fn close(this: &Server);

    type Peer;
    #[wasm_bindgen(method)]
    fn text(this: &Peer, ms: u32) -> Promise;
    #[wasm_bindgen(method)]
    fn send(this: &Peer, text: &str);
    #[wasm_bindgen(method)]
    fn close(this: &Peer);
    #[wasm_bindgen(method)]
    fn ended(this: &Peer, ms: u32) -> Promise;

    type Deadline;
    #[wasm_bindgen(constructor)]
    fn new(ms: u32) -> Deadline;
    #[wasm_bindgen(method, getter)]
    fn promise(this: &Deadline) -> Promise;
    #[wasm_bindgen(method)]
    fn cancel(this: &Deadline);
}

/// Quick enough for a test to see every rule fire.
const FAST: Timing = Timing {
    quiet_ms: 150,
    silent_ms: 400,
    steady_ms: 10_000,
    first_retry_ms: 20,
    last_retry_ms: 40,
    renew_after_ms: 86_400_000,
};

/// As quick to dial again, but no question in the time a test watches for nothing.
const QUIET: Timing = Timing { quiet_ms: 10_000, silent_ms: 20_000, ..FAST };

const WAIT: u32 = 5_000;

fn trades() -> Subscription {
    Subscription::agg_trade("BTCUSDT")
}

fn names(subscriptions: &[Subscription]) -> Vec<String> {
    subscriptions.iter().map(Subscription::name).collect()
}

async fn listen() -> Server {
    JsFuture::from(Server::listen()).await.unwrap().unchecked_into()
}

async fn accept(server: &Server) -> Peer {
    JsFuture::from(server.accept(WAIT)).await.unwrap().unchecked_into()
}

async fn text(peer: &Peer) -> String {
    JsFuture::from(peer.text(WAIT)).await.unwrap().as_string().unwrap()
}

/// `future`'s output, or `None` when `ms` passes first.
async fn within<T>(ms: u32, future: impl Future<Output = T>) -> Option<T> {
    let deadline = Deadline::new(ms);
    let mut future = pin!(future);
    let mut timeout = JsFuture::from(deadline.promise());
    let out = poll_fn(|cx| match future.as_mut().poll(cx) {
        Poll::Ready(out) => Poll::Ready(Some(out)),
        Poll::Pending => match Pin::new(&mut timeout).poll(cx) {
            Poll::Ready(_) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        },
    })
    .await;
    deadline.cancel();
    out
}

async fn event(feed: &mut Feed) -> Event {
    within(WAIT, feed.next()).await.expect("an event in time").expect("a running feed")
}

async fn delivered(feed: &mut Feed) -> (Event, Option<String>) {
    within(WAIT, feed.next_with_text()).await.expect("an event in time").expect("a running feed")
}

#[wasm_bindgen_test]
async fn it_subscribes_on_open_and_hands_back_typed_frames() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new());
    feed.subscribe(trades()).await.unwrap();
    let peer = accept(&server).await;
    assert_eq!(text(&peer).await, subscribe(1, &names(&[trades()])));
    peer.send(frame("btcusdt@aggTrade"));
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Data { data: Data::AggTrade(trade), .. } => assert_eq!(trade.id, 89_238_011),
            other => panic!("expected the trade, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
    feed.close();
    server.close();
}

/// What a feed reads, it can also give as sent; an open or a close has no text.
#[wasm_bindgen_test]
async fn each_frame_comes_with_the_text_it_arrived_as_when_asked() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new());
    let peer = accept(&server).await;
    assert_eq!(delivered(&mut feed).await, (Event::Opened, None));
    peer.send(frame("btcusdt@aggTrade"));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Frame(_)));
    assert_eq!(text.as_deref(), Some(frame("btcusdt@aggTrade")));
    peer.send("not json");
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Unreadable(_)));
    assert_eq!(text.as_deref(), Some("not json"), "what did not read too");
    peer.close();
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Closed { .. }));
    assert_eq!(text, None);
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_dropped_socket_is_dialled_again_and_everything_resubscribed() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new().with_timing(QUIET));
    let book = Subscription::book_ticker("BTCUSDT");
    feed.subscribe(trades()).await.unwrap();
    feed.subscribe(book.clone()).await.unwrap();
    let wanted = names(&[trades(), book]);
    let first = accept(&server).await;
    assert_eq!(text(&first).await, subscribe(1, &wanted));
    first.close();
    let second = accept(&server).await;
    assert_eq!(text(&second).await, subscribe(2, &wanted));
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Closed { retry, .. } => {
            assert!((20..=25).contains(&retry.after_ms), "the first delay and a quarter: {}", retry.after_ms);
        }
        other => panic!("the drop is reported before the next open, got {other:?}"),
    }
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    server.close();
}

/// A quiet socket is asked for its subscriptions, and one that answers nothing is given up.
#[wasm_bindgen_test]
async fn a_quiet_socket_is_asked_and_a_silent_one_dialled_again() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new().with_timing(FAST));
    let silent = accept(&server).await;
    assert_eq!(text(&silent).await, list_subscriptions(1), "asked first");
    let _again = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Closed { reason, .. } => assert_eq!(reason, "nothing arrived for too long"),
        other => panic!("expected the silence, got {other:?}"),
    }
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_book_gap_is_handed_back() {
    let server = listen().await;
    let depth = Subscription::depth("ETHUSDT", Speed::Ms100);
    let mut feed = Feed::start_at(server.url(), Session::new().with_timing(QUIET));
    feed.subscribe(depth.clone()).await.unwrap();
    let peer = accept(&server).await;
    assert_eq!(text(&peer).await, subscribe(1, &names(&[depth])));
    assert_eq!(event(&mut feed).await, Event::Opened);
    let lines: Vec<&str> = DIFFS.lines().collect();
    peer.send(lines[0]);
    assert!(matches!(event(&mut feed).await, Event::Frame(frame) if matches!(*frame, Frame::Data { .. })));
    peer.send(lines[2]);
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Gap { gap, .. } => assert_eq!((gap.expected, gap.got), (648_146_743_465, 648_146_746_136)),
            other => panic!("expected the gap, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_new_listen_key_replaces_the_old_and_a_bad_one_is_refused() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new().with_timing(QUIET));
    feed.set_listen_key("first").await.unwrap();
    let peer = accept(&server).await;
    assert_eq!(text(&peer).await, subscribe(1, &["first".to_owned()]));
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.set_listen_key("second").await.unwrap();
    assert_eq!(text(&peer).await, unsubscribe(2, &["first".to_owned()]));
    assert_eq!(text(&peer).await, subscribe(3, &["second".to_owned()]));
    assert!(matches!(feed.set_listen_key("").await, Err(FeedError::Refused(Refused::Bad(_)))));
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_subscription_past_the_limit_is_refused() {
    let server = listen().await;
    let feed = Feed::start_at(server.url(), Session::new().with_limits(Limits { streams: 1, messages_per_second: 5 }));
    feed.subscribe(trades()).await.unwrap();
    let refused = feed.subscribe(Subscription::AllTickers).await;
    assert_eq!(refused, Err(FeedError::Refused(Refused::Streams { limit: 1 })));
    feed.close();
    server.close();
}

#[wasm_bindgen_test]
async fn a_socket_that_cannot_open_is_dialled_again_later_each_time() {
    let server = listen().await;
    let url = server.url();
    server.close();
    let mut feed = Feed::start_at(url, Session::new().with_timing(FAST));
    let mut delays = Vec::new();
    for _ in 0..2 {
        match event(&mut feed).await {
            Event::Closed { reason, retry } => {
                assert!(!reason.is_empty());
                delays.push(retry.after_ms);
            }
            other => panic!("expected the failed dial, got {other:?}"),
        }
    }
    assert!((20..=25).contains(&delays[0]), "the first delay and a quarter: {delays:?}");
    assert!((40..=50).contains(&delays[1]), "doubled: {delays:?}");
    feed.close();
}

#[wasm_bindgen_test]
async fn a_closed_feed_closes_its_socket_and_dials_no_more() {
    let server = listen().await;
    let mut feed = Feed::start_at(server.url(), Session::new().with_timing(QUIET));
    let peer = accept(&server).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    JsFuture::from(peer.ended(WAIT)).await.expect("the socket ends");
    assert!(JsFuture::from(server.accept(300)).await.is_err(), "no dial after the close");
    server.close();
}
