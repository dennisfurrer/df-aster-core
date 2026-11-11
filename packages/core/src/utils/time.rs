//! Time as the venue counts it: unix milliseconds for every timestamp, microseconds for nonces.

pub const SECOND_MS: u64 = 1_000;
pub const MINUTE_MS: u64 = 60 * SECOND_MS;
pub const HOUR_MS: u64 = 60 * MINUTE_MS;
pub const DAY_MS: u64 = 24 * HOUR_MS;

/// How far a nonce may be from the venue's clock, either way.
pub const NONCE_WINDOW_MS: u64 = 60 * SECOND_MS;

/// Unix milliseconds as microseconds, for a nonce.
#[must_use]
pub const fn ms_to_us(ms: u64) -> u64 {
    ms.saturating_mul(1_000)
}
