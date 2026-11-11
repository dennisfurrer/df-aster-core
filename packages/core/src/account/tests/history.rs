//! Each history read sends the right call, and every row reads from the docs' example answers
//! (fixtures/aster/account_*.json, from the docs: no account was read live).

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};
use crate::utils::{OrderStatus, OrderType};

const OPEN_ORDERS: &str = include_str!("../../../fixtures/aster/account_open_orders.json");
const TRADES: &str = include_str!("../../../fixtures/aster/account_user_trades.json");
const INCOME: &str = include_str!("../../../fixtures/aster/account_income.json");
const INCOME_PAGE: &str = include_str!("../../../fixtures/aster/account_income_page.json");
const FORCE: &str = include_str!("../../../fixtures/aster/account_force_orders.json");
const MARGIN: &str = include_str!("../../../fixtures/aster/account_margin_history.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn fields(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs.iter().map(|(key, value)| (*key, (*value).to_owned())).collect()
}

fn one_order() -> String {
    serde_json::to_string(&serde_json::from_str::<serde_json::Value>(OPEN_ORDERS).unwrap()[0]).unwrap()
}

#[test]
fn an_order_is_looked_up_by_either_id() {
    let transport = Recorded::answering(&one_order());
    let order = block_on(query_order(&transport, "BTCUSDT", &OrderLookup::Id(1_917_641))).unwrap();
    let call = transport.only();
    assert_eq!((call.method, call.path, call.signed), (Method::Get, "/fapi/v3/order", true));
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("orderId", "1917641")]));
    assert_eq!((order.order_id, order.status.clone()), (1_917_641, OrderStatus::New));
    let transport = Recorded::answering(&one_order());
    block_on(open_order(&transport, "BTCUSDT", &OrderLookup::Client("abc".into()))).unwrap();
    let call = transport.only();
    assert_eq!(call.path, "/fapi/v3/openOrder");
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("origClientOrderId", "abc")]));
}

#[test]
fn open_orders_weigh_forty_for_every_symbol() {
    let transport = Recorded::answering(OPEN_ORDERS);
    let orders = block_on(open_orders(&transport, None)).unwrap();
    assert_eq!((transport.only().weight, transport.only().signed), (40, true));
    assert_eq!(orders[0].order_type, OrderType::TrailingStopMarket);
    assert_eq!(open_orders_call(Some("BTCUSDT")).weight, 1);
}

#[test]
fn all_orders_carry_the_window_in_order() {
    let range = TimeRange { start_time: Some(1), end_time: Some(2), limit: Some(500) };
    let transport = Recorded::answering(OPEN_ORDERS);
    block_on(all_orders(&transport, Some("BTCUSDT"), Some(7), &range)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight, call.signed), ("/fapi/v3/allOrders", 5, true));
    let want = [("symbol", "BTCUSDT"), ("orderId", "7"), ("startTime", "1"), ("endTime", "2"), ("limit", "500")];
    assert_eq!(call.fields, fields(&want));
}

#[test]
fn fills_read_with_their_fee_and_profit() {
    let transport = Recorded::answering(TRADES);
    let range = TimeRange { limit: Some(100), ..TimeRange::default() };
    let fills = block_on(user_trades(&transport, "BTCUSDT", None, Some(5), &range)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/userTrades", 5));
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("fromId", "5"), ("limit", "100")]));
    let fill = &fills[0];
    assert_eq!((fill.id, fill.order_id, fill.side.clone()), (698_759, 25_851_813, Side::Sell));
    assert_eq!((fill.price, fill.qty, fill.commission), (d("7819.01"), d("0.002"), d("-0.0781901")));
    assert_eq!((fill.realized_pnl, fill.position_side.clone()), (d("-0.91539999"), PositionSide::Short));
    assert!(!fill.buyer && !fill.maker);
}

#[test]
fn income_reads_by_type_and_page() {
    let query = IncomeQuery {
        symbol: Some("BTCUSDT".into()),
        income_type: Some(IncomeType::FundingFee),
        range: TimeRange { start_time: Some(10), end_time: None, limit: Some(1000) },
    };
    let transport = Recorded::answering(INCOME);
    let rows = block_on(income(&transport, &query)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight, call.signed), ("/fapi/v3/income", 30, true));
    let want = [("symbol", "BTCUSDT"), ("incomeType", "FUNDING_FEE"), ("startTime", "10"), ("limit", "1000")];
    assert_eq!(call.fields, fields(&want));
    assert_eq!((rows[0].income_type.clone(), rows[0].income), (IncomeType::Transfer, d("-0.375")));
    assert_eq!((rows[0].symbol.as_str(), rows[0].trade_id.as_str()), ("", ""));
    assert_eq!((rows[1].income_type.clone(), rows[1].trade_id.as_str()), (IncomeType::Commission, "2059192"));

    let transport = Recorded::answering(INCOME_PAGE);
    let page = block_on(income_page(&transport, &query, 2)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight), ("/fapi/v3/incomePage", 30));
    let want =
        [("symbol", "BTCUSDT"), ("incomeType", "FUNDING_FEE"), ("startTime", "10"), ("page", "2"), ("limit", "1000")];
    assert_eq!(call.fields, fields(&want));
    assert_eq!((page.total, page.page, page.has_more), (5_823, 2, true));
    assert_eq!(page.rows[0].tran_id, "9689322392");
    assert_eq!(IncomeType::parse("SOMETHING_NEW"), IncomeType::Other("SOMETHING_NEW".into()));
    assert_eq!(IncomeType::AutoExchange.to_string(), "AUTO_EXCHANGE");
}

#[test]
fn force_orders_weigh_by_symbol() {
    let transport = Recorded::answering(FORCE);
    let range = TimeRange::default();
    let orders = block_on(force_orders(&transport, None, Some(AutoCloseType::Adl), &range)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight, call.signed), ("/fapi/v3/forceOrders", 50, true));
    assert_eq!(call.fields, fields(&[("autoCloseType", "ADL")]));
    assert_eq!(force_orders_call(Some("BTCUSDT"), None, &range).weight, 20);
    assert_eq!(
        (orders[0].client_order_id.as_str(), orders[0].status.clone()),
        ("autoclose-1596107620040000020", OrderStatus::Filled)
    );
    assert_eq!(orders[1].avg_price, d("10979.82"));
}

#[test]
fn margin_changes_read_their_type() {
    let transport = Recorded::answering(MARGIN);
    let rows = block_on(margin_history(&transport, "BTCUSDT", Some(false), &TimeRange::default())).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.signed), ("/fapi/v3/positionMargin/history", true));
    assert_eq!(call.fields, fields(&[("symbol", "BTCUSDT"), ("type", "2")]));
    assert!(rows[0].is_added());
    assert_eq!((rows[1].amount, rows[1].delta_type.as_str()), (d("100"), "USER_ADJUST"));
}
