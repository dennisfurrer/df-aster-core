//! Main wallet actions match globe.li's approval byte for byte (vectors/aster_web.json) and
//! follow Aster's demo scheme for the rest; registration signs its documented string; every
//! signature recovers to the wallet, and a wallet that is not the account is refused before
//! anything is sent. The migration history reads from the docs' example answer.

use super::*;
use crate::test_support::{Recorded, block_on};

const MIGRATE: &str = include_str!("../../../fixtures/aster/account_migrate_history.json");

#[cfg(feature = "sign")]
mod signing {
    use super::*;

    fn vectors() -> serde_json::Value {
        serde_json::from_str(include_str!("../../../vectors/aster_web.json")).unwrap()
    }
    use crate::client::Client;
    use crate::test_support::{Wire, agent, clock, main_key};
    use crate::utils::sign::{LocalWallet, PrivateKey, Wallet, WalletError, keccak256, recover};

    fn globe_li_approval() -> MainAction {
        let v = vectors();
        let message = &v["approve"]["typedData"]["message"];
        let approval = ApproveAgent {
            agent_name: "globeli".into(),
            ..ApproveAgent::perps_only("", agent().signer(), message["Expired"].as_u64().unwrap())
        };
        MainAction::approve_agent(main_key().address(), message["Nonce"].as_u64().unwrap(), &approval)
    }

    /// A wallet that says it is one address and signs as another.
    struct Impostor {
        claims: crate::utils::Address,
        key: PrivateKey,
    }

    impl Wallet for Impostor {
        fn address(&self) -> crate::utils::Address {
            self.claims
        }

        async fn sign_typed_data(
            &self,
            data: &crate::utils::sign::TypedData,
        ) -> Result<crate::utils::sign::Signature, WalletError> {
            self.key.sign_hash(&data.hash()).map_err(|error| WalletError::Declined(error.to_string()))
        }
    }

    fn impostor() -> Impostor {
        Impostor { claims: main_key().address(), key: PrivateKey::from_bytes(&keccak256(b"someone else")).unwrap() }
    }

    #[test]
    fn approve_agent_matches_globe_li_byte_for_byte() {
        let v = vectors();
        let approve = &v["approve"];
        let action = globe_li_approval();
        let data = action.typed_data();
        let shown = data.to_json();
        let given = &approve["typedData"];
        assert_eq!(shown["types"]["ApproveAgent"], given["types"]["ApproveAgent"]);
        assert_eq!((&shown["message"], &shown["domain"]), (&given["message"], &given["domain"]));
        assert_eq!(crate::utils::hex::encode_prefixed(&data.hash()), approve["signingHash"].as_str().unwrap());
        let signature = main_key().sign_hash(&data.hash()).unwrap();
        assert_eq!(signature.to_string(), approve["signature"].as_str().unwrap());

        // Sent through a client: the URL is globe.li's, character for character.
        let client = Client::reader(Wire::always(r#"{"code":200,"msg":"success"}"#), clock);
        let _: crate::utils::Done = block_on(read(&client, action.call(&signature))).unwrap();
        let sent = client.http().sent();
        assert_eq!(sent[0].url, approve["request"]["url"].as_str().unwrap());
        assert_eq!(sent[0].method, Method::Post);
        assert!(sent[0].body.is_empty());
    }

    #[test]
    fn a_main_action_is_sent_once_its_signature_recovers() {
        let action = globe_li_approval();
        let transport = Recorded::answering(r#"{"code":200,"msg":"success"}"#);
        let wallet = LocalWallet(main_key());
        let _: crate::utils::Done = block_on(send_main_action(&transport, &wallet, &action)).unwrap();
        let call = transport.only();
        assert!(!call.signed && call.in_query);
        let signature: crate::utils::sign::Signature = call.value("signature").unwrap().parse().unwrap();
        assert_eq!(recover(&action.typed_data().hash(), &signature).unwrap(), main_key().address());
        assert_eq!(call.value("signatureChainId"), Some("56"));

        let refused = Recorded::answering("{}");
        let error = block_on(send_main_action::<crate::utils::Done>(&refused, &impostor(), &action)).unwrap_err();
        assert!(matches!(error, WalletCallError::Wallet(WalletError::WrongSigner { .. })), "{error:?}");
        let agent_wallet = LocalWallet(agent().key);
        let error = block_on(send_main_action::<crate::utils::Done>(&refused, &agent_wallet, &action)).unwrap_err();
        assert!(matches!(error, WalletCallError::Wallet(WalletError::WrongSigner { .. })), "{error:?}");
        assert!(refused.sent().is_empty());
    }

    #[test]
    fn registration_signs_its_string_as_the_demo_does() {
        let approval = ApproveAgent {
            ip_whitelist: "192.168.1.1 10.0.0.0/24".into(),
            can_withdraw: true,
            ..ApproveAgent::perps_only("bot", agent().signer(), 1_792_592_000_000)
        };
        let register = RegisterAgent::new(main_key().address(), 1_790_000_000_000_000, approval, Some("REF1".into()));
        assert_eq!(
            register.msg(),
            "user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&nonce=1790000000000000&agentName=bot\
             &agentAddress=0xe0a5193404A0f6d94c087169c539eB0180FCf828&expired=1792592000000&signatureChainId=56\
             &canSpotTrade=false&canPerpTrade=true&canWithdraw=true&ipWhitelist=192.168.1.1 10.0.0.0/24"
        );
        assert_eq!(register.typed_data().domain.chain_id, 1666);
        let bare =
            RegisterAgent::new(main_key().address(), 1, ApproveAgent::perps_only("bot", agent().signer(), 2), None);
        assert!(bare.msg().ends_with("&canWithdraw=false"));
        let call = bare.call(&main_key().sign_hash(&bare.typed_data().hash()).unwrap());
        assert!(call.value("ipWhitelist").is_none() && call.value("agentCode").is_none());
        let transport = Recorded::answering(r#"{"code":200,"msg":"success"}"#);
        block_on(register_and_approve_agent(&transport, &LocalWallet(main_key()), &register)).unwrap();
        let call = transport.only();
        assert_eq!(
            (call.method, call.path, call.signed, call.weight),
            (Method::Post, "/fapi/v3/registerAndApproveAgent", false, 50)
        );
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "user",
                "nonce",
                "agentName",
                "agentAddress",
                "expired",
                "signatureChainId",
                "signature",
                "canSpotTrade",
                "canPerpTrade",
                "canWithdraw",
                "ipWhitelist",
                "agentCode"
            ]
        );
        let signature: crate::utils::sign::Signature = call.value("signature").unwrap().parse().unwrap();
        assert_eq!(recover(&register.typed_data().hash(), &signature).unwrap(), main_key().address());
        let refused = Recorded::answering("{}");
        assert!(block_on(register_and_approve_agent(&refused, &impostor(), &register)).is_err());
        assert!(refused.sent().is_empty());
    }

    #[test]
    fn a_migration_signs_user_and_nonce() {
        let migrate = MigrateUser { user: main_key().address(), nonce: 5 };
        assert_eq!(migrate.msg(), "user=0xD37536B4329222cA26be1cf0BfFdE7550b663f96&nonce=5");
        let transport = Recorded::answering(r#"{"batchId":"b1"}"#);
        let migrated = block_on(migrate_user(&transport, &LocalWallet(main_key()), &migrate)).unwrap();
        assert_eq!(migrated.batch_id.as_deref(), Some("b1"));
        let call = transport.only();
        assert_eq!(
            (call.method, call.path, call.signed, call.uses_query()),
            (Method::Post, "/fapi/v3/asset/migrateUser", false, false)
        );
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["user", "nonce", "signature"]);
        let signature: crate::utils::sign::Signature = call.value("signature").unwrap().parse().unwrap();
        assert_eq!(recover(&migrate.typed_data().hash(), &signature).unwrap(), main_key().address());
        let refused = Recorded::answering("{}");
        assert!(block_on(migrate_user(&refused, &impostor(), &migrate)).is_err());
        assert!(refused.sent().is_empty());
    }

    #[test]
    fn a_fresh_agent_is_a_new_key() {
        let first = new_agent().unwrap();
        assert_ne!(first.address(), new_agent().unwrap().address());
    }
}

#[test]
fn each_action_is_typed_as_the_demo_types_it() {
    let user: crate::utils::Address = "0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e".parse().unwrap();
    let other: crate::utils::Address = "0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0".parse().unwrap();
    let rate = Dec::parse("0.00001").unwrap();
    let cases: [(MainAction, &str, (Method, &str)); 5] = [
        (
            MainAction::update_agent(user, 1, other, "1.2.3.4", false, true, false),
            "UpdateAgent(string AgentAddress,string IpWhitelist,bool CanSpotTrade,bool CanPerpTrade,bool CanWithdraw,\
             string AsterChain,string User,uint256 Nonce)",
            (Method::Post, "/fapi/v3/updateAgent"),
        ),
        (
            MainAction::delete_agent(user, 1, other),
            "DelAgent(string AgentAddress,string AsterChain,string User,uint256 Nonce)",
            (Method::Delete, "/fapi/v3/agent"),
        ),
        (
            MainAction::approve_builder(user, 1, other, rate, "desk"),
            "ApproveBuilder(string Builder,string MaxFeeRate,string BuilderName,string AsterChain,string User,\
             uint256 Nonce)",
            (Method::Post, "/fapi/v3/approveBuilder"),
        ),
        (
            MainAction::update_builder(user, 1, other, rate),
            "UpdateBuilder(string Builder,string MaxFeeRate,string AsterChain,string User,uint256 Nonce)",
            (Method::Post, "/fapi/v3/updateBuilder"),
        ),
        (
            MainAction::delete_builder(user, 1, other),
            "DelBuilder(string Builder,string AsterChain,string User,uint256 Nonce)",
            (Method::Delete, "/fapi/v3/builder"),
        ),
    ];
    let signature: crate::utils::sign::Signature =
        format!("0x{}{}1b", "11".repeat(32), "22".repeat(32)).parse().unwrap();
    for (action, type_string, (method, path)) in cases {
        assert_eq!(action.typed_data().message.encode_type(), type_string);
        assert_eq!(action.typed_data().domain.chain_id, 56);
        let call = action.call(&signature);
        assert_eq!((call.method, call.path, call.in_query, call.signed), (method, path, true, false));
        let keys: Vec<&str> = call.fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(&keys[keys.len() - 5..], ["asterChain", "user", "nonce", "signature", "signatureChainId"]);
    }
    let builder = MainAction::approve_builder(user, 1, other, rate, "desk").call(&signature);
    assert_eq!(builder.value("maxFeeRate"), Some("0.00001"));
    assert_eq!(builder.value("builder"), Some("0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0"));
    assert_eq!(agent_expiry(1_000, AGENT_TTL_MS), 1_000 + 2_592_000_000);
}

#[test]
fn lists_and_the_migration_history_are_the_agents_reads() {
    let transport = Recorded::answering("[]");
    block_on(agents(&transport)).unwrap();
    assert_eq!(
        (transport.only().method, transport.only().path, transport.only().signed),
        (Method::Get, "/fapi/v3/agent", true)
    );
    let transport = Recorded::answering("[]");
    block_on(builders(&transport)).unwrap();
    assert_eq!((transport.only().path, transport.only().signed), ("/fapi/v3/builder", true));

    let transport = Recorded::answering(MIGRATE);
    let history = block_on(migrate_history(&transport, "a1B2c3D4e5F6g7H8i9J0k1")).unwrap();
    let call = transport.only();
    assert_eq!((call.path, call.weight, call.signed), ("/fapi/v3/asset/migrateUser/history", 50, true));
    assert_eq!(call.fields, vec![("batchId", "a1B2c3D4e5F6g7H8i9J0k1".to_owned())]);
    assert_eq!((history.total_count, history.success_count, history.init_count), (2, 1, 1));
    assert!(history.details[0].is_done() && !history.details[1].is_done());
    assert_eq!((history.details[0].tran_id, history.details[1].tran_id), (Some(9_876_543_210), None));
    assert_eq!(history.details[1].from_status, "");
    let migrated: Migrated = serde_json::from_str(r#"{"batchId":"b1"}"#).unwrap();
    assert_eq!(migrated.batch_id.as_deref(), Some("b1"));
    assert_eq!(serde_json::from_str::<Migrated>("{}").unwrap(), Migrated::default());
}
