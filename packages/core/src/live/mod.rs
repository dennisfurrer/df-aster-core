//! The websocket without the socket: streams, the frames sent and received, the connection's
//! state, what a feed hands back, an account's listen key, and a kept book. Each piece is sans-IO
//! and stands alone: an app that runs its own socket sends [`request`] frames, reads with
//! [`decode`] and keeps a book with [`store::Book`]; the native and browser feeds
//! (`client::native::Feed`, `client::web::Feed`) carry a [`Session`] over a socket for an app
//! that does not.
//!
//! - [`subscription`]: every stream and its name.
//! - [`request`]: subscribe, unsubscribe, list, and the `combined` property.
//! - [`frame`]: received frames read into [`Frame`]s.
//! - [`market`], [`account`]: what each stream sends; the account's listen key and its keepalive.
//! - [`session`]: one connection's state: what is wanted, the rate of frames, answers matched to
//!   requests, the reconnect, the 24 hours, the book chains.
//! - [`feed`]: what a feed hands back.
//! - [`store`]: a book kept from a snapshot and the diff stream.

pub mod account;
pub mod feed;
pub mod frame;
pub mod market;
pub mod request;
pub mod session;
pub mod store;
pub mod subscription;

pub use account::{
    AccountChange, AccountUpdate, Announcement, BalanceChange, ConfigUpdate, Execution, KEEP_ALIVE_MS, KeepAlive,
    LISTEN_KEY_LIFETIME_MS, LeverageChange, ListenKey, ListenKeyExpired, MarginCall, MarginCallPosition, ModesChange,
    Notice, OrderEvent, OrderUpdate, PositionChange,
};
pub use feed::{Event, FeedError};
pub use frame::{BadFrame, Data, Frame, Gap, decode};
pub use market::{
    AggTrade, Bar, BookTicker, DepthUpdate, Kline, Level, Liquidation, LiquidationOrder, MarkPrice, MiniTicker, Ticker,
    TradePro,
};
pub use request::{Asked, list_subscriptions, subscribe, unsubscribe};
pub use session::{
    CONNECTION_LIFETIME_MS, Limits, MAX_MESSAGES_PER_SECOND, MAX_STREAMS_PER_CONNECTION, Refused, Retry, Session, Tick,
    Timing,
};
pub use store::{Applied, Book, Snapshot};
pub use subscription::{BadSubscription, Levels, Speed, Subscription};

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
