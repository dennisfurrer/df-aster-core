//! Orders on Aster's futures: placing, cancelling, modifying, chasing, strategies, and a
//! position's leverage and margin. Every call is the account's own, signed by the client's agent;
//! each builder checks what it can before anything is sent and refuses with an [`Invalid`].
//!
//! - [`order`]: [`Order`], the row every order answer comes back as.
//! - [`orders`]: [`NewOrder`] for every order type, its checks, fitting it to a symbol's grid,
//!   `POST /fapi/v3/order` and `POST /fapi/v3/batchOrders` (five at most).
//! - [`manage`]: cancel one (`DELETE /fapi/v3/order`), ten (`DELETE /fapi/v3/batchOrders`) or
//!   all (`DELETE /fapi/v3/allOpenOrders`), the countdown (`POST /fapi/v3/countdownCancelAll`),
//!   modify (`PUT /fapi/v3/order`, `PUT /fapi/v3/batchOrders`) and chase (`POST /fapi/v3/chase`).
//! - [`margin`]: leverage (`POST /fapi/v3/leverage`), cross or isolated
//!   (`POST /fapi/v3/marginType`), isolated margin (`POST /fapi/v3/positionMargin`) and its
//!   history (`GET /fapi/v3/positionMargin/history`).
//! - [`strategy`]: OTO, OCO and OTOCO (`placeStrategyOrder`, `updateStrategyOrder`,
//!   `strategyOpenOrder`, `strategyHistoryOrder`).
//! - `tracked` (with `sign`): calls signed at a chosen nonce: an order whose nonce is kept,
//!   guarded cancels (`guardedCancelOrder`, `guardedBatchOrders`) and a noop (`POST /fapi/v3/noop`).
//! - [`error`]: [`Invalid`] and [`TradeError`].
//!
//! Nothing here retries an order. One whose answer never came may be live: read the open orders
//! before placing it again.

pub mod error;
pub mod manage;
pub mod margin;
pub mod order;
pub mod orders;
pub mod strategy;
#[cfg(feature = "sign")]
pub mod tracked;

pub use error::{Invalid, TradeError};
pub use manage::{
    Chase, ChaseAnswer, Countdown, MOST_CANCELS, MOST_MODIFIES, Modify, OffsetType, OrderRef, QuantityUnit, cancel,
    cancel_all, cancel_batch, chase, countdown_cancel_all, modify, modify_batch,
};
pub use margin::{
    Leverage, MAX_LEVERAGE, MarginChange, MarginChanged, MarginEntry, MarginQuery, change_margin, margin_history,
    set_leverage, set_margin_type,
};
pub use order::Order;
pub use orders::{MOST_IN_BATCH, NewOrder, place, place_batch};
pub use strategy::{
    Driver, HistoryQuery, Leg, Strategy, StrategyAnswer, StrategyOrder, StrategyRef, StrategyType, SubOrder,
    open_strategy, place_strategy, strategy_history, update_strategy,
};
#[cfg(feature = "sign")]
pub use tracked::{Tracked, guarded_cancel, guarded_cancel_batch, noop_at, place_tracked, signed_at};
