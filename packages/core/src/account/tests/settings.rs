//! Each setting is read and changed with the right call; answers from the docs.

use super::*;
use crate::client::Method;
use crate::test_support::{Recorded, block_on};

const MMP: &str = include_str!("../../../fixtures/aster/account_mmp.json");
const INVITEE: &str = include_str!("../../../fixtures/aster/account_invitee.json");
const SUCCESS: &str = r#"{"code":200,"msg":"success"}"#;

fn fields(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    pairs.iter().map(|(key, value)| (*key, (*value).to_owned())).collect()
}

#[test]
fn modes_read_and_change() {
    let transport = Recorded::answering(r#"{"dualSidePosition":true}"#);
    assert!(block_on(position_mode(&transport)).unwrap());
    assert_eq!(
        (transport.only().path, transport.only().weight, transport.only().signed),
        ("/fapi/v3/positionSide/dual", 30, true)
    );
    let transport = Recorded::answering(SUCCESS);
    block_on(set_position_mode(&transport, false)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.method, call.weight, call.fields.clone()),
        (Method::Post, 1, fields(&[("dualSidePosition", "false")]))
    );

    let transport = Recorded::answering(r#"{"stpMode":"EXPIRE_TAKER"}"#);
    assert_eq!(block_on(stp_mode(&transport)).unwrap(), StpMode::ExpireTaker);
    let transport = Recorded::answering(SUCCESS);
    block_on(set_stp_mode(&transport, &StpMode::ExpireBoth)).unwrap();
    assert_eq!(transport.only().fields, fields(&[("stpMode", "EXPIRE_BOTH")]));

    let transport = Recorded::answering(r#"{"multiAssetsMargin":"true"}"#);
    assert!(block_on(multi_assets_mode(&transport)).unwrap());
    let transport = Recorded::answering(SUCCESS);
    block_on(set_multi_assets_mode(&transport, true)).unwrap();
    let call = transport.only();
    assert_eq!(
        (call.path, call.signed, call.fields.clone()),
        ("/fapi/v3/multiAssetsMargin", true, fields(&[("multiAssetsMargin", "true")]))
    );
    let refused = Recorded::answering(r#"{"code":-4059,"msg":"No need to change position side."}"#);
    assert_eq!(block_on(set_position_mode(&refused, true)).unwrap_err().code(), Some(-4059));
}

#[test]
fn market_maker_protection_round_trips() {
    let transport = Recorded::answering(MMP);
    let rows = block_on(mmp(&transport, Some("BTCUSDT"))).unwrap();
    assert_eq!(transport.only().fields, fields(&[("symbol", "BTCUSDT")]));
    let btc = &rows[0];
    assert_eq!((btc.window_ms, btc.frozen_ms, btc.qty_limit), (5_000, 10_000, Some(Dec::from_int(10))));
    let transport = Recorded::answering("true");
    assert!(block_on(update_mmp(&transport, btc)).unwrap());
    let want = [
        ("symbol", "BTCUSDT"),
        ("windowTimeInMilliseconds", "5000"),
        ("frozenTimeInMilliseconds", "10000"),
        ("qtyLimit", "10"),
        ("valueLimit", "200000000"),
        ("deltaLimit", "100000000"),
    ];
    let call = transport.only();
    assert_eq!((call.method, call.path, call.fields.clone()), (Method::Post, "/fapi/v3/mmp", fields(&want)));
    let transport = Recorded::answering("true");
    assert!(block_on(delete_mmp(&transport, "BTCUSDT")).unwrap());
    assert_eq!((transport.only().method, transport.only().signed), (Method::Delete, true));
    let transport = Recorded::answering("true");
    assert!(block_on(reset_mmp(&transport, "BTCUSDT")).unwrap());
    assert_eq!(transport.only().path, "/fapi/v3/mmpReset");
}

#[test]
fn an_invitee_reads_with_or_without_its_figures() {
    let transport = Recorded::answering(INVITEE);
    let info = block_on(invitee_info(&transport, "0x1234...abcd")).unwrap();
    assert_eq!(transport.only().fields, fields(&[("address", "0x1234...abcd")]));
    assert!(info.is_invitee);
    assert_eq!((info.fee_paid, info.date_joined), (Dec::parse("4.82052705").ok(), Some(1_759_499_712_000)));
    let stranger: InviteeInfo = serde_json::from_str(r#"{"isInvitee":false,"walletAddress":"0xab"}"#).unwrap();
    assert_eq!((stranger.is_invitee, stranger.your_rewards, stranger.expired_status), (false, None, None));
}
