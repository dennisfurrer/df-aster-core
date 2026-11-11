//! The market streams, live for a few seconds: each subscribed stream sends a frame that reads.

use std::collections::BTreeSet;
use std::time::Duration;

use df_aster_core::client::native::Feed;
use df_aster_core::live::{Event, Frame, Levels, Session, Speed, Subscription};
use df_aster_core::utils::Interval;

#[tokio::test]
#[ignore = "reads Aster"]
async fn market_streams_send_frames_that_read() {
    let symbol = || "btcusdt".to_owned();
    let wanted = [
        Subscription::AggTrade { symbol: symbol() },
        Subscription::MarkPrice { symbol: symbol(), every_second: true },
        Subscription::Kline { symbol: symbol(), interval: Interval::M1 },
        Subscription::BookTicker { symbol: symbol() },
        Subscription::PartialDepth { symbol: symbol(), levels: Levels::Five, speed: Speed::Ms100 },
    ];
    let mut feed = Feed::start(Session::new());
    for subscription in wanted.iter().cloned() {
        feed.subscribe(subscription).await.unwrap();
    }
    let names: BTreeSet<String> = wanted.iter().map(Subscription::name).collect();
    let mut heard = BTreeSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while heard != names {
        let event = tokio::time::timeout_at(deadline, feed.next()).await.expect("every stream within 20 s").unwrap();
        match event {
            Event::Frame(frame) => {
                if let Frame::Data { stream: Some(stream), .. } = *frame {
                    heard.insert(stream);
                }
            }
            Event::Unreadable(bad) => panic!("{bad:?}"),
            Event::Opened | Event::Closed { .. } => {}
        }
    }
}
