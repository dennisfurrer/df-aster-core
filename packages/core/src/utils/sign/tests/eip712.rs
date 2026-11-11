//! Type strings, the domain separator and the signing hash, held to what viem computes (the
//! globe.li vectors) and to the JSON a wallet is shown.

use super::*;

fn vectors() -> Value {
    serde_json::from_str(include_str!("../../../../vectors/aster_web.json")).unwrap()
}

fn approve(v: &Value) -> TypedData {
    let message = &v["approve"]["typedData"]["message"];
    let text = |key: &str| Field::String(message[key].as_str().unwrap().to_owned());
    let number = |key: &str| Field::Uint256(u128::from(message[key].as_u64().unwrap()));
    let flag = |key: &str| Field::Bool(message[key].as_bool().unwrap());
    TypedData::new(
        Domain::new("AsterSignTransaction", "1", 56),
        Struct::new("ApproveAgent")
            .field("AgentName", text("AgentName"))
            .field("AgentAddress", text("AgentAddress"))
            .field("IpWhitelist", text("IpWhitelist"))
            .field("Expired", number("Expired"))
            .field("CanSpotTrade", flag("CanSpotTrade"))
            .field("CanPerpTrade", flag("CanPerpTrade"))
            .field("CanWithdraw", flag("CanWithdraw"))
            .field("AsterChain", text("AsterChain"))
            .field("User", text("User"))
            .field("Nonce", number("Nonce")),
    )
}

#[test]
fn approve_agent_hashes_as_viem_does() {
    let v = vectors();
    let data = approve(&v);
    assert_eq!(
        data.message.encode_type(),
        "ApproveAgent(string AgentName,string AgentAddress,string IpWhitelist,uint256 Expired,bool CanSpotTrade,\
         bool CanPerpTrade,bool CanWithdraw,string AsterChain,string User,uint256 Nonce)"
    );
    assert_eq!(crate::utils::hex::encode_prefixed(&data.hash()), v["approve"]["signingHash"].as_str().unwrap());
    // The wallet is shown the same types, message and domain viem was given.
    let shown = data.to_json();
    let given = &v["approve"]["typedData"];
    assert_eq!(shown["types"]["ApproveAgent"], given["types"]["ApproveAgent"]);
    assert_eq!(shown["message"], given["message"]);
    assert_eq!(shown["domain"], given["domain"]);
    assert_eq!(shown["primaryType"], given["primaryType"]);
}

#[test]
fn message_hashes_as_viem_does() {
    let v = vectors();
    for trade in v["trades"].as_array().unwrap() {
        let data = crate::utils::sign::message(trade["msg"].as_str().unwrap());
        assert_eq!(
            crate::utils::hex::encode_prefixed(&data.hash()),
            trade["signingHash"].as_str().unwrap(),
            "{}",
            trade["name"]
        );
    }
}

#[test]
fn field_names_with_spaces_are_written_as_they_are() {
    let action = Struct::new("Action")
        .field("type", Field::String("Withdraw".into()))
        .field("destination", Field::Address(crate::utils::Address::ZERO))
        .field("destination Chain", Field::String("BSC".into()));
    assert_eq!(action.encode_type(), "Action(string type,address destination,string destination Chain)");
    let big = Field::Uint256(u128::from(u64::MAX));
    let data = TypedData::new(Domain::new("Aster", "1", 56), Struct::new("N").field("n", big));
    assert_eq!(data.to_json()["message"]["n"], "18446744073709551615");
}
