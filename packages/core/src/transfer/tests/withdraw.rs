//! A withdrawal's `Action` held to eth_account, its two signatures and its two routes, the Solana
//! message, the docs' answers for the account's reads, and the fee estimates recorded on
//! 2026-11-11.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const INFO: &str = include_str!("../../../fixtures/aster/transfer_withdraw_info.json");
const HISTORY: &str = include_str!("../../../fixtures/aster/transfer_history.json");
const CHAIN_FEE: &str = include_str!("../../../fixtures/aster/transfer_chain_estimate_fee.json");
const WEB_FEE: &str = include_str!("../../../fixtures/aster/transfer_estimate_withdraw_fee.json");
const WITHDRAWN: &str = include_str!("../../../fixtures/aster/transfer_withdrawn.json");

/// The `Action` for 10.5 USDT less a 0.5 fee to 0xD375... on BNB Smart Chain at user nonce
/// 1790000000000000: its hash and the main key's signature, made by eth_account 0.13.7
/// (`encode_typed_data` with the docs' `Action` types under `{Aster, "1", 56, 0x0}`, then
/// `Account.sign_message`) with the key keccak256("atlas aster main").
const ETH_ACCOUNT_HASH: &str = "0x79621dbbba218566a0768ddd1990adf4ac4dde558e42f2af3dd8f75483264503";
const ETH_ACCOUNT_SIGNATURE: &str = "0x741ddff459be410f7b595522e03058772f95e3ffb3ba4db7f2868da2f07e291707b929e4c5c81d89452c9a64880cecb3b74a93a84a4a5b1bf0f388f49b5143101b";

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn withdrawal() -> Withdrawal {
    Withdrawal {
        chain_id: 56,
        asset: "USDT".into(),
        amount: d("10.50"),
        fee: d("0.50"),
        receiver: "0xD37536B4329222cA26be1cf0BfFdE7550b663f96".parse().unwrap(),
        user_nonce: 1_790_000_000_000_000,
    }
}

#[test]
fn the_action_hashes_as_eth_account_does() {
    let action = withdrawal().action();
    assert_eq!(
        action.message.encode_type(),
        "Action(string type,address destination,string destination Chain,string token,string amount,string fee,\
         uint256 nonce,string aster chain)"
    );
    assert_eq!(crate::utils::hex::encode_prefixed(&action.hash()), ETH_ACCOUNT_HASH);
    assert_eq!((action.domain.name.as_str(), action.domain.chain_id), ("Aster", 56));
    let shown = action.to_json();
    assert_eq!((&shown["message"]["amount"], &shown["message"]["destination Chain"]), (&"10.5".into(), &"BSC".into()));
}

#[test]
fn a_withdrawal_the_venue_would_refuse_is_refused_first() {
    assert_eq!(withdrawal().fault(), None);
    assert!(Withdrawal { chain_id: 10, ..withdrawal() }.fault().is_some());
    assert!(Withdrawal { amount: Dec::ZERO, ..withdrawal() }.fault().is_some());
    assert!(Withdrawal { fee: d("10.5"), ..withdrawal() }.fault().is_some());
    assert_eq!(
        [1, 56, 42161, 131, 7].map(destination_chain),
        [Some("ETH"), Some("BSC"), Some("Arbitrum"), Some("SUI"), None]
    );
}

#[test]
fn both_routes_carry_the_fields_in_the_demos_order() {
    let signature: Signature = ETH_ACCOUNT_SIGNATURE.parse().unwrap();
    let chain = withdraw_call(&withdrawal(), &signature);
    assert_eq!(
        (chain.method, chain.host, chain.path, chain.signed),
        (Method::Post, Host::Chain, "/aster-chain/v3/perp/user-withdraw", false)
    );
    let keys: Vec<&str> = chain.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["chainId", "asset", "amount", "fee", "receiver", "userNonce", "signatureType", "userSignature"]);
    assert_eq!((chain.value("amount"), chain.value("fee")), (Some("10.5"), Some("0.5")));
    assert_eq!(chain.value("userSignature"), Some(ETH_ACCOUNT_SIGNATURE));
    let futures = withdraw_by_agent_call(&withdrawal(), &signature);
    assert_eq!((futures.host, futures.path, futures.signed), (Host::Futures, "/fapi/v3/aster/user-withdraw", true));
    let keys: Vec<&str> = futures.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["chainId", "asset", "amount", "fee", "receiver", "userNonce", "userSignature"]);
}

#[test]
fn the_solana_message_and_call() {
    let solana = SolanaWithdrawal {
        asset: "USDT".into(),
        amount: d("1.20"),
        fee: d("0.10"),
        receiver: "H7LqU4p4f8LDddADXDH9oFeoh3r7vhfJFf3XCEot8pkd".into(),
        user_nonce: 1_773_741_793_787_000,
    };
    // the docs' example message
    assert_eq!(
        solana_message(&solana),
        "PrimaryType=Withdraw,AsterChain=Mainnet,Destination=H7LqU4p4f8LDddADXDH9oFeoh3r7vhfJFf3XCEot8pkd,\
         DestinationChain=Solana,Token=USDT,Amount=1.2,Fee=0.1,Nonce=1773741793787000"
    );
    let transport = Recorded::answering(WITHDRAWN);
    let done = block_on(solana_withdraw_by_agent(&transport, &solana, "sig58")).unwrap();
    assert_eq!(done.withdraw_id, "987654321");
    let call = transport.only();
    assert_eq!(
        (call.path, call.signed, call.value("chainId")),
        ("/fapi/v3/aster/user-solana-withdraw", true, Some("101"))
    );
    assert_eq!(call.value("userSignature"), Some("sig58"));
}

#[test]
fn the_accounts_reads_on_the_docs_answers() {
    let transport = Recorded::answering(INFO);
    let info = block_on(withdraw_info(&transport)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.method, call.host, call.path, call.signed),
        (Method::Get, Host::Chain, "/aster-chain/v3/perp/user-withdraw-info", true)
    );
    assert_eq!((info.user_daily_limit, info.user_remaining_daily_limit), (d("10000"), d("9500")));
    let usdt = &info.balances["USDT"];
    let eth = &usdt.chain_balances["1"];
    assert_eq!(
        (usdt.perp_total_withdraw_amount, eth.chain_id, eth.perp_max_withdraw_amount, eth.withdraw_fee),
        (d("500"), 1, d("4500"), d("0.5"))
    );

    let transport = Recorded::answering(HISTORY);
    let rows = block_on(history(&transport, Some(1))).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.path, call.fields.clone()),
        ("/aster-chain/v3/perp/deposit-withdraw-history", vec![("chainId", "1".to_owned())])
    );
    assert_eq!(call.weight, 5);
    let row = &rows[0];
    assert_eq!(
        (row.id.as_str(), row.kind.as_str(), row.amount, row.chain_id, row.account_type.as_str()),
        ("12345", "WITHDRAW", d("100"), Some(1), "perp")
    );
}

#[test]
fn the_fee_estimates_read_as_written() {
    let transport = Recorded::answering(CHAIN_FEE);
    let chain = block_on(estimate_fee(&transport, 56, "USDT")).unwrap();
    let call = transport.only();
    assert_eq!((call.host, call.path, call.signed), (Host::Chain, "/aster-chain/v3/withdraw/estimateFee", false));
    assert_eq!(call.fields, vec![("chainId", "56".to_owned()), ("asset", "USDT".to_owned())]);
    assert_eq!(
        (chain.gas_cost, chain.token_price, chain.gas_limit, chain.gas_price),
        (Some(d("0.11")), Some(d("0.999")), Some(d("200000")), None)
    );

    let transport = Recorded::answering(WEB_FEE);
    let web = block_on(web_estimate_fee(&transport, 56, "EVM", "USDT", "perp")).unwrap();
    let call = transport.only();
    assert_eq!((call.host, call.path), (Host::Web, "/bapi/futures/v1/public/future/aster/estimate-withdraw-fee"));
    let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["chainId", "network", "currency", "accountType"]);
    assert_eq!((web.gas_cost, web.token_price, web.native_price), (Some(d("0.11")), Some(d("0.999")), None));
}

#[cfg(feature = "sign")]
mod signed {
    use super::*;
    use crate::test_support::{Wire, agent, main_key, signing_client};
    use crate::utils::sign::{LocalWallet, keccak256, message, recover};

    #[test]
    fn the_main_wallets_action_signature_is_eth_accounts() {
        let signature = main_key().sign_hash(&withdrawal().action().hash()).unwrap();
        assert_eq!(signature.to_hex(), ETH_ACCOUNT_SIGNATURE);
    }

    #[test]
    fn through_aster_chain_the_main_wallet_signs_both() {
        let client = signing_client(Wire::always(WITHDRAWN));
        let done = block_on(withdraw(&client, &LocalWallet(main_key()), &withdrawal())).unwrap();
        assert_eq!((done.withdraw_id.as_str(), done.hash.as_str()), ("987654321", "0xabc123..."));
        let bodies = client.http().bodies();
        assert_eq!(bodies[0].0, "https://chainapi.asterdex.com/aster-chain/v3/perp/user-withdraw");
        let (msg, signature) = bodies[0].1.rsplit_once("&signature=").unwrap();
        assert_eq!(
            msg,
            format!(
                "chainId=56&asset=USDT&amount=10.5&fee=0.5&receiver=0xD37536B4329222cA26be1cf0BfFdE7550b663f96\
                 &userNonce=1790000000000000&signatureType=EOA&userSignature={ETH_ACCOUNT_SIGNATURE}\
                 &nonce=1790000000000000&user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96"
            )
        );
        assert_eq!(recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap(), main_key().address());
    }

    #[test]
    fn through_the_futures_api_the_agent_signs_the_call() {
        let client = signing_client(Wire::always(WITHDRAWN));
        block_on(withdraw_by_agent(&client, &LocalWallet(main_key()), &withdrawal())).unwrap();
        let bodies = client.http().bodies();
        assert_eq!(bodies[0].0, "https://fapi.asterdex.com/fapi/v3/aster/user-withdraw");
        let (msg, signature) = bodies[0].1.rsplit_once("&signature=").unwrap();
        assert!(msg.contains(&format!("&userSignature={ETH_ACCOUNT_SIGNATURE}&nonce=")), "{msg}");
        assert_eq!(recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap(), agent().signer());
    }

    #[test]
    fn a_wallet_that_is_not_the_main_one_or_a_bad_withdrawal_sends_nothing() {
        let client = signing_client(Wire::always(WITHDRAWN));
        let other = LocalWallet(crate::utils::sign::PrivateKey::from_bytes(&keccak256(b"someone else")).unwrap());
        // signs as itself: the Action is valid for it, but it is not the receiver's account
        // wallet; Aster decides that, so this only checks a mismatched signer is caught
        struct Claims(LocalWallet);
        impl Wallet for Claims {
            fn address(&self) -> Address {
                main_key().address()
            }
            fn sign_typed_data(
                &self,
                data: &TypedData,
            ) -> impl std::future::Future<Output = Result<Signature, crate::utils::sign::WalletError>> {
                self.0.sign_typed_data(data)
            }
        }
        let error = block_on(withdraw(&client, &Claims(other), &withdrawal())).unwrap_err();
        assert!(matches!(error, TransferError::Wallet(_)), "{error:?}");
        let error = block_on(withdraw(&client, &LocalWallet(main_key()), &Withdrawal { chain_id: 10, ..withdrawal() }));
        assert!(matches!(error, Err(TransferError::Invalid(_))));
        assert!(client.http().sent().is_empty());
    }
}
