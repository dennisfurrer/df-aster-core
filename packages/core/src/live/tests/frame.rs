//! Every stream's frame as Aster sent it on 2026-11-11, every account event as the docs give
//! it, answers and refusals, and frames this build does not know.

use super::*;
use crate::live::account::{Execution, ModesChange};
use crate::live::fixtures::{STREAM, docs, frame};
use crate::utils::{Dec, Interval, MarginType, OrderStatus, OrderType, PositionSide, Side, TimeInForce, WorkingType};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn data(text: &str) -> (Option<String>, Data) {
    match decode(text).unwrap() {
        Frame::Data { stream, data } => (stream, data),
        other => panic!("expected data, got {other:?}"),
    }
}

#[test]
fn every_recorded_frame_reads() {
    for line in STREAM.lines() {
        let frame = decode(line).unwrap();
        if let Frame::Data { data: Data::Other { event, .. }, .. } = &frame {
            panic!("{event} was not read: {line}");
        }
    }
}

#[test]
fn market_streams_read_with_exact_decimals() {
    let (stream, trade) = data(frame("btcusdt@aggTrade"));
    assert_eq!(stream.as_deref(), Some("btcusdt@aggTrade"));
    let Data::AggTrade(trade) = trade else { panic!("{trade:?}") };
    assert_eq!(
        (trade.symbol.as_str(), trade.id, trade.price, trade.quantity),
        ("BTCUSDT", 89_238_011, d("73876.0"), d("0.001"))
    );
    assert_eq!(
        (trade.first_trade_id, trade.trade_time, trade.taker_side()),
        (149_792_472, 1_794_400_120_050, Side::Buy)
    );

    let Data::MarkPrice(mark) = data(frame("btcusdt@markPrice")).1 else { panic!() };
    assert_eq!(
        (mark.mark_price, mark.index_price, mark.settle_price),
        (d("73876"), d("73904.89891304"), Some(d("74001.28881413")))
    );
    assert_eq!((mark.funding_rate, mark.next_funding_time), (d("0.000067"), 1_794_412_800_000));

    let Data::Kline(kline) = data(frame("btcusdt@kline_1m")).1 else { panic!() };
    let bar = &kline.bar;
    assert_eq!(
        (bar.interval.clone(), bar.open_time, bar.close_time, bar.closed),
        (Interval::M1, 1_794_400_080_000, 1_794_400_139_999, false)
    );
    assert_eq!((bar.open, bar.high, bar.low, bar.close), (d("73839.3"), d("73876"), d("73833"), d("73876")));
    assert_eq!(
        (bar.volume, bar.quote_volume, bar.trades, bar.taker_buy_volume),
        (d("2.367"), d("174825.2725"), 36, d("0.997"))
    );

    let Data::MiniTicker(mini) = data(frame("btcusdt@miniTicker")).1 else { panic!() };
    assert_eq!(
        (mini.close, mini.open, mini.volume, mini.quote_volume),
        (d("73876"), d("74260.8"), d("7466.307"), d("550075059.57"))
    );

    let Data::Ticker(ticker) = data(frame("btcusdt@ticker")).1 else { panic!() };
    assert_eq!(
        (ticker.price_change, ticker.price_change_percent, ticker.weighted_avg_price),
        (d("-384.8"), d("-0.518"), d("73674.3"))
    );
    assert_eq!((ticker.first_trade_id, ticker.last_trade_id, ticker.trades), (149_730_765, 149_792_472, 61_695));

    let Data::BookTicker(book) = data(frame("btcusdt@bookTicker")).1 else { panic!() };
    assert_eq!(
        (book.bid_price, book.bid_quantity, book.ask_price, book.ask_quantity),
        (d("73875.9"), d("2.311"), d("73876"), d("0.001"))
    );
    assert_eq!(book.update_id, 648_146_296_273);

    let Data::Depth(top) = data(frame("btcusdt@depth5@100ms")).1 else { panic!() };
    assert_eq!(
        (top.bids.len(), top.asks.len(), top.bids[0].price, top.asks[0].quantity),
        (5, 5, d("73875.9"), d("0.001"))
    );
    assert_eq!(
        (top.first_id, top.final_id, top.previous_final_id),
        (648_146_280_361, 648_146_281_260, 648_146_280_265)
    );

    let Data::Depth(diff) = data(frame("btcusdt@depth@100ms")).1 else { panic!() };
    assert!(diff.bids.iter().any(|level| level.quantity.is_zero()), "a removed level reads as zero");

    let Data::TradePro(pro) = data(frame("btcusdt@tradepro")).1 else { panic!() };
    assert_eq!((pro.id, pro.price, pro.quantity), (149_792_472, d("73876"), d("0.001")));
    assert_eq!(pro.taker(), Some("0x2239d39AD5C4B14911f39cABD21709819BB385C4"));
    assert_eq!(pro.maker(), Some("0x58e58CDf3b0cb6F56f5061D2702093Cd40E02d76"));

    let Data::Liquidation(liquidation) = data(docs("forceOrder", 0)).1 else { panic!() };
    let order = &liquidation.order;
    assert_eq!(
        (order.side.clone(), order.order_type.clone(), order.time_in_force.clone()),
        (Side::Sell, OrderType::Limit, TimeInForce::Ioc)
    );
    assert_eq!((order.status.clone(), order.filled, order.avg_price), (OrderStatus::Filled, d("0.014"), d("9910")));
}

#[test]
fn every_symbol_arrays_read_by_their_first_event_or_their_stream() {
    let (stream, marks) = data(docs("markPriceUpdate", 0));
    assert_eq!(stream, None, "a raw socket names no stream");
    let Data::MarkPrices(marks) = marks else { panic!() };
    assert_eq!(marks[0].mark_price, d("11185.87786614"));
    let Data::MiniTickers(minis) = data(docs("24hrMiniTicker", 0)).1 else { panic!() };
    assert_eq!(minis[0].quote_volume, d("18"));
    let (_, empty) = data(r#"{"stream":"!ticker@arr","data":[]}"#);
    assert_eq!(empty, Data::Tickers(Vec::new()));
}

#[test]
fn account_events_read_as_the_docs_give_them() {
    let Data::ListenKeyExpired(expired) = data(docs("listenKeyExpired", 0)).1 else { panic!() };
    assert_eq!(expired.event_time, 1_576_653_824_250);

    let Data::MarginCall(call) = data(docs("MARGIN_CALL", 0)).1 else { panic!() };
    let position = &call.positions[0];
    assert_eq!(
        (call.cross_wallet_balance, position.position_side.clone(), position.margin_type.clone()),
        (Some(d("3.16812045")), PositionSide::Long, MarginType::Crossed)
    );
    assert_eq!(
        (position.amount, position.unrealized_pnl, position.maintenance_margin),
        (d("1.327"), d("-1.166074"), d("1.614445"))
    );

    let Data::AccountUpdate(update) = data(docs("ACCOUNT_UPDATE", 0)).1 else { panic!() };
    let change = &update.update;
    assert_eq!((change.reason.as_str(), change.balances.len(), change.positions.len()), ("ORDER", 2, 3));
    assert_eq!((change.balances[1].asset.as_str(), change.balances[1].balance_change), ("BUSD", d("-49.12345678")));
    let short = &change.positions[2];
    assert_eq!(
        (short.amount, short.accumulated_realized, short.margin_type.clone(), short.position_side.clone()),
        (d("-10"), d("-45.04"), MarginType::Isolated, PositionSide::Short)
    );
    assert_eq!(short.break_even_price, None);

    let Data::OrderUpdate(update) = data(docs("ORDER_TRADE_UPDATE", 0)).1 else { panic!() };
    let order = &update.order;
    assert_eq!((order.order_id, order.client_order_id.as_str(), order.side.clone()), (8_886_774, "TEST", Side::Sell));
    assert_eq!(
        (order.order_type.clone(), order.execution.clone(), order.status.clone()),
        (OrderType::TrailingStopMarket, Execution::New, OrderStatus::New)
    );
    assert_eq!(
        (order.stop_price, order.activation_price, order.callback_rate),
        (d("7103.04"), Some(d("7476.89")), Some(d("5")))
    );
    assert_eq!(
        (order.commission_asset.as_deref(), order.commission, order.asks_notional),
        (Some("USDT"), Some(Dec::ZERO), d("9.91"))
    );
    assert_eq!(
        (order.working_type.clone(), order.position_side.clone(), order.reduce_only),
        (Some(WorkingType::ContractPrice), PositionSide::Long, false)
    );

    let Data::ConfigUpdate(leverage) = data(docs("ACCOUNT_CONFIG_UPDATE", 0)).1 else { panic!() };
    assert_eq!(leverage.leverage.map(|change| (change.symbol, change.leverage)), Some(("BTCUSDT".to_owned(), 25)));
    let Data::ConfigUpdate(modes) = data(docs("ACCOUNT_CONFIG_UPDATE", 1)).1 else { panic!() };
    assert_eq!(
        modes.modes,
        Some(ModesChange { multi_assets: Some(true), token_fees: Some(true), hedge_mode: Some(true) })
    );

    let Data::Announcement(public) = data(docs("announcement", 0)).1 else { panic!() };
    let english = public.en.unwrap();
    assert_eq!((english.id, english.category.as_str(), english.is_direct()), (161, "NEW_LISTING", false));
    assert_eq!(public.zh.unwrap().title, "测试公告");
    let Data::Announcement(direct) = data(docs("announcement", 1)).1 else { panic!() };
    assert!(direct.en.unwrap().is_direct());
}

#[test]
fn answers_and_refusals_read_with_their_ids() {
    assert_eq!(
        decode(r#"{"id":1,"result":null}"#).unwrap(),
        Frame::Answer { id: 1, result: serde_json::Value::Null, asked: None }
    );
    let Frame::Answer { id, result, .. } =
        decode(STREAM.lines().find(|l| l.starts_with(r#"{"id":2"#)).unwrap()).unwrap()
    else {
        panic!()
    };
    assert_eq!((id, result.as_array().map(Vec::len)), (2, Some(4)));
    let refused = decode(STREAM.lines().find(|l| l.contains("error")).unwrap()).unwrap();
    let Frame::Refusal { id, code, message, .. } = refused else { panic!() };
    assert_eq!((id, code), (Some(4), 2));
    assert!(message.starts_with("Invalid request: unknown method"));
    let bare = decode(r#"{"code": 0, "msg": "Unknown property"}"#).unwrap();
    assert!(matches!(bare, Frame::Refusal { id: None, code: 0, .. }));
}

#[test]
fn what_this_build_does_not_know_is_kept_and_new_fields_are_ignored() {
    let (_, other) = data(r#"{"stream":"btcusdt@newThing","data":{"e":"newThing","x":1}}"#);
    assert!(matches!(other, Data::Other { ref event, .. } if event == "newThing"));
    let extra = r#"{"e":"aggTrade","E":1,"a":2,"s":"BTCUSDT","p":"1.5","q":"2","f":3,"l":3,"T":4,"m":true,"zz":"new"}"#;
    let Data::AggTrade(trade) = data(extra).1 else { panic!() };
    assert_eq!((trade.price, trade.taker_side()), (d("1.5"), Side::Sell));
    assert!(decode("not json").is_err());
    let broken = decode(r#"{"e":"aggTrade","s":"BTCUSDT","p":"one","q":"2"}"#).unwrap_err();
    assert!(broken.excerpt.contains("aggTrade"));
    assert!(decode(r#"{"result":null}"#).is_err(), "an answer names its id");
}
