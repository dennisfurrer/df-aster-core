//! Against a websocket server on 127.0.0.1. Its frames were recorded from Aster's socket on
//! 2026-11-11 (`fixtures/aster/live_*`). Nothing leaves the machine.

use std::time::Duration;

use crate::client::Client;
use crate::live::fixtures::{DIFFS, frame};
use crate::live::{
    Data, Frame, Limits, Refused, Session, Speed, Subscription, Timing, list_subscriptions, subscribe, unsubscribe,
};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

use super::*;

/// Quick enough for a test to see every rule fire.
const FAST: Timing = Timing {
    quiet_ms: 150,
    silent_ms: 400,
    steady_ms: 10_000,
    first_retry_ms: 20,
    last_retry_ms: 40,
    renew_after_ms: 86_400_000,
};

const WAIT: Duration = Duration::from_secs(5);

fn trades() -> Subscription {
    Subscription::agg_trade("BTCUSDT")
}

fn names(subscriptions: &[Subscription]) -> Vec<String> {
    subscriptions.iter().map(Subscription::name).collect()
}

async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    (listener, url)
}

async fn accept(listener: &TcpListener) -> WebSocketStream<TcpStream> {
    let (stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
    accept_async(stream).await.unwrap()
}

async fn text(socket: &mut WebSocketStream<TcpStream>) -> String {
    loop {
        match timeout(WAIT, socket.next()).await.unwrap().unwrap().unwrap() {
            Message::Text(text) => return text.to_string(),
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("expected text, got {other:?}"),
        }
    }
}

async fn event(feed: &mut Feed) -> Event {
    timeout(WAIT, feed.next()).await.unwrap().unwrap()
}

async fn delivered(feed: &mut Feed) -> (Event, Option<String>) {
    timeout(WAIT, feed.next_with_text()).await.unwrap().unwrap()
}

#[tokio::test]
async fn it_subscribes_on_open_and_hands_back_typed_frames() {
    let (listener, url) = listen().await;
    let feed = Feed::start_at(url, Session::new());
    feed.subscribe(trades()).await.unwrap();
    let mut feed = feed;
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(1, &names(&[trades()])));
    server.send(Message::text(frame("btcusdt@aggTrade"))).await.unwrap();
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Data { stream, data: Data::AggTrade(trade) } => {
                assert_eq!((stream.as_deref(), trade.id), (Some("btcusdt@aggTrade"), 89_238_011));
            }
            other => panic!("expected the trade, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
}

/// What a feed reads, it can also give as sent; an open or a close has no text.
#[tokio::test]
async fn each_frame_comes_with_the_text_it_arrived_as_when_asked() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new());
    let mut server = accept(&listener).await;
    let trade = frame("btcusdt@aggTrade");
    server.send(Message::text(trade)).await.unwrap();
    server.send(Message::text("not json")).await.unwrap();
    assert_eq!(delivered(&mut feed).await, (Event::Opened, None));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Frame(_)));
    assert_eq!(text.as_deref(), Some(trade));
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Unreadable(_)));
    assert_eq!(text.as_deref(), Some("not json"), "what did not read too");
    drop(server);
    let (event, text) = delivered(&mut feed).await;
    assert!(matches!(event, Event::Closed { .. }));
    assert_eq!(text, None);
}

#[tokio::test]
async fn a_dropped_socket_is_dialled_again_and_everything_resubscribed() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new().with_timing(FAST));
    let book = Subscription::book_ticker("BTCUSDT");
    feed.subscribe(trades()).await.unwrap();
    feed.subscribe(book.clone()).await.unwrap();
    let wanted = names(&[trades(), book]);
    let mut first = accept(&listener).await;
    assert_eq!(text(&mut first).await, subscribe(1, &wanted));
    first.close(None).await.unwrap();
    drop(first);
    let mut second = accept(&listener).await;
    assert_eq!(text(&mut second).await, subscribe(2, &wanted));
    let mut saw_closed = false;
    while let Ok(Some(event)) = timeout(Duration::from_millis(200), feed.next()).await {
        saw_closed |= matches!(event, Event::Closed { .. });
    }
    assert!(saw_closed, "the drop is reported before the next open");
}

/// A quiet socket is asked for its subscriptions; the answer shows it alive.
#[tokio::test]
async fn a_quiet_socket_is_asked_and_its_answer_comes_back_named() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new().with_timing(FAST));
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, list_subscriptions(1));
    server.send(Message::text(r#"{"result":[],"id":1}"#)).await.unwrap();
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Frame(frame) => {
            assert!(matches!(*frame, Frame::Answer { id: 1, asked: Some(crate::live::Asked::ListSubscriptions), .. }));
        }
        other => panic!("expected the answer, got {other:?}"),
    }
}

/// The venue's protocol pings are answered by tungstenite and keep the socket counted alive.
#[tokio::test]
async fn protocol_pings_are_answered_and_count_as_heard() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new().with_timing(FAST));
    let mut server = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    for _ in 0..6 {
        server.send(Message::Ping(vec![7].into())).await.unwrap();
        match timeout(WAIT, server.next()).await.unwrap().unwrap().unwrap() {
            Message::Pong(payload) => assert_eq!(payload.as_ref(), &[7]),
            Message::Text(text) => assert_eq!(text.as_str(), list_subscriptions(1), "only once a quiet spell"),
            other => panic!("expected a pong, got {other:?}"),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(timeout(Duration::from_millis(50), feed.next()).await.is_err(), "600 ms of pings, and no close");
}

#[tokio::test]
async fn a_socket_nothing_arrives_on_is_given_up_and_dialled_again() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new().with_timing(FAST));
    let _silent = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    match event(&mut feed).await {
        Event::Closed { reason, retry } => {
            assert_eq!(reason, "nothing arrived for too long");
            assert!((20..=25).contains(&retry.after_ms), "{retry:?}");
        }
        other => panic!("expected the silence, got {other:?}"),
    }
    let _again = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
}

/// A diff book update that does not follow is handed back as a gap.
#[tokio::test]
async fn a_book_gap_is_handed_back() {
    let (listener, url) = listen().await;
    let depth = Subscription::depth("ETHUSDT", Speed::Ms100);
    let mut feed = Feed::start_at(url, Session::new());
    feed.subscribe(depth.clone()).await.unwrap();
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(1, &names(&[depth])));
    let lines: Vec<&str> = DIFFS.lines().collect();
    server.send(Message::text(lines[0])).await.unwrap();
    server.send(Message::text(lines[2])).await.unwrap();
    assert_eq!(event(&mut feed).await, Event::Opened);
    assert!(matches!(event(&mut feed).await, Event::Frame(frame) if matches!(*frame, Frame::Data { .. })));
    match event(&mut feed).await {
        Event::Frame(frame) => match *frame {
            Frame::Gap { gap, .. } => assert_eq!((gap.expected, gap.got), (648_146_743_465, 648_146_746_136)),
            other => panic!("expected the gap, got {other:?}"),
        },
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[tokio::test]
async fn a_new_listen_key_replaces_the_old_on_the_open_socket() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new());
    feed.set_listen_key("first").await.unwrap();
    let mut server = accept(&listener).await;
    assert_eq!(text(&mut server).await, subscribe(1, &["first".to_owned()]));
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.set_listen_key("second").await.unwrap();
    assert_eq!(text(&mut server).await, unsubscribe(2, &["first".to_owned()]));
    assert_eq!(text(&mut server).await, subscribe(3, &["second".to_owned()]));
    assert!(matches!(feed.set_listen_key("").await, Err(FeedError::Refused(Refused::Bad(_)))));
}

#[tokio::test]
async fn a_subscription_past_the_limit_is_refused() {
    let (_listener, url) = listen().await;
    let feed = Feed::start_at(url, Session::new().with_limits(Limits { streams: 1, messages_per_second: 5 }));
    feed.subscribe(trades()).await.unwrap();
    let refused = feed.subscribe(Subscription::AllTickers).await;
    assert_eq!(refused, Err(FeedError::Refused(Refused::Streams { limit: 1 })));
}

#[tokio::test]
async fn a_closed_feed_closes_its_socket_and_dials_no_more() {
    let (listener, url) = listen().await;
    let mut feed = Feed::start_at(url, Session::new());
    let mut server = accept(&listener).await;
    assert_eq!(event(&mut feed).await, Event::Opened);
    feed.close();
    let ended = timeout(WAIT, server.next()).await.unwrap();
    assert!(matches!(ended, None | Some(Ok(Message::Close(_))) | Some(Err(_))), "{ended:?}");
    assert!(timeout(Duration::from_millis(300), listener.accept()).await.is_err(), "no dial after the close");
}

/// The keepalive task ends by itself once the last holder of the client lets go. A reader has no
/// agent, so its keepalive is refused before anything goes out.
#[tokio::test]
async fn keeping_the_listen_key_alive_ends_with_the_last_holder() {
    let client = Arc::new(Client::reader(Native::new(), clock));
    let keeping = keep_listen_key_alive(&client, Duration::from_millis(20));
    tokio::time::sleep(Duration::from_millis(70)).await;
    drop(client);
    timeout(WAIT, keeping).await.unwrap().unwrap();
}
