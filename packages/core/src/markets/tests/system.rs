//! `ping`, `time` and the asset logos, recorded from fapi.asterdex.com on 2026-11-11 (the logos
//! cut to three).

use super::*;
use crate::test_support::{Recorded, block_on};

#[test]
fn ping_and_time_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_ping.json"));
    block_on(ping(&transport)).unwrap();
    assert_eq!((transport.only().path, transport.only().weight), ("/fapi/v3/ping", 1));
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_time.json"));
    assert_eq!(block_on(server_time(&transport)).unwrap(), 1_794_400_048_197);
    assert_eq!(transport.only().path, "/fapi/v3/time");
    assert!(!transport.only().signed);
}

#[test]
fn asset_logos_read() {
    let transport = Recorded::answering(include_str!("../../../fixtures/aster/markets_assetLogos.json"));
    let logos = block_on(asset_logos(&transport)).unwrap();
    assert_eq!(transport.only().path, "/fapi/v3/common/asset/all-asset-logo");
    assert_eq!(logos.len(), 3);
    assert_eq!(logos[0].asset_code, "BTC");
    assert!(logos[0].logo_url.starts_with("https://"));
}
