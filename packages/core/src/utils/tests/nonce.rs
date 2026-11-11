//! Nonces never repeat and never go back.

use super::*;

#[test]
fn never_repeats_or_goes_back() {
    let nonces = Nonces::default();
    assert_eq!(nonces.last(), 0);
    assert_eq!(nonces.next(1_000), 1_000);
    assert_eq!(nonces.next(1_000), 1_001);
    assert_eq!(nonces.next(900), 1_002);
    assert_eq!(nonces.next(5_000), 5_000);
    assert_eq!(nonces.last(), 5_000);
}
