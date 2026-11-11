//! What a builder reads about the users that approved it, page by page. The account the agent
//! signs for is the builder; the reads are its own (`/fapi/v3/builder/*`, weight 5 each), and its
//! account must have deposited once.
//!
//! - [`user_accounts`] (`userAccounts`): each user's settings and per-symbol leverage.
//! - [`user_open_orders`] (`userOpenOrders`) and [`user_all_orders`] (`userAllOrders`): orders,
//!   as [`crate::trade::Order`] with their owner.
//! - [`user_balances`] (`userBalances`): wallet balances, with their USD value.
//! - [`user_positions`] (`userPositionRisk`): positions that hold something, as
//!   [`crate::account::Position`] with their owner.
//! - [`user_commission_rates`] (`userCommissionRates`): each user's rates on a symbol.
//! - [`user_trades`] (`userTrades`) and [`user_commissions`] (`userCommissions`): fills with
//!   the builder's fee, or the fee alone (privacy mode does not hide these).
//! - [`approved_users`] (`approvedUserList`): who approved the builder, and up to what fee.
//!
//! A query may name up to 50 users ([`BuilderQuery::users`]); one that has not approved the
//! builder, or keeps its account private, is left out of `rows` and listed in `errors`. With no
//! users named, the pages run over every user bound to the builder.

use serde::Deserialize;

use crate::account::{CommissionRate, Position, address_list, number_dec};
use crate::client::http::{ApiError, Call, Transport, read};
use crate::trade::Order;
use crate::utils::lenient;
use crate::utils::{Address, Dec, PositionSide, Side};

/// The most users one query may name.
pub const MOST_USERS: usize = 50;

/// One page of a builder read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "T: Deserialize<'de>"))]
pub struct Page<T> {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub current_page: u32,
    #[serde(default)]
    pub total_pages: u32,
    #[serde(default)]
    pub page_size: u32,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub has_more: bool,
    #[serde(default, deserialize_with = "lenient::list")]
    pub rows: Vec<T>,
    /// Users named in the query that were left out, and why.
    #[serde(default, deserialize_with = "lenient::list")]
    pub errors: Vec<LeftOut>,
}

/// A named user the read left out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftOut {
    pub address: String,
    /// Not deposited, privacy mode, or not bound to this builder.
    #[serde(default, deserialize_with = "lenient::text")]
    pub error_msg: String,
}

/// A row with the user it is about. Addresses stay as the venue wrote them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Owned<T> {
    pub address: String,
    #[serde(flatten)]
    pub row: T,
}

/// What to read: users, a symbol, a window, a page; every part optional.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuilderQuery {
    /// Up to [`MOST_USERS`]; none for every bound user.
    pub users: Vec<Address>,
    pub symbol: Option<String>,
    /// Unix ms; no earlier than 90 days ago.
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    /// From 1.
    pub page: Option<u32>,
    /// 50 by default, at most 1,000.
    pub limit: Option<u32>,
}

impl BuilderQuery {
    /// The call to `path` with what applies: users, then the symbol and window where `path`
    /// takes them, then the page.
    fn call(&self, path: &'static str, symbol: bool, window: bool) -> Call {
        let call = Call::get(path).maybe("userAddresses", address_list(&self.users));
        let call = match symbol {
            true => call.maybe("symbol", self.symbol.as_deref()),
            false => call,
        };
        let call = match window {
            true => call.maybe("startTime", self.start_time).maybe("endTime", self.end_time),
            false => call,
        };
        call.maybe("page", self.page).maybe("limit", self.limit).signed()
    }
}

/// A user's settings (`userAccounts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAccount {
    pub address: String,
    #[serde(default)]
    pub fee_tier: u32,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_trade: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_deposit: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_withdraw: bool,
    #[serde(default)]
    pub update_time: u64,
    #[serde(default)]
    pub account_type: u32,
    /// Hedge mode.
    #[serde(default, deserialize_with = "lenient::flag")]
    pub dual_side_position: bool,
    /// Multi-assets mode.
    #[serde(default, deserialize_with = "lenient::flag")]
    pub joint_margin: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub fee_burn: bool,
    #[serde(default)]
    pub fee_burn_asset_id: u64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub symbol_config: Vec<SymbolConfig>,
}

/// A user's leverage on one symbol.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolConfig {
    pub symbol: String,
    #[serde(deserialize_with = "crate::account::whole")]
    pub leverage: u32,
    #[serde(default, deserialize_with = "number_dec")]
    pub notional_limit_coef: Dec,
}

/// A user's balance of one asset (`userBalances`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBalance {
    pub address: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub wallet_balance: Dec,
    /// The asset's price the USD value was taken at.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub balance_in_usd: Dec,
}

/// A user's fill, with the builder's fee (`userTrades`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderTrade {
    pub trade_id: u64,
    /// When it traded, unix ms.
    pub insert_time: u64,
    pub symbol: String,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub qty: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub base_asset: String,
    /// `CRYPTO` or `STOCK`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub base_type: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub quote_asset: String,
    pub side: Side,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub active_buy: bool,
    #[serde(default, deserialize_with = "lenient::text")]
    pub fee_asset: String,
    /// Price times quantity.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_quota: Dec,
    /// The user's fee: negative when charged, positive for a rebate.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub fee: Dec,
    pub order_id: u64,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub realized_profit: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub margin_asset: String,
    pub user_address: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub builder_fee: Dec,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// The builder's fee on one fill (`userCommissions`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderCommission {
    pub trade_id: u64,
    pub insert_time: u64,
    pub order_id: u64,
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub fee_asset: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub margin_asset: String,
    /// Zero when none was charged.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub builder_fee: Dec,
    pub user_address: String,
}

/// A user that approved the builder (`approvedUserList`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedUser {
    pub user_address: String,
    pub builder_address: String,
    /// The highest fee rate the user granted, a fraction of the notional. Written as a JSON
    /// number.
    #[serde(deserialize_with = "number_dec")]
    pub max_fee_rate: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub builder_name: String,
    /// When it approved, unix ms.
    #[serde(default)]
    pub approve_time: u64,
}

/// `GET /fapi/v3/builder/userAccounts`; `symbol` keeps that symbol's entry alone.
#[must_use]
pub fn user_accounts_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userAccounts", true, false)
}

pub async fn user_accounts(transport: &impl Transport, query: &BuilderQuery) -> Result<Page<UserAccount>, ApiError> {
    read(transport, user_accounts_call(query)).await
}

/// `GET /fapi/v3/builder/userOpenOrders`. The page runs over users, not orders.
#[must_use]
pub fn user_open_orders_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userOpenOrders", true, false)
}

pub async fn user_open_orders(
    transport: &impl Transport,
    query: &BuilderQuery,
) -> Result<Page<Owned<Order>>, ApiError> {
    read(transport, user_open_orders_call(query)).await
}

/// `GET /fapi/v3/builder/userBalances`.
#[must_use]
pub fn user_balances_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userBalances", false, false)
}

pub async fn user_balances(transport: &impl Transport, query: &BuilderQuery) -> Result<Page<UserBalance>, ApiError> {
    read(transport, user_balances_call(query)).await
}

/// `GET /fapi/v3/builder/userPositionRisk`: positions that hold something.
#[must_use]
pub fn user_positions_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userPositionRisk", true, false)
}

pub async fn user_positions(
    transport: &impl Transport,
    query: &BuilderQuery,
) -> Result<Page<Owned<Position>>, ApiError> {
    read(transport, user_positions_call(query)).await
}

/// `GET /fapi/v3/builder/userCommissionRates`: `symbol` is required here.
#[must_use]
pub fn user_commission_rates_call(query: &BuilderQuery, symbol: &str) -> Call {
    let users = address_list(&query.users);
    let call = Call::get("/fapi/v3/builder/userCommissionRates").maybe("userAddresses", users);
    call.field("symbol", symbol).maybe("page", query.page).maybe("limit", query.limit).signed()
}

pub async fn user_commission_rates(
    transport: &impl Transport,
    query: &BuilderQuery,
    symbol: &str,
) -> Result<Page<Owned<CommissionRate>>, ApiError> {
    read(transport, user_commission_rates_call(query, symbol)).await
}

/// `GET /fapi/v3/builder/userTrades`: the newest first; the last seven days by default.
#[must_use]
pub fn user_trades_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userTrades", false, true)
}

pub async fn user_trades(transport: &impl Transport, query: &BuilderQuery) -> Result<Page<BuilderTrade>, ApiError> {
    read(transport, user_trades_call(query)).await
}

/// `GET /fapi/v3/builder/userCommissions`: the same fills as [`user_trades`], the fee alone.
#[must_use]
pub fn user_commissions_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userCommissions", false, true)
}

pub async fn user_commissions(
    transport: &impl Transport,
    query: &BuilderQuery,
) -> Result<Page<BuilderCommission>, ApiError> {
    read(transport, user_commissions_call(query)).await
}

/// `GET /fapi/v3/builder/userAllOrders`: the last seven days by default.
#[must_use]
pub fn user_all_orders_call(query: &BuilderQuery) -> Call {
    query.call("/fapi/v3/builder/userAllOrders", true, true)
}

pub async fn user_all_orders(transport: &impl Transport, query: &BuilderQuery) -> Result<Page<Owned<Order>>, ApiError> {
    read(transport, user_all_orders_call(query)).await
}

/// `GET /fapi/v3/builder/approvedUserList`: by approval time; `limit` 100 by default.
#[must_use]
pub fn approved_users_call(query: &BuilderQuery) -> Call {
    let call = Call::get("/fapi/v3/builder/approvedUserList");
    let call = call.maybe("startTime", query.start_time).maybe("endTime", query.end_time);
    call.maybe("page", query.page).maybe("limit", query.limit).signed()
}

pub async fn approved_users(transport: &impl Transport, query: &BuilderQuery) -> Result<Page<ApprovedUser>, ApiError> {
    read(transport, approved_users_call(query)).await
}

#[cfg(test)]
#[path = "tests/builder.rs"]
mod tests;
