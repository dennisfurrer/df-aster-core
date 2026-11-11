//! An order fitted to its symbol's grid, placed, listed and cancelled, every call signed by the
//! agent at a fresh nonce.

use df_aster_core::account::agents::{ApproveAgent, MainAction, send_main_action};
use df_aster_core::account::history::open_orders;
use df_aster_core::markets::info::{Markets, exchange_info};
use df_aster_core::trade::manage::{OrderRef, cancel};
use df_aster_core::trade::orders::{NewOrder, place};
use df_aster_core::utils::sign::{Agent, LocalWallet, PrivateKey, keccak256};
use df_aster_core::utils::{Done, OrderStatus, Round, Side, TimeInForce};
use df_aster_core::{Client, Dec};

use crate::venue::{Venue, block_on, clock};

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[test]
fn an_order_is_fitted_placed_listed_and_cancelled() {
    let venue = Venue::default();
    let wallet = LocalWallet(PrivateKey::from_bytes(&keccak256(b"mocked trader")).unwrap());
    let agent = Agent::new(wallet.0.address(), PrivateKey::from_bytes(&keccak256(b"mocked trader agent")).unwrap());
    let client = Client::new(&venue, clock, agent.clone());
    let approval = ApproveAgent::perps_only("mocked", agent.signer(), 1_800_000_000_000);
    let action = MainAction::approve_agent(agent.user, client.next_nonce(), &approval);
    let _: Done = block_on(send_main_action(&client, &wallet, &action)).unwrap();

    let markets = Markets::from(block_on(exchange_info(&client)).unwrap());
    let btc = markets.get("BTCUSDT").unwrap();
    let order = NewOrder::limit("BTCUSDT", Side::Buy, d("0.00123"), d("60000.123"), TimeInForce::Gtc)
        .fit(&btc.grid(), Round::Down)
        .unwrap();
    let placed = block_on(place(&client, &order)).unwrap();
    assert_eq!(placed.status, OrderStatus::New);
    assert_eq!(placed.price, btc.grid().price(d("60000.123"), Round::Down).unwrap());

    let open = block_on(open_orders(&client, Some("BTCUSDT"))).unwrap();
    assert_eq!(open.len(), 1);
    let gone = block_on(cancel(&client, "BTCUSDT", &OrderRef::Id(placed.order_id))).unwrap();
    assert_eq!(gone.status, OrderStatus::Canceled);
    assert!(block_on(open_orders(&client, Some("BTCUSDT"))).unwrap().is_empty());
    let again = block_on(cancel(&client, "BTCUSDT", &OrderRef::Id(placed.order_id))).unwrap_err();
    assert_eq!(again.code(), Some(-2011));
}
