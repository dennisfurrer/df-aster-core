//! The venue's websocket limits, the part of them one session holds ([`Limits`]), and why a
//! session refuses ([`Refused`]).

use crate::live::BadSubscription;
use crate::utils::time::DAY_MS;

/// Streams one connection may hold.
pub const MAX_STREAMS_PER_CONNECTION: usize = 200;

/// Frames a client may send a connection each second; past it the venue disconnects, and an IP
/// disconnected often may be banned.
pub const MAX_MESSAGES_PER_SECOND: usize = 10;

/// How long the venue keeps a connection: it disconnects at 24 hours.
pub const CONNECTION_LIFETIME_MS: u64 = DAY_MS;

/// What one session holds itself to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub streams: usize,
    /// Frames it sends a second: half the venue's ten by default, so a burst never reaches it.
    pub messages_per_second: usize,
}

impl Limits {
    pub const DEFAULT: Limits = Limits { streams: MAX_STREAMS_PER_CONNECTION, messages_per_second: 5 };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}

/// Why a subscription was not kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("{limit} streams is the most this session holds; the venue allows a connection 200")]
    Streams { limit: usize },
    #[error(transparent)]
    Bad(#[from] BadSubscription),
}
