//! The quiet question, the silence, the reconnect delays and the 24 hours, on a fixed clock.

use super::*;
use crate::live::{Session, list_subscriptions};

const FAST: Timing = Timing {
    quiet_ms: 100,
    silent_ms: 300,
    steady_ms: 1_000,
    first_retry_ms: 20,
    last_retry_ms: 80,
    renew_after_ms: 10_000,
};

#[test]
fn a_quiet_socket_is_asked_once_a_quiet_spell_and_a_silent_one_given_up() {
    let mut session = Session::new().with_timing(FAST);
    assert_eq!((session.tick(0), session.due()), (Tick::Wait, None), "closed");
    session.opened(0);
    assert_eq!(session.due(), Some(100));
    assert_eq!(session.tick(99), Tick::Wait);
    assert_eq!(session.tick(100), Tick::Send);
    assert_eq!(session.owed(100), vec![list_subscriptions(1)]);
    assert_eq!(session.tick(150), Tick::Wait, "asked already this spell");
    assert_eq!(session.due(), Some(200));
    session.received(160, r#"{"result":[],"id":1}"#).unwrap();
    assert_eq!(session.due(), Some(260));
    assert_eq!(session.tick(460), Tick::Reconnect);
}

#[test]
fn reconnects_back_off_and_start_over_after_a_socket_that_held() {
    let mut session = Session::new().with_timing(FAST);
    let delays: Vec<u64> = (0..4).map(|_| session.closed(0, 0).after_ms).collect();
    assert_eq!(delays, vec![20, 40, 80, 80]);
    assert_eq!(session.closed(0, 7).after_ms, 87, "jitter adds up to a quarter: 7 of at most 20");
    session.opened(1_000);
    session.heard(1_500);
    assert_eq!(session.closed(2_000, 0).after_ms, 20, "held a second and heard: starts over");
    session.opened(3_000);
    assert_eq!(session.closed(3_100, 0).after_ms, 40, "dropped soon: keeps backing off");
}

#[test]
fn a_socket_near_its_24_hours_is_renewed_at_once() {
    let mut session = Session::new().with_timing(FAST);
    session.opened(0);
    for at in (50..10_000).step_by(50) {
        session.heard(at);
        session.tick(at);
    }
    assert_eq!(session.tick(10_000), Tick::Renew);
    assert_eq!(session.renewed().after_ms, 0);
    assert!(!session.is_open());
    assert_eq!(Timing::DEFAULT.renew_after_ms + 10 * 60_000, crate::live::CONNECTION_LIFETIME_MS);
}
