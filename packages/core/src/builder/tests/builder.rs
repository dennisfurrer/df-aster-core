//! Each builder read sends its query in order and reads its page from the docs' example answers
//! (fixtures/aster/builder_*.json, from the docs: no builder was read live).

use super::*;
use crate::test_support::{Recorded, block_on};
use crate::utils::{MarginType, OrderStatus};

const ACCOUNTS: &str = include_str!("../../../fixtures/aster/builder_user_accounts.json");
const OPEN_ORDERS: &str = include_str!("../../../fixtures/aster/builder_user_open_orders.json");
const BALANCES: &str = include_str!("../../../fixtures/aster/builder_user_balances.json");
const POSITIONS: &str = include_str!("../../../fixtures/aster/builder_user_position_risk.json");
const RATES: &str = include_str!("../../../fixtures/aster/builder_user_commission_rates.json");
const TRADES: &str = include_str!("../../../fixtures/aster/builder_user_trades.json");
const COMMISSIONS: &str = include_str!("../../../fixtures/aster/builder_user_commissions.json");
const ALL_ORDERS: &str = include_str!("../../../fixtures/aster/builder_user_all_orders.json");
const APPROVED: &str = include_str!("../../../fixtures/aster/builder_approved_users.json");

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn keys(call: &Call) -> Vec<&'static str> {
    call.fields.iter().map(|(key, _)| *key).collect()
}

fn full() -> BuilderQuery {
    BuilderQuery {
        users: vec![
            "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed".parse().unwrap(),
            "0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359".parse().unwrap(),
        ],
        symbol: Some("BTCUSDT".into()),
        start_time: Some(1),
        end_time: Some(2),
        page: Some(1),
        limit: Some(50),
    }
}

#[test]
fn each_read_takes_what_applies_in_order() {
    let query = full();
    let users = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed,0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359";
    let all = ["userAddresses", "symbol", "startTime", "endTime", "page", "limit"];
    let cases: [(Call, &str, &[&str]); 9] = [
        (user_accounts_call(&query), "userAccounts", &["userAddresses", "symbol", "page", "limit"]),
        (user_open_orders_call(&query), "userOpenOrders", &["userAddresses", "symbol", "page", "limit"]),
        (user_balances_call(&query), "userBalances", &["userAddresses", "page", "limit"]),
        (user_positions_call(&query), "userPositionRisk", &["userAddresses", "symbol", "page", "limit"]),
        (
            user_commission_rates_call(&query, "ETHUSDT"),
            "userCommissionRates",
            &["userAddresses", "symbol", "page", "limit"],
        ),
        (user_trades_call(&query), "userTrades", &["userAddresses", "startTime", "endTime", "page", "limit"]),
        (user_commissions_call(&query), "userCommissions", &["userAddresses", "startTime", "endTime", "page", "limit"]),
        (user_all_orders_call(&query), "userAllOrders", &all),
        (approved_users_call(&query), "approvedUserList", &["startTime", "endTime", "page", "limit"]),
    ];
    for (call, name, want) in cases {
        assert_eq!(call.path, format!("/fapi/v3/builder/{name}"));
        assert_eq!((call.signed, call.weight), (true, 5), "{name}");
        assert_eq!(keys(&call), want, "{name}");
        if want.contains(&"userAddresses") {
            assert_eq!(call.value("userAddresses"), Some(users), "{name}");
        }
    }
    assert_eq!(user_commission_rates_call(&query, "ETHUSDT").value("symbol"), Some("ETHUSDT"));
    assert!(user_balances_call(&BuilderQuery::default()).fields.is_empty());
}

#[test]
fn pages_read_their_rows() {
    let query = BuilderQuery::default();
    let accounts = block_on(user_accounts(&Recorded::answering(ACCOUNTS), &query)).unwrap();
    assert_eq!((accounts.total, accounts.page_size, accounts.has_more), (2, 50, false));
    let account = &accounts.rows[0];
    assert_eq!((account.address.as_str(), account.symbol_config[0].leverage), ("0x1234...abcd", 20));
    assert_eq!(account.symbol_config[0].notional_limit_coef, d("10"));
    assert!(accounts.errors.is_empty());

    let open = block_on(user_open_orders(&Recorded::answering(OPEN_ORDERS), &query)).unwrap();
    assert_eq!((open.rows[0].address.as_str(), open.rows[0].row.order_id), ("0x1234...abcd", 1_917_641));
    assert_eq!((open.rows[0].row.status.clone(), open.rows[0].row.stop_price), (OrderStatus::New, d("9300")));

    let balances = block_on(user_balances(&Recorded::answering(BALANCES), &query)).unwrap();
    assert_eq!((balances.rows[0].wallet_balance, balances.rows[0].price), (d("23.72469206"), d("1")));

    let positions = block_on(user_positions(&Recorded::answering(POSITIONS), &query)).unwrap();
    let position = &positions.rows[0].row;
    assert_eq!(
        (position.position_amt, position.leverage, position.margin_type.clone()),
        (d("20"), 10, MarginType::Isolated)
    );

    let rates = block_on(user_commission_rates(&Recorded::answering(RATES), &query, "BTCUSDT")).unwrap();
    assert_eq!(rates.rows[0].row.taker_commission_rate, d("0.0004"));

    let trades = block_on(user_trades(&Recorded::answering(TRADES), &query)).unwrap();
    let trade = &trades.rows[0];
    assert_eq!((trades.total, trade.trade_id, trade.side.clone()), (42, 698_759, Side::Sell));
    assert_eq!((trade.fee, trade.builder_fee, trade.total_quota), (d("-0.0781901"), d("0.0005"), d("15.63802")));

    let commissions = block_on(user_commissions(&Recorded::answering(COMMISSIONS), &query)).unwrap();
    assert_eq!((commissions.rows[0].order_id, commissions.rows[0].builder_fee), (25_851_813, d("0.0005")));

    let all = block_on(user_all_orders(&Recorded::answering(ALL_ORDERS), &query)).unwrap();
    assert_eq!((all.rows[0].row.price_rate, all.rows[0].address.as_str()), (Some(d("0.3")), "0x1234...abcd"));

    let approved = block_on(approved_users(&Recorded::answering(APPROVED), &query)).unwrap();
    assert_eq!((approved.rows[0].max_fee_rate, approved.page_size), (d("0.0001"), 100));
    assert_eq!(approved.rows[0].builder_name, "MyBuilder");
}

#[test]
fn users_left_out_are_listed_with_why() {
    let text = r#"{"total":0,"currentPage":1,"totalPages":0,"pageSize":50,"hasMore":false,"rows":[],
        "errors":[{"address":"0xab","errorMsg":"This user address is not bound to this builder."}]}"#;
    let page: Page<UserBalance> = serde_json::from_str(text).unwrap();
    assert_eq!(page.errors[0].error_msg, "This user address is not bound to this builder.");
}
