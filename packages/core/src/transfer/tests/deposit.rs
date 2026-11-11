//! Deposit calldata byte for byte, a deposit's plan for a token and for the chain's coin, and the
//! asset lists recorded from the web app on 2026-11-11.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};
use crate::utils::hex;

const DEPOSIT_ASSETS: &str = include_str!("../../../fixtures/aster/transfer_deposit_assets.json");
const WITHDRAW_ASSETS: &str = include_str!("../../../fixtures/aster/transfer_withdraw_assets.json");
const DEPOSIT_ADDRESS: &str = include_str!("../../../fixtures/aster/transfer_deposit_address.json");

fn address(text: &str) -> Address {
    text.parse().unwrap()
}

/// USDT on BNB Smart Chain, 18 decimals.
const USDT: &str = "0x55d398326f99059fF775485246999027B3197955";
const USER: &str = "0xD37536B4329222cA26be1cf0BfFdE7550b663f96";

#[test]
fn selectors_are_the_known_ones() {
    assert_eq!(selector("approve(address,uint256)"), [0x09, 0x5e, 0xa7, 0xb3]);
    assert_eq!(selector("transfer(address,uint256)"), [0xa9, 0x05, 0x9c, 0xbb]);
}

#[test]
fn calldata_is_the_selector_then_one_word_each() {
    let vault = vault(56).unwrap().address;
    let data = deposit_for_calldata(address(USDT), address(USER), 100_000_000_000_000_000_000, BROKER_SPOT);
    assert_eq!(data.len(), 4 + 4 * 32);
    assert_eq!(
        hex::encode(&data[4..]),
        [
            "00000000000000000000000055d398326f99059ff775485246999027b3197955",
            "000000000000000000000000d37536b4329222ca26be1cf0bffde7550b663f96",
            "0000000000000000000000000000000000000000000000056bc75e2d63100000",
            "00000000000000000000000000000000000000000000000000000000000003e8",
        ]
        .concat()
    );
    assert_eq!(data[..4], selector("depositFor(address,address,uint256,uint256)"));
    let approve = approve_calldata(vault, 1);
    assert_eq!(
        hex::encode_prefixed(&approve),
        "0x095ea7b3000000000000000000000000128463a60784c4d3f46c23af3f65ed859ba87974\
         0000000000000000000000000000000000000000000000000000000000000001"
    );
}

#[test]
fn a_token_deposit_approves_then_deposits_and_a_coin_deposit_sends_value() {
    let token = Deposit {
        chain_id: 56,
        currency: address(USDT),
        decimals: 18,
        for_address: address(USER),
        amount: Dec::parse("100").unwrap(),
        to_spot: false,
    };
    let plan = token.plan().unwrap();
    let vault = vault(56).unwrap();
    assert_eq!(plan.units, 100_000_000_000_000_000_000);
    let approve = plan.approve.unwrap();
    assert_eq!((approve.to, approve.value, approve.chain_id), (address(USDT), 0, 56));
    assert_eq!(approve.data, approve_calldata(vault.address, plan.units));
    assert_eq!((plan.deposit.to, plan.deposit.value), (vault.address, 0));
    assert_eq!(plan.deposit.data, deposit_for_calldata(address(USDT), address(USER), plan.units, BROKER_FUTURES));
    assert!(plan.deposit.data_hex().starts_with("0x"));

    let coin = Deposit {
        currency: NATIVE_TOKEN,
        amount: Dec::parse("0.5").unwrap(),
        chain_id: 1,
        to_spot: true,
        ..token.clone()
    };
    let plan = coin.plan().unwrap();
    assert_eq!((plan.approve, plan.deposit.value), (None, 500_000_000_000_000_000));
    assert_eq!(plan.deposit.to, vault_address(1));
    assert_eq!(plan.deposit.data, deposit_for_calldata(NATIVE_TOKEN, address(USER), plan.units, BROKER_SPOT));

    let fine = Deposit { decimals: 6, amount: Dec::parse("1.0000001").unwrap(), ..token.clone() };
    assert!(matches!(fine.plan(), Err(BadDeposit::TooFine { .. })));
    assert!(matches!(Deposit { chain_id: 10, ..token.clone() }.plan(), Err(BadDeposit::NoVault(10))));
    assert!(matches!(Deposit { amount: Dec::ZERO, ..token }.plan(), Err(BadDeposit::NotPositive(_))));
}

fn vault_address(chain_id: u64) -> Address {
    vault(chain_id).unwrap().address
}

#[test]
fn the_vaults_are_the_documented_ones() {
    assert_eq!(VAULTS.map(|vault| (vault.chain_id, vault.name)), [(1, "ETH"), (56, "BSC"), (42161, "Arbitrum")]);
    assert_eq!(vault_address(1).to_checksum(), "0x604DD02d620633Ae427888d41bfd15e38483736E");
    assert_eq!(vault_address(42161).to_checksum(), "0x9E36CB86a159d479cEd94Fa05036f235Ac40E1d5");
    assert_eq!(NATIVE_TOKEN.to_checksum(), "0xfdAE1bA7C826aBDc4c99903c8056f82a1A04a615");
    assert_eq!(vault(10), None);
}

#[test]
fn the_asset_lists_read_from_the_web_app() {
    let transport = Recorded::answering(DEPOSIT_ASSETS);
    let assets = block_on(deposit_assets(&transport, &[56], &["EVM"], "perp")).unwrap();
    let call = transport.only();
    assert_eq!((call.method, call.host, call.signed), (Method::Get, Host::Web, false));
    assert_eq!(call.path, "/bapi/futures/v1/public/future/aster/deposit/assets");
    assert_eq!(
        call.fields,
        vec![("chainIds", "56".to_owned()), ("networks", "EVM".to_owned()), ("accountType", "perp".to_owned())]
    );
    assert_eq!(assets.len(), 24);
    let usdt = &assets[0];
    assert_eq!((usdt.name.as_str(), usdt.decimals, usdt.chain_id, usdt.rank), ("USDT", 18, 56, 1));
    assert_eq!(usdt.evm_address(), Some(address(USDT)));
    assert_eq!(usdt.min_deposit, Some(Dec::ZERO));
    let bnb = assets.iter().find(|asset| asset.name == "BNB").unwrap();
    assert!(bnb.is_native && bnb.evm_address() == Some(NATIVE_TOKEN));

    let transport = Recorded::answering(WITHDRAW_ASSETS);
    let assets = block_on(withdraw_assets(&transport, &[56, 1], &[], "perp")).unwrap();
    let call = transport.only();
    assert_eq!(call.fields, vec![("chainIds", "56,1".to_owned()), ("accountType", "perp".to_owned())]);
    assert_eq!((assets.len(), assets[0].withdraw_type.as_deref()), (27, Some("autoWithdraw")));
}

#[test]
fn the_deposit_address_is_read_with_the_agent() {
    let transport = Recorded::answering(DEPOSIT_ADDRESS);
    let found = block_on(user_deposit_address(&transport, Some("SUI"))).unwrap();
    assert_eq!(found.network, "SUI");
    let call = transport.only();
    assert_eq!((call.host, call.path, call.signed), (Host::Chain, "/aster-chain/v3/spot/user-deposit-address", true));
    assert_eq!(call.fields, vec![("network", "SUI".to_owned())]);
}
