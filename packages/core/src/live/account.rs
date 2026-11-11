//! An account's stream (the docs' "User Data Streams"): the listen key that opens it, kept alive
//! by the agent's signed calls, and what it sends. Each field is named in full, with the venue's
//! key beside it.
//!
//! A listen key lasts 60 minutes from its start or its last keepalive. The venue may send the
//! stream's events out of order when busy: order them by `event_time`.

use serde::{Deserialize, Deserializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::lenient::{self, Done};
use crate::utils::time::MINUTE_MS;
use crate::utils::{Dec, MarginType, OrderStatus, OrderType, PositionSide, Side, TimeInForce, WorkingType};

/// How long a listen key lasts after its start or its last keepalive.
pub const LISTEN_KEY_LIFETIME_MS: u64 = 60 * MINUTE_MS;

/// How often [`KeepAlive`] asks for a keepalive: half the key's life, so one that fails has
/// time to be tried again.
pub const KEEP_ALIVE_MS: u64 = 30 * MINUTE_MS;

/// What `POST /fapi/v3/listenKey` answers.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenKey {
    pub listen_key: String,
}

/// Its length and its first characters only: the key reads the account's events.
impl std::fmt::Debug for ListenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown: String = self.listen_key.chars().take(6).collect();
        write!(f, "ListenKey({shown}.., {} chars)", self.listen_key.len())
    }
}

/// `POST /fapi/v3/listenKey`, signed by the agent: a new listen key, or the account's active one
/// with its life extended.
#[must_use]
pub fn start_call() -> Call {
    Call::post("/fapi/v3/listenKey").signed()
}

/// `PUT /fapi/v3/listenKey`, signed: the active key lasts another 60 minutes.
#[must_use]
pub fn keep_alive_call() -> Call {
    Call::put("/fapi/v3/listenKey").signed()
}

/// `DELETE /fapi/v3/listenKey`, signed: the stream closes and the key no longer works.
#[must_use]
pub fn close_call() -> Call {
    Call::delete("/fapi/v3/listenKey").signed()
}

/// Starts the account's stream, or extends the active one: its listen key.
pub async fn start(transport: &impl Transport) -> Result<ListenKey, ApiError> {
    read(transport, start_call()).await
}

/// Extends the active listen key by 60 minutes.
pub async fn keep_alive(transport: &impl Transport) -> Result<(), ApiError> {
    read::<Done>(transport, keep_alive_call()).await.map(|_| ())
}

/// Closes the account's stream.
pub async fn close(transport: &impl Transport) -> Result<(), ApiError> {
    read::<Done>(transport, close_call()).await.map(|_| ())
}

/// When a listen key is due its keepalive, with no clock of its own: the program asks
/// [`KeepAlive::due`] and calls [`keep_alive`] when it says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepAlive {
    every_ms: u64,
    last: Option<u64>,
}

impl KeepAlive {
    /// Every [`KEEP_ALIVE_MS`]; nothing is due until [`KeepAlive::kept`] is first called.
    #[must_use]
    pub const fn new() -> Self {
        KeepAlive { every_ms: KEEP_ALIVE_MS, last: None }
    }

    /// Every `every_ms` instead; anything over 50 minutes is held to 50, inside the key's life.
    #[must_use]
    pub const fn every(every_ms: u64) -> Self {
        let most = 50 * MINUTE_MS;
        KeepAlive { every_ms: if every_ms > most { most } else { every_ms }, last: None }
    }

    /// The key was started or kept alive at `now`.
    pub fn kept(&mut self, now: u64) {
        self.last = Some(now);
    }

    /// When the next keepalive is due; `None` before the key started.
    #[must_use]
    pub fn due_at(&self) -> Option<u64> {
        self.last.map(|last| last.saturating_add(self.every_ms))
    }

    /// Whether a keepalive is due at `now`.
    #[must_use]
    pub fn due(&self, now: u64) -> bool {
        self.due_at().is_some_and(|at| now >= at)
    }

    /// Whether the key has lapsed at `now`: no keepalive for its whole life.
    #[must_use]
    pub fn lapsed(&self, now: u64) -> bool {
        self.last.is_some_and(|last| now >= last.saturating_add(LISTEN_KEY_LIFETIME_MS))
    }
}

impl Default for KeepAlive {
    fn default() -> Self {
        KeepAlive::new()
    }
}

/// The listen key lapsed (`listenKeyExpired`): no more events come until the stream is
/// subscribed with a new key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListenKeyExpired {
    #[serde(rename = "E", default)]
    pub event_time: u64,
}

/// Positions close to liquidation (`MARGIN_CALL`): guidance only, and a position may already be
/// liquidated when it arrives.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarginCall {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    /// `cw`: the cross wallet balance, sent with a cross position's call.
    #[serde(rename = "cw", default, deserialize_with = "lenient::blank_dec")]
    pub cross_wallet_balance: Option<Dec>,
    #[serde(rename = "p", default, deserialize_with = "lenient::list")]
    pub positions: Vec<MarginCallPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarginCallPosition {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "ps", default = "both")]
    pub position_side: PositionSide,
    /// `pa`: the position's size, negative when short.
    #[serde(rename = "pa", default, deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    #[serde(rename = "mt", default = "crossed", deserialize_with = "margin_type")]
    pub margin_type: MarginType,
    /// `iw`: the isolated wallet, for an isolated position.
    #[serde(rename = "iw", default, deserialize_with = "lenient::zero_dec")]
    pub isolated_wallet: Dec,
    #[serde(rename = "mp", default, deserialize_with = "lenient::zero_dec")]
    pub mark_price: Dec,
    #[serde(rename = "up", default, deserialize_with = "lenient::zero_dec")]
    pub unrealized_pnl: Dec,
    /// `mm`: the maintenance margin it needs.
    #[serde(rename = "mm", default, deserialize_with = "lenient::zero_dec")]
    pub maintenance_margin: Dec,
}

/// Balances and positions that changed (`ACCOUNT_UPDATE`). A funding fee on a cross position
/// sends the balance alone; only positions with a size or an isolated wallet are sent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountUpdate {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    #[serde(rename = "a")]
    pub update: AccountChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountChange {
    /// `m`: why: `ORDER`, `FUNDING_FEE`, `DEPOSIT`, `WITHDRAW`, `MARGIN_TRANSFER`,
    /// `ASSET_TRANSFER`, `AUTO_EXCHANGE` and others, as the venue writes it.
    #[serde(rename = "m", default, deserialize_with = "lenient::text")]
    pub reason: String,
    #[serde(rename = "B", default, deserialize_with = "lenient::list")]
    pub balances: Vec<BalanceChange>,
    #[serde(rename = "P", default, deserialize_with = "lenient::list")]
    pub positions: Vec<PositionChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BalanceChange {
    #[serde(rename = "a")]
    pub asset: String,
    #[serde(rename = "wb", default, deserialize_with = "lenient::zero_dec")]
    pub wallet_balance: Dec,
    #[serde(rename = "cw", default, deserialize_with = "lenient::zero_dec")]
    pub cross_wallet_balance: Dec,
    /// `bc`: the change, less profit and commission.
    #[serde(rename = "bc", default, deserialize_with = "lenient::zero_dec")]
    pub balance_change: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PositionChange {
    #[serde(rename = "s")]
    pub symbol: String,
    /// `pa`: the size, negative when short.
    #[serde(rename = "pa", default, deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    #[serde(rename = "ep", default, deserialize_with = "lenient::zero_dec")]
    pub entry_price: Dec,
    /// `cr`: realized profit accumulated, before fees.
    #[serde(rename = "cr", default, deserialize_with = "lenient::zero_dec")]
    pub accumulated_realized: Dec,
    #[serde(rename = "up", default, deserialize_with = "lenient::zero_dec")]
    pub unrealized_pnl: Dec,
    #[serde(rename = "mt", default = "crossed", deserialize_with = "margin_type")]
    pub margin_type: MarginType,
    #[serde(rename = "iw", default, deserialize_with = "lenient::zero_dec")]
    pub isolated_wallet: Dec,
    #[serde(rename = "ps", default = "both")]
    pub position_side: PositionSide,
    /// `bep`: the break-even price, where the venue sends it.
    #[serde(rename = "bep", default, deserialize_with = "lenient::blank_dec")]
    pub break_even_price: Option<Dec>,
}

/// What happened to an order (`x`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Execution {
    New,
    Canceled,
    /// A liquidation's execution.
    Calculated,
    Expired,
    Trade,
    Amendment,
    #[serde(other)]
    Other,
}

/// An order placed, filled, cancelled or expired (`ORDER_TRADE_UPDATE`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderUpdate {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    #[serde(rename = "o")]
    pub order: OrderEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderEvent {
    #[serde(rename = "s")]
    pub symbol: String,
    /// `c`: the client order id. `autoclose-...` is a liquidation's, `adl_autoclose` an
    /// auto-deleverage's.
    #[serde(rename = "c", default, deserialize_with = "lenient::id_text")]
    pub client_order_id: String,
    #[serde(rename = "S")]
    pub side: Side,
    #[serde(rename = "o")]
    pub order_type: OrderType,
    #[serde(rename = "f", default)]
    pub time_in_force: Option<TimeInForce>,
    #[serde(rename = "q", default, deserialize_with = "lenient::zero_dec")]
    pub quantity: Dec,
    #[serde(rename = "p", default, deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(rename = "ap", default, deserialize_with = "lenient::zero_dec")]
    pub avg_price: Dec,
    /// `sp`: the stop price; to be ignored for a trailing stop.
    #[serde(rename = "sp", default, deserialize_with = "lenient::zero_dec")]
    pub stop_price: Dec,
    #[serde(rename = "x")]
    pub execution: Execution,
    #[serde(rename = "X")]
    pub status: OrderStatus,
    #[serde(rename = "i")]
    pub order_id: u64,
    /// `l`, `L`: the last fill's quantity and price; `z`: the quantity filled in all.
    #[serde(rename = "l", default, deserialize_with = "lenient::zero_dec")]
    pub last_filled_quantity: Dec,
    #[serde(rename = "L", default, deserialize_with = "lenient::zero_dec")]
    pub last_filled_price: Dec,
    #[serde(rename = "z", default, deserialize_with = "lenient::zero_dec")]
    pub filled_quantity: Dec,
    /// `N` and `n`: the commission's asset and amount, sent only with a commission.
    #[serde(rename = "N", default)]
    pub commission_asset: Option<String>,
    #[serde(rename = "n", default, deserialize_with = "lenient::blank_dec")]
    pub commission: Option<Dec>,
    #[serde(rename = "T", default)]
    pub trade_time: u64,
    #[serde(rename = "t", default, deserialize_with = "lenient::int")]
    pub trade_id: i64,
    /// `b` and `a`: the notional of the account's open bids and asks.
    #[serde(rename = "b", default, deserialize_with = "lenient::zero_dec")]
    pub bids_notional: Dec,
    #[serde(rename = "a", default, deserialize_with = "lenient::zero_dec")]
    pub asks_notional: Dec,
    /// `m`: this fill was the maker's.
    #[serde(rename = "m", default)]
    pub is_maker: bool,
    #[serde(rename = "R", default, deserialize_with = "lenient::flag")]
    pub reduce_only: bool,
    #[serde(rename = "wt", default)]
    pub working_type: Option<WorkingType>,
    /// `ot`: the type it was placed as.
    #[serde(rename = "ot", default)]
    pub original_type: Option<OrderType>,
    #[serde(rename = "ps", default = "both")]
    pub position_side: PositionSide,
    /// `cp`: a stop that closes the whole position.
    #[serde(rename = "cp", default, deserialize_with = "lenient::flag")]
    pub close_position: bool,
    /// `AP` and `cr`: a trailing stop's activation price and callback rate.
    #[serde(rename = "AP", default, deserialize_with = "lenient::blank_dec")]
    pub activation_price: Option<Dec>,
    #[serde(rename = "cr", default, deserialize_with = "lenient::blank_dec")]
    pub callback_rate: Option<Dec>,
    /// `rp`: this trade's realized profit.
    #[serde(rename = "rp", default, deserialize_with = "lenient::zero_dec")]
    pub realized_profit: Dec,
}

/// The account's settings changed (`ACCOUNT_CONFIG_UPDATE`): a symbol's leverage, or the
/// account's modes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConfigUpdate {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    #[serde(rename = "ac", default)]
    pub leverage: Option<LeverageChange>,
    #[serde(rename = "ai", default)]
    pub modes: Option<ModesChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeverageChange {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "l")]
    pub leverage: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModesChange {
    /// `j`: multi-assets margin.
    #[serde(rename = "j", default)]
    pub multi_assets: Option<bool>,
    /// `f`: fees paid in the venue's token.
    #[serde(rename = "f", default)]
    pub token_fees: Option<bool>,
    /// `d`: hedge mode (both sides at once); false is one-way.
    #[serde(rename = "d", default)]
    pub hedge_mode: Option<bool>,
}

/// An announcement, public (`NEW_LISTING`) or to this account alone (`DIRECT`), in English and
/// Chinese.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Announcement {
    #[serde(default)]
    pub en: Option<Notice>,
    #[serde(default)]
    pub zh: Option<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Notice {
    #[serde(rename = "i")]
    pub id: u64,
    #[serde(rename = "t", default, deserialize_with = "lenient::text")]
    pub title: String,
    #[serde(rename = "s", default, deserialize_with = "lenient::text")]
    pub subtitle: String,
    /// `c`: `NEW_LISTING` for a public one, `DIRECT` for one to this account.
    #[serde(rename = "c", default, deserialize_with = "lenient::text")]
    pub category: String,
    /// `d`: when, unix ms.
    #[serde(rename = "d", default)]
    pub time: u64,
    /// `S`: whether it was read.
    #[serde(rename = "S", default)]
    pub read: bool,
    /// `j`: the link it points to.
    #[serde(rename = "j", default, deserialize_with = "lenient::text")]
    pub link: String,
    /// `C`: its content, as HTML.
    #[serde(rename = "C", default, deserialize_with = "lenient::text")]
    pub content: String,
}

impl Notice {
    /// Whether it is to this account alone.
    #[must_use]
    pub fn is_direct(&self) -> bool {
        self.category == "DIRECT"
    }
}

fn both() -> PositionSide {
    PositionSide::Both
}

fn crossed() -> MarginType {
    MarginType::Crossed
}

/// `isolated`, `cross` or `CROSSED`, whatever the case.
fn margin_type<'de, D: Deserializer<'de>>(deserializer: D) -> Result<MarginType, D::Error> {
    let text = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
    Ok(MarginType::parse_lower(&text))
}

#[cfg(test)]
#[path = "tests/account.rs"]
mod tests;
