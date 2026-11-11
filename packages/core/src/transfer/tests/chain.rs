//! Transfers to addresses signed by the main wallet and refused before signing when the venue
//! would refuse them, a batch's list as the venue reads it, and the docs' answers.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const ACCEPTED: &str = include_str!("../../../fixtures/aster/transfer_batch_accepted.json");
const HISTORY: &str = include_str!("../../../fixtures/aster/transfer_batch_history.json");
const STATUS: &str = include_str!("../../../fixtures/aster/transfer_account_status.json");

fn address(text: &str) -> Address {
    text.parse().unwrap()
}

fn item(to: &str, amount: &str) -> BatchItem {
    BatchItem {
        to_address: address(to),
        asset: "USDT".into(),
        amount: Dec::parse(amount).unwrap(),
        transfer_chain_id: None,
    }
}

const ID: &str = "batch-20261026-000000001";

#[test]
fn a_batch_carries_its_items_as_json_and_is_checked_first() {
    let items = [
        item("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "1.50"),
        item("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359", "2"),
    ];
    let call = batch_transfer_call(ID, &items).unwrap();
    assert_eq!(
        (call.method, call.host, call.path, call.signed),
        (Method::Post, Host::Chain, "/aster-chain/v3/batch-transfer", false)
    );
    assert_eq!(call.value("clientTranId"), Some(ID));
    assert_eq!(
        call.value("items"),
        Some(
            r#"[{"toAddress":"0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed","asset":"USDT","amount":"1.5"},{"toAddress":"0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359","asset":"USDT","amount":"2"}]"#
        )
    );
    assert_eq!(call.weight, 50);
    assert!(batch_transfer_call("short", &items).is_err());
    assert!(batch_transfer_call(ID, &[]).is_err());
    let eleven: Vec<BatchItem> = (0..11).map(|_| items[0].clone()).collect();
    assert!(batch_transfer_call(ID, &eleven).is_err());
    assert!(batch_transfer_call(ID, &[items[0].clone(), items[0].clone()]).unwrap_err().contains("twice"));
    assert!(batch_transfer_call(ID, &[item("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "0.000000001")]).is_err());
    assert!(batch_transfer_call(ID, &[item("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "0")]).is_err());
}

#[test]
fn the_docs_batch_answers_read() {
    let accepted: BatchAccepted = serde_json::from_str(ACCEPTED).unwrap();
    assert_eq!((accepted.client_tran_id.as_str(), accepted.status.as_str(), accepted.total_count), (ID, "I", 2));
    assert_eq!(accepted.items[1].transfer_id, "100002");
    let transport = Recorded::answering(HISTORY);
    let query = BatchQuery { client_tran_id: Some(ID.into()), limit: Some(20), ..BatchQuery::default() };
    let page = block_on(batch_history(&transport, &query)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.method, call.host, call.path, call.signed),
        (Method::Get, Host::Chain, "/aster-chain/v3/batch-transfer/history", true)
    );
    assert_eq!(call.fields, vec![("clientTranId", ID.to_owned()), ("limit", "20".to_owned())]);
    let row = &page.rows[0];
    assert_eq!((page.total, row.status.as_str(), row.success_count, row.fail_count), (1, "M", 1, 1));
    assert_eq!(row.details[0].success_amount, Some(Dec::parse("1.5").unwrap()));
    assert_eq!((row.details[1].success_amount, row.failed_items[0].transfer_id.as_str()), (None, "100002"));
}

#[test]
fn privacy_reads_and_changes_with_the_agent() {
    let transport = Recorded::answering(STATUS);
    assert_eq!(block_on(account_status(&transport)).unwrap().status, Privacy::Private);
    let read = transport.only();
    assert_eq!(
        (read.method, read.host, read.path, read.signed),
        (Method::Get, Host::Chain, "/aster-chain/v3/account/status", true)
    );
    let transport = Recorded::answering(STATUS);
    block_on(modify_account_status(&transport, Privacy::Public)).unwrap();
    let change = transport.only();
    assert_eq!((change.method, change.path, change.weight), (Method::Post, "/aster-chain/v3/account/modify-status", 5));
    assert_eq!(change.fields, vec![("status", "PUBLIC".to_owned())]);
}

#[cfg(feature = "sign")]
mod signed {
    use super::*;
    use crate::test_support::{Wire, block_on, main_key, signing_client};
    use crate::utils::sign::{LocalWallet, Signature, TypedData, WalletError, keccak256, message, recover};

    const TRANSFERRED: &str = include_str!("../../../fixtures/aster/transfer_chain_transfer.json");

    /// A wallet that claims one address and signs with another key.
    struct Impostor(LocalWallet);

    impl Wallet for Impostor {
        fn address(&self) -> Address {
            main_key().address()
        }

        fn sign_typed_data(
            &self,
            data: &TypedData,
        ) -> impl std::future::Future<Output = Result<Signature, WalletError>> {
            self.0.sign_typed_data(data)
        }
    }

    fn transfer() -> ChainTransfer {
        ChainTransfer {
            asset: "USDT".into(),
            amount: Dec::from_int(10),
            to: address("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"),
            client_tran_id: None,
        }
    }

    #[test]
    fn a_transfer_to_an_address_is_signed_by_the_main_wallet() {
        let client = signing_client(Wire::always(TRANSFERRED));
        let done = block_on(transfer_to_address(&client, &LocalWallet(main_key()), &transfer())).unwrap();
        assert_eq!(
            (done.transfer_id.as_str(), done.amount, done.status.as_str()),
            ("123456789", Dec::from_int(10), "SUCCESS")
        );
        let bodies = client.http().bodies();
        assert_eq!(bodies[0].0, "https://chainapi.asterdex.com/aster-chain/v3/transfer");
        let (msg, signature) = bodies[0].1.rsplit_once("&signature=").unwrap();
        assert_eq!(
            msg,
            "asset=USDT&amount=10&toAddress=0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed&nonce=1790000000000000\
             &user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96"
        );
        assert!(!msg.contains("signer="), "no agent signs it");
        assert_eq!(recover(&message(msg).hash(), &signature.parse().unwrap()).unwrap(), main_key().address());
    }

    #[test]
    fn a_wallet_that_signs_as_someone_else_sends_nothing() {
        let client = signing_client(Wire::always(TRANSFERRED));
        let other = crate::utils::sign::PrivateKey::from_bytes(&keccak256(b"someone else")).unwrap();
        let error = block_on(transfer_to_address(&client, &Impostor(LocalWallet(other)), &transfer())).unwrap_err();
        assert!(matches!(error, TransferError::Wallet(WalletError::WrongSigner { .. })), "{error:?}");
        let items = [item("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "1")];
        let other = crate::utils::sign::PrivateKey::from_bytes(&keccak256(b"someone else")).unwrap();
        assert!(block_on(batch_transfer(&client, &Impostor(LocalWallet(other)), ID, &items)).is_err());
        assert!(client.http().sent().is_empty());
    }

    #[test]
    fn a_batch_is_signed_by_the_main_wallet_and_a_bad_one_is_never_signed() {
        let client = signing_client(Wire::always(ACCEPTED));
        let items = [item("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed", "1")];
        let accepted = block_on(batch_transfer(&client, &LocalWallet(main_key()), ID, &items)).unwrap();
        assert_eq!(accepted.items.len(), 2);
        let body = &client.http().bodies()[0].1;
        assert!(body.starts_with("clientTranId=batch-20261026-000000001&items=%5B%7B%22toAddress%22"), "{body}");
        let mut zero = transfer();
        zero.amount = Dec::ZERO;
        assert!(matches!(
            block_on(transfer_to_address(&client, &LocalWallet(main_key()), &zero)),
            Err(TransferError::Invalid(_))
        ));
        assert_eq!(client.http().sent().len(), 1);
    }
}
