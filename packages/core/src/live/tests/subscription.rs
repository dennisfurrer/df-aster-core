//! Every stream's name, read back the same, and names the venue cannot serve refused.

use super::*;

#[test]
fn every_stream_has_its_name_and_reads_back() {
    let all = [
        (Subscription::agg_trade("BTCUSDT"), "btcusdt@aggTrade"),
        (Subscription::mark_price("BTCUSDT"), "btcusdt@markPrice"),
        (Subscription::MarkPrice { symbol: "btcusdt".into(), every_second: true }, "btcusdt@markPrice@1s"),
        (Subscription::AllMarkPrices { every_second: false }, "!markPrice@arr"),
        (Subscription::AllMarkPrices { every_second: true }, "!markPrice@arr@1s"),
        (Subscription::kline("ETHUSDT", Interval::H4), "ethusdt@kline_4h"),
        (Subscription::kline("ETHUSDT", Interval::Mo1), "ethusdt@kline_1M"),
        (Subscription::mini_ticker("BTCUSDT"), "btcusdt@miniTicker"),
        (Subscription::AllMiniTickers, "!miniTicker@arr"),
        (Subscription::ticker("BTCUSDT"), "btcusdt@ticker"),
        (Subscription::AllTickers, "!ticker@arr"),
        (Subscription::book_ticker("BTCUSDT"), "btcusdt@bookTicker"),
        (Subscription::AllBookTickers, "!bookTicker"),
        (Subscription::liquidations("BTCUSDT"), "btcusdt@forceOrder"),
        (Subscription::AllLiquidations, "!forceOrder@arr"),
        (Subscription::partial_depth("BTCUSDT", Levels::Five, Speed::Ms100), "btcusdt@depth5@100ms"),
        (Subscription::partial_depth("BTCUSDT", Levels::Twenty, Speed::Ms250), "btcusdt@depth20"),
        (Subscription::depth("BTCUSDT", Speed::Ms250), "btcusdt@depth"),
        (Subscription::depth("BTCUSDT", Speed::Ms500), "btcusdt@depth@500ms"),
        (Subscription::trade_pro("BTCUSDT"), "btcusdt@tradepro"),
        (Subscription::account("pqia91ma19a5s61cv6a81va65sdf19v8a65a1"), "pqia91ma19a5s61cv6a81va65sdf19v8a65a1"),
        (Subscription::agg_trade("龙虾USDT"), "龙虾usdt@aggTrade"),
    ];
    for (subscription, name) in all {
        assert_eq!(subscription.name(), name);
        assert_eq!(subscription.to_string(), name);
        assert_eq!(Subscription::parse(name).unwrap(), subscription, "{name}");
        assert!(subscription.check().is_ok(), "{name}");
    }
    assert!(Subscription::depth("btcusdt", Speed::Ms100).is_diff_depth());
    assert!(!Subscription::partial_depth("btcusdt", Levels::Ten, Speed::Ms100).is_diff_depth());
    assert!(Subscription::account("k").is_account());
    assert_eq!(Subscription::ticker("BTCUSDT").symbol(), Some("btcusdt"));
    assert_eq!(Subscription::AllTickers.symbol(), None);
}

#[test]
fn what_the_venue_cannot_serve_is_refused() {
    for bad in
        ["btcusdt@depth7", "btcusdt@depth@50ms", "btcusdt@trades", "BTCUSDT@aggTrade", "@aggTrade", "btcusdt@kline_2w"]
    {
        assert!(Subscription::parse(bad).is_err(), "{bad}");
    }
    assert_eq!(Subscription::account("").check(), Err(BadSubscription::ListenKey));
    assert!(Subscription::account("a/b").check().is_err());
    assert!(Subscription::agg_trade("btc usdt").check().is_err());
    assert_eq!(Levels::of(10), Some(Levels::Ten));
    assert_eq!(Levels::of(15), None);
}
