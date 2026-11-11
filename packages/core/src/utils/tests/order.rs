//! The venue's spellings both ways, and an unknown value kept rather than refused.

use super::*;

#[test]
fn spellings_round_trip_and_unknowns_are_kept() {
    for side in Side::ALL {
        assert_eq!(&Side::parse(side.as_str()), side);
    }
    for ty in OrderType::ALL {
        let json = serde_json::to_string(ty).unwrap();
        assert_eq!(&serde_json::from_str::<OrderType>(&json).unwrap(), ty);
    }
    assert_eq!(serde_json::from_str::<OrderStatus>("\"NEW\"").unwrap(), OrderStatus::New);
    assert_eq!(OrderStatus::parse("SOMETHING_NEW"), OrderStatus::Other("SOMETHING_NEW".to_owned()));
    assert_eq!(TimeInForce::Gtx.to_string(), "GTX");
    assert_eq!(Interval::parse("1M"), Interval::Mo1);
    assert_eq!(Interval::H4.ms(), Some(4 * 3_600_000));
    assert_eq!(MarginType::parse_lower("cross"), MarginType::Crossed);
    assert_eq!(MarginType::parse_lower("isolated"), MarginType::Isolated);
    assert!(OrderType::StopMarket.is_conditional() && !OrderType::Limit.is_conditional());
    assert_eq!(Side::Buy.opposite(), Side::Sell);
}
