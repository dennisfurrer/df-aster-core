//! A new agent: refused until the main wallet approves it, then every signed read goes through;
//! a wallet that signs as someone else never reaches the venue.

use df_aster_core::account::agents::{
    AGENT_TTL_MS, ApproveAgent, MainAction, agent_expiry, new_agent, send_main_action,
};
use df_aster_core::account::holdings::balances;
use df_aster_core::utils::Done;
use df_aster_core::utils::sign::{Agent, LocalWallet, PrivateKey, keccak256};
use df_aster_core::{Client, Dec};

use crate::venue::{Venue, block_on, clock};

fn main_wallet() -> LocalWallet {
    LocalWallet(PrivateKey::from_bytes(&keccak256(b"mocked main")).unwrap())
}

#[test]
fn an_agent_works_once_the_main_wallet_approves_it() {
    let venue = Venue::default();
    let wallet = main_wallet();
    let user = wallet.0.address();
    let agent = Agent::new(user, new_agent().unwrap());
    let client = Client::new(&venue, clock, agent.clone());

    let before = block_on(balances(&client)).unwrap_err();
    assert!(before.is_unauthorized(), "{before:?}");

    let approval = ApproveAgent::perps_only("mocked", agent.signer(), agent_expiry(clock(), AGENT_TTL_MS));
    let action = MainAction::approve_agent(user, client.next_nonce(), &approval);
    let _: Done = block_on(send_main_action(&client, &wallet, &action)).unwrap();

    let held = block_on(balances(&client)).unwrap();
    assert_eq!(held[0].balance, Dec::from_int(100));
    assert_eq!(*venue.log.borrow(), ["GET /fapi/v3/balance", "POST /fapi/v3/approveAgent", "GET /fapi/v3/balance"]);
}

#[test]
fn a_wallet_that_signs_as_someone_else_sends_nothing() {
    let venue = Venue::default();
    let impostor = LocalWallet(PrivateKey::from_bytes(&keccak256(b"someone else")).unwrap());
    let user = main_wallet().0.address();
    let client = Client::reader(&venue, clock);
    let approval = ApproveAgent::perps_only("mocked", new_agent().unwrap().address(), 1_800_000_000_000);
    let action = MainAction::approve_agent(user, client.next_nonce(), &approval);
    assert!(block_on(send_main_action::<Done>(&client, &impostor, &action)).is_err());
    assert!(venue.log.borrow().is_empty());
}
