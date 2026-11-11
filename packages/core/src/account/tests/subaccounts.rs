//! The list is an agent's read; binding and creating carry the child's and the main wallet's
//! signatures over their documented strings, updating the main wallet's alone; each recovers to
//! its wallet, and a wallet that signs as someone else is refused before anything is sent.
//! Answers from the docs.

use super::*;
use crate::test_support::{Recorded, block_on};

const LIST: &str = include_str!("../../../fixtures/aster/account_sub_accounts.json");

#[test]
fn the_list_reads_main_and_sub_accounts() {
    let transport = Recorded::answering(LIST);
    let rows = block_on(sub_accounts(&transport)).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.signed, call.weight), ("/fapi/v3/getSubAccountList", true, 5));
    assert!(call.fields.is_empty());
    assert_eq!(
        (rows[0].account_id, rows[0].sub_account_name.as_str(), rows[0].parent_account),
        (123_456, "trading-desk-1", false)
    );
    assert!(rows[1].parent_account);
}

#[cfg(feature = "sign")]
mod signing {
    use super::*;
    use crate::test_support::{agent, main_key};
    use crate::utils::sign::{LocalWallet, PrivateKey, Signature, Wallet, keccak256, message, recover};

    fn child() -> LocalWallet {
        LocalWallet(PrivateKey::from_bytes(&keccak256(b"aster child")).unwrap())
    }

    fn text(call: &Call) -> String {
        call.fields.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join("&")
    }

    fn recovered(msg: &str, signature: &str) -> Address {
        let signature: Signature = signature.parse().unwrap();
        recover(&message(msg).hash(), &signature).unwrap()
    }

    #[test]
    fn binding_carries_both_signatures() {
        let (main, child) = (LocalWallet(main_key()), child());
        let transport = Recorded::answering(r#"{"code":200,"msg":"success"}"#);
        block_on(bind_sub_account(&transport, &main, &child, "desk", 1_790_000_000_000_000)).unwrap();
        let call = transport.only();
        assert_eq!((call.path, call.signed, call.uses_query()), ("/fapi/v3/sub-accounts/bind", false, false));
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["childAddress", "name", "nonce", "user", "childSignature", "signature"]);
        let child_msg = format!(
            "childAddress={}&name=desk&nonce=1790000000000000&user={}",
            child.address().to_checksum(),
            main.address().to_checksum()
        );
        assert_eq!(recovered(&child_msg, call.value("childSignature").unwrap()), child.address());
        let main_msg = format!("{child_msg}&childSignature={}", call.value("childSignature").unwrap());
        assert_eq!(recovered(&main_msg, call.value("signature").unwrap()), main.address());
    }

    #[test]
    fn creating_names_the_signer_and_carries_both_signatures() {
        let (main, child) = (LocalWallet(main_key()), child());
        let signer = agent().signer();
        let transport = Recorded::answering(r#"{"code":200,"msg":"success"}"#);
        block_on(create_sub_account(&transport, &main, &child, "desk 2", signer, 7)).unwrap();
        let call = transport.only();
        assert_eq!(call.path, "/fapi/v3/createSubAccount");
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["subSourceAddr", "subAccountName", "nonce", "user", "signer", "childSignature", "signature"]);
        let mut unsigned = call.clone();
        unsigned.fields.swap(0, 1);
        unsigned.fields.truncate(5);
        assert!(text(&unsigned).starts_with("subAccountName=desk 2&subSourceAddr="));
        assert_eq!(recovered(&text(&unsigned), call.value("childSignature").unwrap()), child.address());
        unsigned.fields.push(call.fields[5].clone());
        assert_eq!(recovered(&text(&unsigned), call.value("signature").unwrap()), main.address());
    }

    #[test]
    fn updating_is_the_main_wallets_alone() {
        let main = LocalWallet(main_key());
        let sub = child().address();
        let transport = Recorded::answering(r#"{"code":200,"msg":"success"}"#);
        let signer = agent().signer();
        block_on(update_sub_account(&transport, &main, sub, None, Some(SubAccountStatus::Frozen), signer, 9)).unwrap();
        let call = transport.only();
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["subSourceAddr", "nonce", "user", "signer", "status", "signature"]);
        let mut unsigned = call.clone();
        unsigned.fields.pop();
        assert!(text(&unsigned).ends_with("&status=FROZEN"));
        assert_eq!(recovered(&text(&unsigned), call.value("signature").unwrap()), main.address());
    }

    #[test]
    fn a_child_that_signs_as_someone_else_is_refused_before_sending() {
        struct Impostor(LocalWallet, Address);
        impl Wallet for Impostor {
            fn address(&self) -> Address {
                self.1
            }

            async fn sign_typed_data(
                &self,
                data: &crate::utils::sign::TypedData,
            ) -> Result<Signature, crate::utils::sign::WalletError> {
                self.0.sign_typed_data(data).await
            }
        }
        let impostor = Impostor(LocalWallet(main_key()), child().address());
        let transport = Recorded::answering("{}");
        let error = block_on(bind_sub_account(&transport, &LocalWallet(main_key()), &impostor, "x", 1)).unwrap_err();
        assert!(matches!(error, WalletCallError::Wallet(_)), "{error:?}");
        assert!(transport.sent().is_empty());
    }
}
