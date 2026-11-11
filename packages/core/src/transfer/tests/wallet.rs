//! Futures and spot through both APIs, and an asset exchange, on the docs' answers.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const TRANSFERRED: &str = include_str!("../../../fixtures/aster/transfer_wallet_transfer.json");
const EXCHANGED: &str = include_str!("../../../fixtures/aster/transfer_asset_exchange.json");

fn pairs(call: &Call) -> Vec<(&str, &str)> {
    call.fields.iter().map(|(key, value)| (*key, value.as_str())).collect()
}

#[test]
fn futures_to_spot_through_the_futures_api() {
    let transport = Recorded::answering(TRANSFERRED);
    let done = block_on(wallet_transfer(&transport, "USDT", Dec::parse("1.50").unwrap(), "t-1", Direction::FutureSpot))
        .unwrap();
    assert_eq!((done.tran_id.as_str(), done.status.as_str()), ("21841", "SUCCESS"));
    let call = transport.only();
    assert_eq!((call.method, call.host, call.path), (Method::Post, Host::Futures, "/fapi/v3/asset/wallet/transfer"));
    assert_eq!(
        pairs(&call),
        [("amount", "1.5"), ("asset", "USDT"), ("clientTranId", "t-1"), ("kindType", "FUTURE_SPOT")]
    );
    assert!(call.signed && !call.uses_query());
    assert_eq!(call.weight, 50);
}

#[test]
fn spot_to_futures_through_aster_chain() {
    let transport = Recorded::answering(TRANSFERRED);
    block_on(chain_wallet_transfer(&transport, "USDT", Dec::from_int(2), "w-1", Direction::SpotFuture)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.method, call.host, call.path),
        (Method::Post, Host::Chain, "/aster-chain/v3/perp/wallet/transfer")
    );
    assert_eq!(
        pairs(&call),
        [("asset", "USDT"), ("amount", "2"), ("clientTranId", "w-1"), ("kindType", "SPOT_FUTURE")]
    );
    assert!(call.signed);
    assert_eq!(call.weight, 5);
}

#[test]
fn an_asset_exchange_or_nothing() {
    let transport = Recorded::answering(EXCHANGED);
    let exchanged = block_on(asset_exchange(&transport)).unwrap().unwrap();
    assert_eq!((exchanged.account_id.as_str(), exchanged.asset.as_str()), ("123456", "USDT"));
    assert_eq!((exchanged.balance, exchanged.update_time), (Dec::parse("100.5").unwrap(), Some(1_774_077_572_319)));
    let call = transport.only();
    assert_eq!(
        (call.method, call.path, call.signed, call.fields.len()),
        (Method::Post, "/fapi/v3/assetExchange", true, 0)
    );
    assert_eq!(block_on(asset_exchange(&Recorded::answering(""))).unwrap(), None);
}
