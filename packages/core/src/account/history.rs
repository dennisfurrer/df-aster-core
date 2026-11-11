//! What an account did: one order by its id (`order`, and `openOrder` while it is open), the open
//! orders (`openOrders`), every order of the last week (`allOrders`), fills (`userTrades`),
//! income such as funding, fees and realized profit (`income`, and page by page `incomePage`),
//! orders the venue placed to liquidate or deleverage (`forceOrders`), and isolated margin added
//! or removed (`positionMargin/history`).
//!
//! Orders come back as [`crate::trade::Order`]. The venue forgets an order that was cancelled or
//! expired with no fill once it is seven days old.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::client::http::{ApiError, Call, Transport, read};
use crate::trade::Order;
use crate::utils::lenient;
use crate::utils::{Dec, PositionSide, Side};

/// Which order: by the venue's id or by the client's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OrderLookup {
    Id(u64),
    Client(String),
}

impl OrderLookup {
    fn on(&self, call: Call) -> Call {
        match self {
            OrderLookup::Id(id) => call.field("orderId", id),
            OrderLookup::Client(id) => call.field("origClientOrderId", id),
        }
    }
}

/// A window and a page size: every part optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TimeRange {
    /// Unix ms, inclusive.
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub limit: Option<u32>,
}

impl TimeRange {
    fn on(&self, call: Call) -> Call {
        call.maybe("startTime", self.start_time).maybe("endTime", self.end_time).maybe("limit", self.limit)
    }
}

/// `GET /fapi/v3/order`.
#[must_use]
pub fn query_order_call(symbol: &str, order: &OrderLookup) -> Call {
    order.on(Call::get("/fapi/v3/order").field("symbol", symbol)).signed()
}

/// One order, open or done.
pub async fn query_order(transport: &impl Transport, symbol: &str, order: &OrderLookup) -> Result<Order, ApiError> {
    read(transport, query_order_call(symbol, order)).await
}

/// `GET /fapi/v3/openOrder`.
#[must_use]
pub fn open_order_call(symbol: &str, order: &OrderLookup) -> Call {
    order.on(Call::get("/fapi/v3/openOrder").field("symbol", symbol)).signed()
}

/// One open order; one that filled or was cancelled is refused as not existing.
pub async fn open_order(transport: &impl Transport, symbol: &str, order: &OrderLookup) -> Result<Order, ApiError> {
    read(transport, open_order_call(symbol, order)).await
}

/// `GET /fapi/v3/openOrders`: weight 1 for one symbol, 40 for every one.
#[must_use]
pub fn open_orders_call(symbol: Option<&str>) -> Call {
    let call = Call::get("/fapi/v3/openOrders").maybe("symbol", symbol).signed();
    match symbol {
        Some(_) => call,
        None => call.weigh(40),
    }
}

/// The open orders on one symbol, or on every one.
pub async fn open_orders(transport: &impl Transport, symbol: Option<&str>) -> Result<Vec<Order>, ApiError> {
    read(transport, open_orders_call(symbol)).await
}

/// `GET /fapi/v3/allOrders`. `from_id` reads orders from that id up; otherwise the newest come
/// back. The window may not be longer than seven days (the last seven by default); `limit`
/// defaults to 500, at most 1,000.
#[must_use]
pub fn all_orders_call(symbol: Option<&str>, from_id: Option<u64>, range: &TimeRange) -> Call {
    range.on(Call::get("/fapi/v3/allOrders").maybe("symbol", symbol).maybe("orderId", from_id)).signed()
}

pub async fn all_orders(
    transport: &impl Transport,
    symbol: Option<&str>,
    from_id: Option<u64>,
    range: &TimeRange,
) -> Result<Vec<Order>, ApiError> {
    read(transport, all_orders_call(symbol, from_id, range)).await
}

/// One fill of the account's (`userTrades`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    pub symbol: String,
    /// The trade's id.
    pub id: u64,
    pub order_id: u64,
    pub side: Side,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub qty: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub quote_qty: Dec,
    /// The fee, as the venue signs it.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub commission: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub commission_asset: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub realized_pnl: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub buyer: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub maker: bool,
    pub time: u64,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// `GET /fapi/v3/userTrades`. With neither time the last seven days come back; a window may not
/// be longer than seven days; `from_id` may not go with a window; `order_id` filters, ahead of
/// the rest. `limit` defaults to 500, at most 1,000.
#[must_use]
pub fn user_trades_call(symbol: &str, order_id: Option<u64>, from_id: Option<u64>, range: &TimeRange) -> Call {
    let call = Call::get("/fapi/v3/userTrades").field("symbol", symbol).maybe("orderId", order_id);
    let call = call.maybe("startTime", range.start_time).maybe("endTime", range.end_time);
    call.maybe("fromId", from_id).maybe("limit", range.limit).signed()
}

pub async fn user_trades(
    transport: &impl Transport,
    symbol: &str,
    order_id: Option<u64>,
    from_id: Option<u64>,
    range: &TimeRange,
) -> Result<Vec<Fill>, ApiError> {
    read(transport, user_trades_call(symbol, order_id, from_id, range)).await
}

/// What an income row is for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IncomeType {
    Transfer,
    Withdraw,
    Deposit,
    WelcomeBonus,
    RealizedPnl,
    FundingFee,
    Commission,
    InsuranceClear,
    MarketMerchantReturnReward,
    ReferralKickback,
    CommissionRebate,
    MarketMakerRebate,
    ApiRebate,
    ContestReward,
    CrossCollateralTransfer,
    InternalTransfer,
    AutoExchange,
    /// A type this build does not know, as the venue wrote it.
    Other(String),
}

impl IncomeType {
    const NAMES: [(IncomeType, &'static str); 17] = [
        (IncomeType::Transfer, "TRANSFER"),
        (IncomeType::Withdraw, "WITHDRAW"),
        (IncomeType::Deposit, "DEPOSIT"),
        (IncomeType::WelcomeBonus, "WELCOME_BONUS"),
        (IncomeType::RealizedPnl, "REALIZED_PNL"),
        (IncomeType::FundingFee, "FUNDING_FEE"),
        (IncomeType::Commission, "COMMISSION"),
        (IncomeType::InsuranceClear, "INSURANCE_CLEAR"),
        (IncomeType::MarketMerchantReturnReward, "MARKET_MERCHANT_RETURN_REWARD"),
        (IncomeType::ReferralKickback, "REFERRAL_KICKBACK"),
        (IncomeType::CommissionRebate, "COMMISSION_REBATE"),
        (IncomeType::MarketMakerRebate, "MARKET_MAKER_REBATE"),
        (IncomeType::ApiRebate, "API_REBATE"),
        (IncomeType::ContestReward, "CONTEST_REWARD"),
        (IncomeType::CrossCollateralTransfer, "CROSS_COLLATERAL_TRANSFER"),
        (IncomeType::InternalTransfer, "INTERNAL_TRANSFER"),
        (IncomeType::AutoExchange, "AUTO_EXCHANGE"),
    ];

    /// As the venue writes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            IncomeType::Other(text) => text,
            known => Self::NAMES.iter().find(|(ty, _)| ty == known).map_or("", |(_, name)| name),
        }
    }

    /// From the venue's spelling; anything else is `Other`.
    #[must_use]
    pub fn parse(text: &str) -> IncomeType {
        Self::NAMES
            .iter()
            .find(|(_, name)| *name == text)
            .map_or_else(|| IncomeType::Other(text.to_owned()), |(ty, _)| ty.clone())
    }
}

impl fmt::Display for IncomeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for IncomeType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IncomeType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Ok(IncomeType::parse(&text))
    }
}

/// One income row: money in (positive) or out (negative) of the account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Income {
    /// Empty when it is not a symbol's.
    #[serde(default, deserialize_with = "lenient::text")]
    pub symbol: String,
    pub income_type: IncomeType,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub income: Dec,
    pub asset: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub info: String,
    pub time: u64,
    /// Unique within its type for the account; written as a number or a string.
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub tran_id: String,
    /// Empty when it is not a trade's.
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub trade_id: String,
}

/// What income to read: a symbol, a type, a window and a page size, every part optional.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IncomeQuery {
    pub symbol: Option<String>,
    pub income_type: Option<IncomeType>,
    pub range: TimeRange,
}

impl IncomeQuery {
    fn on(&self, call: Call) -> Call {
        let call = call.maybe("symbol", self.symbol.as_deref()).maybe("incomeType", self.income_type.as_ref());
        self.range.on(call)
    }
}

/// `GET /fapi/v3/income`, weight 30. With neither time the last seven days come back; `limit`
/// defaults to 100, at most 1,000.
#[must_use]
pub fn income_call(query: &IncomeQuery) -> Call {
    query.on(Call::get("/fapi/v3/income")).signed()
}

pub async fn income(transport: &impl Transport, query: &IncomeQuery) -> Result<Vec<Income>, ApiError> {
    read(transport, income_call(query)).await
}

/// One page of income, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomePage {
    /// Rows matching across every page.
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub has_more: bool,
    #[serde(default, deserialize_with = "lenient::list")]
    pub rows: Vec<Income>,
}

/// `GET /fapi/v3/incomePage`, weight 30: `page` from 1, `limit` up to 1,000, and page times
/// limit at most 100,000. Send a fixed `end_time` while paging, or new rows shift the pages.
#[must_use]
pub fn income_page_call(query: &IncomeQuery, page: u32) -> Call {
    let call = Call::get("/fapi/v3/incomePage").maybe("symbol", query.symbol.as_deref());
    let call = call.maybe("incomeType", query.income_type.as_ref());
    let call = call.maybe("startTime", query.range.start_time).maybe("endTime", query.range.end_time);
    call.field("page", page).maybe("limit", query.range.limit).signed()
}

pub async fn income_page(transport: &impl Transport, query: &IncomeQuery, page: u32) -> Result<IncomePage, ApiError> {
    read(transport, income_page_call(query, page)).await
}

/// Which orders the venue placed for the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoCloseType {
    Liquidation,
    Adl,
}

impl AutoCloseType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AutoCloseType::Liquidation => "LIQUIDATION",
            AutoCloseType::Adl => "ADL",
        }
    }
}

/// `GET /fapi/v3/forceOrders`: weight 20 for one symbol, 50 for every one. Both types when none
/// is given; `limit` defaults to 50, at most 100.
#[must_use]
pub fn force_orders_call(symbol: Option<&str>, auto_close: Option<AutoCloseType>, range: &TimeRange) -> Call {
    let call = Call::get("/fapi/v3/forceOrders").maybe("symbol", symbol);
    let call = range.on(call.maybe("autoCloseType", auto_close.map(AutoCloseType::as_str))).signed();
    match symbol {
        Some(_) => call.weigh(20),
        None => call.weigh(50),
    }
}

/// The account's liquidation and ADL orders.
pub async fn force_orders(
    transport: &impl Transport,
    symbol: Option<&str>,
    auto_close: Option<AutoCloseType>,
    range: &TimeRange,
) -> Result<Vec<Order>, ApiError> {
    read(transport, force_orders_call(symbol, auto_close, range)).await
}

/// One change to an isolated position's margin.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginChange {
    pub symbol: String,
    pub asset: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub amount: Dec,
    /// 1 added, 2 removed.
    #[serde(rename = "type")]
    pub change_type: u8,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    /// `TRADE`, `USER_ADJUST` and the like.
    #[serde(default, deserialize_with = "lenient::text")]
    pub delta_type: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub client_tran_id: String,
    pub time: u64,
}

impl MarginChange {
    /// Whether margin was added.
    #[must_use]
    pub const fn is_added(&self) -> bool {
        self.change_type == 1
    }
}

/// `GET /fapi/v3/positionMargin/history`. `added` keeps only additions (`true`) or removals
/// (`false`); `limit` defaults to 500.
#[must_use]
pub fn margin_history_call(symbol: &str, added: Option<bool>, range: &TimeRange) -> Call {
    let change_type = added.map(|added| if added { 1 } else { 2 });
    range.on(Call::get("/fapi/v3/positionMargin/history").field("symbol", symbol).maybe("type", change_type)).signed()
}

pub async fn margin_history(
    transport: &impl Transport,
    symbol: &str,
    added: Option<bool>,
    range: &TimeRange,
) -> Result<Vec<MarginChange>, ApiError> {
    read(transport, margin_history_call(symbol, added, range)).await
}

#[cfg(test)]
#[path = "tests/history.rs"]
mod tests;
