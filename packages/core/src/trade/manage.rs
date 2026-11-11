//! Orders already placed: cancel one (`DELETE /fapi/v3/order`), up to ten
//! (`DELETE /fapi/v3/batchOrders`) or every one on a symbol (`DELETE /fapi/v3/allOpenOrders`);
//! cancel all after a countdown unless it is renewed (`POST /fapi/v3/countdownCancelAll`);
//! change a limit order's price and quantity (`PUT /fapi/v3/order`, `PUT /fapi/v3/batchOrders`);
//! and a chase, a post-only limit order the venue keeps re-pricing to the best bid or ask
//! (`POST /fapi/v3/chase`).

use serde::Deserialize;
use serde_json::Value;

use crate::client::{ApiError, Call, Transport, read};
use crate::trade::Order;
use crate::trade::error::{Invalid, TradeError, valid_client_id};
use crate::trade::orders::{count, json_list, quoted, rows};
use crate::utils::{Dec, Done, PositionSide, Side, TimeInForce, lenient};

/// The most orders one cancel batch names.
pub const MOST_CANCELS: usize = 10;

/// The most orders one modify batch carries: 5, or 10 for an account the venue lists as a market
/// maker. More than 5 is refused by the venue for any other account.
pub const MOST_MODIFIES: usize = 10;

/// The longest a chase's client strategy id may be.
pub const STRATEGY_ID_MAX: usize = 28;

/// An order named by the id the venue gave it, or by the client id it was placed with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OrderRef {
    Id(u64),
    Client(String),
}

impl From<u64> for OrderRef {
    fn from(id: u64) -> Self {
        OrderRef::Id(id)
    }
}

impl From<&str> for OrderRef {
    fn from(id: &str) -> Self {
        OrderRef::Client(id.to_owned())
    }
}

impl OrderRef {
    /// The field it is sent as: `orderId` or `origClientOrderId`.
    #[must_use]
    pub fn field(&self) -> (&'static str, String) {
        match self {
            OrderRef::Id(id) => ("orderId", id.to_string()),
            OrderRef::Client(id) => ("origClientOrderId", id.clone()),
        }
    }
}

/// `DELETE /fapi/v3/order`.
#[must_use]
pub fn cancel_call(symbol: &str, order: &OrderRef) -> Call {
    let (key, value) = order.field();
    Call::delete("/fapi/v3/order").field("symbol", symbol).field(key, value).signed()
}

/// Cancels one open order; answers it as it now stands.
pub async fn cancel(transport: &impl Transport, symbol: &str, order: &OrderRef) -> Result<Order, ApiError> {
    read(transport, cancel_call(symbol, order)).await
}

/// The list fields a batch cancel names its orders by: `orderIdList` as `[1,2]`, or
/// `origClientOrderIdList` as `["a","b"]`, no spaces. Every order is named the same way.
pub(crate) fn id_lists(orders: &[OrderRef]) -> Result<Vec<(&'static str, String)>, Invalid> {
    count("orders to cancel", orders.len(), MOST_CANCELS, "1 to 10")?;
    let ids: Vec<String> = orders
        .iter()
        .filter_map(|order| if let OrderRef::Id(id) = order { Some(id.to_string()) } else { None })
        .collect();
    let clients: Vec<String> = orders
        .iter()
        .filter_map(|order| if let OrderRef::Client(id) = order { Some(quoted(id)) } else { None })
        .collect();
    match (ids.is_empty(), clients.is_empty()) {
        (false, true) => Ok(vec![("orderIdList", format!("[{}]", ids.join(",")))]),
        (true, false) => Ok(vec![("origClientOrderIdList", format!("[{}]", clients.join(",")))]),
        _ => Err(Invalid::Conflict("orderIdList", "origClientOrderIdList")),
    }
}

/// `DELETE /fapi/v3/batchOrders`: one to ten orders on `symbol`, all by id or all by client id.
pub fn cancel_batch_call(symbol: &str, orders: &[OrderRef]) -> Result<Call, Invalid> {
    let mut call = Call::delete("/fapi/v3/batchOrders").field("symbol", symbol).signed();
    call.fields.extend(id_lists(orders)?);
    Ok(call)
}

/// Cancels one to ten orders; a row an order, in the order named: the order, or why it was not
/// cancelled.
pub async fn cancel_batch(
    transport: &impl Transport,
    symbol: &str,
    orders: &[OrderRef],
) -> Result<Vec<Result<Order, ApiError>>, TradeError> {
    let call = cancel_batch_call(symbol, orders)?;
    let path = call.path;
    let answer: Vec<Value> = read(transport, call).await?;
    Ok(rows(path, answer))
}

/// `DELETE /fapi/v3/allOpenOrders`.
#[must_use]
pub fn cancel_all_call(symbol: &str) -> Call {
    Call::delete("/fapi/v3/allOpenOrders").field("symbol", symbol).signed()
}

/// Cancels every open order on `symbol`.
pub async fn cancel_all(transport: &impl Transport, symbol: &str) -> Result<(), ApiError> {
    read::<Done>(transport, cancel_all_call(symbol)).await.map(|_| ())
}

/// What a countdown answers: the symbol and the countdown now set, in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Countdown {
    pub symbol: String,
    #[serde(deserialize_with = "lenient::int")]
    pub countdown_time: i64,
}

/// `POST /fapi/v3/countdownCancelAll`.
#[must_use]
pub fn countdown_cancel_all_call(symbol: &str, countdown_ms: u64) -> Call {
    Call::post("/fapi/v3/countdownCancelAll").field("symbol", symbol).field("countdownTime", countdown_ms).signed()
}

/// Cancels every open order on `symbol` once `countdown_ms` passes without another call; 0
/// stops the countdown. Call it again well inside the countdown (the docs: every 30 seconds
/// for 120): the venue checks about every 10 milliseconds.
pub async fn countdown_cancel_all(
    transport: &impl Transport,
    symbol: &str,
    countdown_ms: u64,
) -> Result<Countdown, ApiError> {
    read(transport, countdown_cancel_all_call(symbol, countdown_ms)).await
}

/// A limit order's new price and quantity. Its time priority is reset, even when only the
/// quantity changes; an order may be modified at most 10,000 times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modify {
    pub symbol: String,
    pub order: OrderRef,
    /// Checked against the order's side by the venue when given.
    pub side: Option<Side>,
    pub quantity: Dec,
    pub price: Dec,
}

impl Modify {
    #[must_use]
    pub fn new(symbol: &str, order: impl Into<OrderRef>, quantity: Dec, price: Dec) -> Modify {
        Modify { symbol: symbol.to_owned(), order: order.into(), side: None, quantity, price }
    }

    #[must_use]
    pub fn side(mut self, side: Side) -> Modify {
        self.side = Some(side);
        self
    }

    /// The fields as they go out.
    pub fn fields(&self) -> Result<Vec<(&'static str, String)>, Invalid> {
        for (param, value) in [("quantity", self.quantity), ("price", self.price)] {
            if !value.is_positive() {
                return Err(Invalid::OutOfRange { param, value: value.to_wire(), range: "above zero" });
            }
        }
        let mut fields = vec![("symbol", self.symbol.clone()), self.order.field()];
        if let Some(side) = &self.side {
            fields.push(("side", side.to_string()));
        }
        fields.push(("quantity", self.quantity.to_wire()));
        fields.push(("price", self.price.to_wire()));
        Ok(fields)
    }
}

/// `PUT /fapi/v3/order`.
pub fn modify_call(modify: &Modify) -> Result<Call, Invalid> {
    let mut call = Call::put("/fapi/v3/order").signed().places(1);
    call.fields = modify.fields()?;
    Ok(call)
}

/// Changes a limit order's price and quantity. Refused by the venue off the grid; the order then
/// stays as it was.
pub async fn modify(transport: &impl Transport, modify: &Modify) -> Result<Order, TradeError> {
    Ok(read(transport, modify_call(modify)?).await?)
}

/// `PUT /fapi/v3/batchOrders`.
pub fn modify_batch_call(modifies: &[Modify]) -> Result<Call, Invalid> {
    count("batchOrders", modifies.len(), MOST_MODIFIES, "1 to 10")?;
    let rows = modifies.iter().map(Modify::fields).collect::<Result<Vec<_>, _>>()?;
    let n = u32::try_from(modifies.len()).unwrap_or(u32::MAX);
    Ok(Call::put("/fapi/v3/batchOrders").field("batchOrders", json_list(&rows)).signed().places(n))
}

/// Changes up to five (or ten) limit orders, each on its own; a row an order, in the order sent.
pub async fn modify_batch(
    transport: &impl Transport,
    modifies: &[Modify],
) -> Result<Vec<Result<Order, ApiError>>, TradeError> {
    let call = modify_batch_call(modifies)?;
    let path = call.path;
    let answer: Vec<Value> = read(transport, call).await?;
    Ok(rows(path, answer))
}

/// What a chase's quantity counts: the base, or the quote converted at the mark price.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuantityUnit {
    Base,
    Quote,
}

impl QuantityUnit {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            QuantityUnit::Base => "BASE",
            QuantityUnit::Quote => "QUOTE",
        }
    }
}

/// How a chase's furthest offset is counted: in price, or in percent (two places at most).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetType {
    Absolute,
    Percentage,
}

impl OffsetType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            OffsetType::Absolute => "ABSOLUTE",
            OffsetType::Percentage => "PERCENTAGE",
        }
    }
}

/// A chase: a post-only limit order kept at the best bid (a buy) or ask (a sell), less or plus
/// `offset`, re-priced each second until it fills, is cancelled, or the market moves
/// `max_offset` from where it started. Cancel it with [`cancel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chase {
    pub symbol: String,
    pub side: Side,
    pub position_side: Option<PositionSide>,
    pub unit: QuantityUnit,
    pub quantity: Dec,
    pub reduce_only: Option<bool>,
    /// The distance from the best price, a multiple of the tick; 0 when `None`.
    pub offset: Option<Dec>,
    pub max_offset: Option<(Dec, OffsetType)>,
    /// `GTX` (post only) when `None`.
    pub time_in_force: Option<TimeInForce>,
    pub client_strategy_id: Option<String>,
}

impl Chase {
    #[must_use]
    pub fn new(symbol: &str, side: Side, unit: QuantityUnit, quantity: Dec) -> Chase {
        Chase {
            symbol: symbol.to_owned(),
            side,
            position_side: None,
            unit,
            quantity,
            reduce_only: None,
            offset: None,
            max_offset: None,
            time_in_force: None,
            client_strategy_id: None,
        }
    }

    /// The fields as they go out.
    pub fn fields(&self) -> Result<Vec<(&'static str, String)>, Invalid> {
        if !self.quantity.is_positive() {
            return Err(Invalid::OutOfRange { param: "quantity", value: self.quantity.to_wire(), range: "above zero" });
        }
        if let Some(offset) = self.offset.filter(|offset| offset.is_negative()) {
            return Err(Invalid::OutOfRange { param: "chaseOffset", value: offset.to_wire(), range: "zero or above" });
        }
        if let Some((most, kind)) = self.max_offset {
            if !most.is_positive() {
                return Err(Invalid::OutOfRange {
                    param: "maxChaseOffset",
                    value: most.to_wire(),
                    range: "above zero",
                });
            }
            if kind == OffsetType::Percentage && most.places() > 2 {
                let value = most.to_wire();
                return Err(Invalid::OutOfRange { param: "maxChaseOffset", value, range: "two places at most" });
            }
        }
        if let Some(id) = self.client_strategy_id.as_deref().filter(|id| !valid_client_id(id, STRATEGY_ID_MAX)) {
            return Err(Invalid::ClientId(id.to_owned()));
        }
        let mut fields = vec![("symbol", self.symbol.clone()), ("side", self.side.to_string())];
        if let Some(side) = &self.position_side {
            fields.push(("positionSide", side.to_string()));
        }
        fields.push(("quantityUnit", self.unit.as_str().to_owned()));
        fields.push(("quantity", self.quantity.to_wire()));
        if let Some(reduce_only) = self.reduce_only {
            fields.push(("reduceOnly", reduce_only.to_string()));
        }
        if let Some(offset) = self.offset {
            fields.push(("chaseOffset", offset.to_wire()));
        }
        if let Some((most, kind)) = self.max_offset {
            fields.push(("maxChaseOffset", most.to_wire()));
            fields.push(("maxChaseOffsetType", kind.as_str().to_owned()));
        }
        if let Some(tif) = &self.time_in_force {
            fields.push(("timeInForce", tif.to_string()));
        }
        if let Some(id) = &self.client_strategy_id {
            fields.push(("clientStrategyId", id.clone()));
        }
        Ok(fields)
    }
}

/// A chase as the venue took it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaseAnswer {
    pub strategy_id: u64,
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub client_strategy_id: String,
    pub symbol: String,
    pub side: Side,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub quantity: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub quantity_unit: String,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub reduce_only: bool,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub chase_offset: Dec,
    #[serde(default, deserialize_with = "lenient::text")]
    pub chase_offset_type: String,
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub max_chase_offset: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub max_chase_offset_type: String,
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub strategy_status: String,
    #[serde(default)]
    pub book_time: Option<u64>,
    #[serde(default)]
    pub update_time: Option<u64>,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// `POST /fapi/v3/chase`.
pub fn chase_call(chase: &Chase) -> Result<Call, Invalid> {
    let mut call = Call::post("/fapi/v3/chase").signed().places(1);
    call.fields = chase.fields()?;
    Ok(call)
}

/// Places a chase.
pub async fn chase(transport: &impl Transport, chase: &Chase) -> Result<ChaseAnswer, TradeError> {
    Ok(read(transport, chase_call(chase)?).await?)
}

#[cfg(test)]
#[path = "tests/manage.rs"]
mod tests;
