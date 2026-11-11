//! Strategy orders: two or three orders placed together, where one's fill places or cancels
//! another. `OTO` (one triggers the other), `OCO` (one cancels the other) and `OTOCO` (an entry
//! that places a take-profit and a stop, each cancelling the other).
//!
//! - `POST /fapi/v3/placeStrategyOrder`, `POST /fapi/v3/updateStrategyOrder`.
//! - `GET /fapi/v3/strategyOpenOrder`, `GET /fapi/v3/strategyHistoryOrder`.
//!
//! A refusal may come back with HTTP 200 and a `failureCode` (a bracket that would trigger at
//! once, -4142); [`place_strategy`] and [`update_strategy`] read it as the refusal it is.

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::trade::error::{Invalid, TradeError, valid_client_id};
use crate::trade::manage::STRATEGY_ID_MAX;
use crate::trade::orders::{CLIENT_ID_MAX, json_list};
use crate::utils::{
    Address, Dec, OrderStatus, OrderType, PegPriceType, PositionSide, Side, TimeInForce, WorkingType, lenient,
};

/// How a strategy's orders depend on each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrategyType {
    /// One triggers the other: two orders.
    Oto,
    /// One cancels the other: two orders.
    Oco,
    /// One triggers one-cancels-the-other: three orders.
    Otoco,
}

impl StrategyType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            StrategyType::Oto => "OTO",
            StrategyType::Oco => "OCO",
            StrategyType::Otoco => "OTOCO",
        }
    }

    /// How many orders a placement carries, and an update at most.
    #[must_use]
    pub const fn legs(self) -> usize {
        match self {
            StrategyType::Oto | StrategyType::Oco => 2,
            StrategyType::Otoco => 3,
        }
    }
}

/// What makes one leg act on another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    /// The leg whose event it is, by its place in the list, counting from 1.
    pub leg: u32,
    /// `NEW`, `PARTIALLY_FILLED_OR_FILLED`, `FILLED`, `CANCELED`, `REPLACED`, `STOPPED`,
    /// `REJECTED` or `EXPIRED`.
    pub on: String,
    /// `PLACE_ORDER` or `CANCEL_ORDER`.
    pub action: String,
}

/// One leg of a strategy. Its `strategySubId` is its place in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leg {
    /// `USDT_FUTURES` unless set.
    pub security_type: String,
    pub symbol: String,
    pub side: Side,
    pub position_side: Option<PositionSide>,
    pub order_type: OrderType,
    pub quantity: Option<Dec>,
    pub price: Option<Dec>,
    pub peg: Option<PegPriceType>,
    pub stop_price: Option<Dec>,
    pub time_in_force: Option<TimeInForce>,
    pub working_type: Option<WorkingType>,
    pub reduce_only: Option<bool>,
    pub close_position: Option<bool>,
    pub price_protect: Option<bool>,
    pub client_id: Option<String>,
    pub activation_price: Option<Dec>,
    pub callback_rate: Option<Dec>,
    pub first: Option<Driver>,
    pub second: Option<Driver>,
}

impl Leg {
    #[must_use]
    pub fn new(symbol: &str, side: Side, order_type: OrderType) -> Leg {
        Leg {
            security_type: "USDT_FUTURES".to_owned(),
            symbol: symbol.to_owned(),
            side,
            position_side: None,
            order_type,
            quantity: None,
            price: None,
            peg: None,
            stop_price: None,
            time_in_force: None,
            working_type: None,
            reduce_only: None,
            close_position: None,
            price_protect: None,
            client_id: None,
            activation_price: None,
            callback_rate: None,
            first: None,
            second: None,
        }
    }

    /// The fields of leg `at` (counting from 1), in the docs' order. A pegged leg's price goes
    /// as `0`, as the docs ask.
    fn fields(&self, at: usize) -> Result<Vec<(&'static str, String)>, Invalid> {
        if let Some(id) = self.client_id.as_deref().filter(|id| !valid_client_id(id, CLIENT_ID_MAX)) {
            return Err(Invalid::ClientId(id.to_owned()));
        }
        if matches!(self.time_in_force, Some(TimeInForce::Ioc | TimeInForce::Fok)) {
            return Err(Invalid::NotTaken { order_type: self.order_type.clone(), param: "IOC or FOK in a strategy" });
        }
        if self.peg.is_some() && !matches!(self.order_type, OrderType::Limit | OrderType::Stop | OrderType::TakeProfit)
        {
            return Err(Invalid::NotTaken { order_type: self.order_type.clone(), param: "pegPriceType" });
        }
        let mut fields = vec![
            ("strategySubId", at.to_string()),
            ("securityType", self.security_type.clone()),
            ("symbol", self.symbol.clone()),
            ("side", self.side.to_string()),
        ];
        let mut put = |key: &'static str, value: Option<String>| {
            if let Some(value) = value {
                fields.push((key, value));
            }
        };
        let wire = |value: Option<Dec>| value.map(Dec::to_wire);
        let flag = |value: Option<bool>| value.map(|flag| flag.to_string());
        put("positionSide", self.position_side.as_ref().map(ToString::to_string));
        put("type", Some(self.order_type.to_string()));
        put("quantity", wire(self.quantity));
        put("price", if self.peg.is_some() { Some("0".to_owned()) } else { wire(self.price) });
        put("pegPriceType", self.peg.as_ref().map(ToString::to_string));
        put("stopPrice", wire(self.stop_price));
        put("timeInForce", self.time_in_force.as_ref().map(ToString::to_string));
        put("workingType", self.working_type.as_ref().map(ToString::to_string));
        put("reduceOnly", flag(self.reduce_only));
        put("closePosition", flag(self.close_position));
        put("priceProtect", flag(self.price_protect));
        put("clientOrderId", self.client_id.clone());
        put("activationPrice", wire(self.activation_price));
        put("callbackRate", wire(self.callback_rate));
        if let Some(driver) = &self.first {
            put("firstDrivenId", Some(driver.leg.to_string()));
            put("firstDrivenOn", Some(driver.on.clone()));
            put("firstTrigger", Some(driver.action.clone()));
        }
        if let Some(driver) = &self.second {
            put("secondDrivenId", Some(driver.leg.to_string()));
            put("secondDrivenOn", Some(driver.on.clone()));
            put("secondTrigger", Some(driver.action.clone()));
        }
        Ok(fields)
    }
}

/// A strategy to place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strategy {
    pub strategy_type: StrategyType,
    pub legs: Vec<Leg>,
    pub client_strategy_id: Option<String>,
    /// The builder it is placed through, and its fee rate.
    pub builder: Option<Address>,
    pub fee_rate: Option<Dec>,
}

fn legs_list(kind: StrategyType, legs: &[Leg]) -> Result<String, Invalid> {
    if legs.len() != kind.legs() {
        let range = if kind == StrategyType::Otoco { "exactly 3" } else { "exactly 2" };
        return Err(Invalid::Count { what: "subOrderList", count: legs.len(), range });
    }
    let rows = legs.iter().enumerate().map(|(at, leg)| leg.fields(at + 1)).collect::<Result<Vec<_>, _>>()?;
    Ok(json_list(&rows))
}

/// `POST /fapi/v3/placeStrategyOrder`.
pub fn place_strategy_call(strategy: &Strategy) -> Result<Call, Invalid> {
    if let Some(id) = strategy.client_strategy_id.as_deref().filter(|id| !valid_client_id(id, STRATEGY_ID_MAX)) {
        return Err(Invalid::ClientId(id.to_owned()));
    }
    if strategy.fee_rate.is_some() && strategy.builder.is_none() {
        return Err(Invalid::Conflict("feeRate", "no builder"));
    }
    let list = legs_list(strategy.strategy_type, &strategy.legs)?;
    let n = u32::try_from(strategy.legs.len()).unwrap_or(u32::MAX);
    Ok(Call::post("/fapi/v3/placeStrategyOrder")
        .maybe("clientStrategyId", strategy.client_strategy_id.as_deref())
        .field("strategyType", strategy.strategy_type.as_str())
        .field("subOrderList", list)
        .maybe("builder", strategy.builder.map(Address::to_checksum))
        .maybe("feeRate", strategy.fee_rate.map(Dec::to_wire))
        .signed()
        .places(n))
}

/// What placing or updating a strategy answers. A `failure_code` other than 0 is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyAnswer {
    pub strategy_id: u64,
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub client_strategy_id: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub strategy_type: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub strategy_status: String,
    /// On an update: the leg it changed, and whether it did.
    #[serde(default)]
    pub updated_sub_order: Option<u32>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub update_status: String,
    #[serde(default)]
    pub update_time: Option<u64>,
    #[serde(default, deserialize_with = "lenient::int")]
    pub failure_code: i64,
    #[serde(default, deserialize_with = "lenient::text")]
    pub failure_reason: String,
}

impl StrategyAnswer {
    /// The refusal it carries, if any.
    #[must_use]
    pub fn failure(&self) -> Option<ApiError> {
        (self.failure_code != 0)
            .then(|| ApiError::Refused { code: self.failure_code, message: self.failure_reason.clone() })
    }
}

/// Places a strategy. A refusal the venue answers with HTTP 200 and a `failureCode` comes back
/// as [`ApiError::Refused`].
pub async fn place_strategy(transport: &impl Transport, strategy: &Strategy) -> Result<StrategyAnswer, TradeError> {
    let answer: StrategyAnswer = read(transport, place_strategy_call(strategy)?).await?;
    match answer.failure() {
        Some(refused) => Err(TradeError::Api(refused)),
        None => Ok(answer),
    }
}

/// `POST /fapi/v3/updateStrategyOrder`: the legs given, each with its `strategySubId` (its place,
/// counting from 1, in the list the strategy was placed with).
pub fn update_strategy_call(
    strategy_id: u64,
    strategy_type: StrategyType,
    legs: &[(u32, Leg)],
) -> Result<Call, Invalid> {
    if legs.is_empty() || legs.len() > strategy_type.legs() {
        let range = if strategy_type == StrategyType::Otoco { "1 to 3" } else { "1 to 2" };
        return Err(Invalid::Count { what: "subOrderList", count: legs.len(), range });
    }
    let rows = legs
        .iter()
        .map(|(sub_id, leg)| leg.fields(usize::try_from(*sub_id).unwrap_or(usize::MAX)))
        .collect::<Result<Vec<_>, _>>()?;
    let n = u32::try_from(legs.len()).unwrap_or(u32::MAX);
    Ok(Call::post("/fapi/v3/updateStrategyOrder")
        .field("strategyId", strategy_id)
        .field("strategyType", strategy_type.as_str())
        .field("subOrderList", json_list(&rows))
        .signed()
        .places(n))
}

/// Updates legs of a placed strategy, each `(sub id, leg)`; a row a leg. Any row with a
/// `failureCode` makes the whole answer that refusal.
pub async fn update_strategy(
    transport: &impl Transport,
    strategy_id: u64,
    strategy_type: StrategyType,
    legs: &[(u32, Leg)],
) -> Result<Vec<StrategyAnswer>, TradeError> {
    let answer: Vec<StrategyAnswer> = read(transport, update_strategy_call(strategy_id, strategy_type, legs)?).await?;
    match answer.iter().find_map(StrategyAnswer::failure) {
        Some(refused) => Err(TradeError::Api(refused)),
        None => Ok(answer),
    }
}

/// A strategy and its legs as the venue keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyOrder {
    pub strategy_id: u64,
    #[serde(default, deserialize_with = "lenient::id_text")]
    pub client_strategy_id: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub strategy_type: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub strategy_status: String,
    #[serde(default)]
    pub book_time: Option<u64>,
    #[serde(default)]
    pub update_time: Option<u64>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub sub_orders: Vec<SubOrder>,
}

/// One leg of a strategy as the venue keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubOrder {
    pub strategy_id: u64,
    #[serde(default)]
    pub order_id: Option<u64>,
    #[serde(default, deserialize_with = "lenient::text")]
    pub client_order_id: String,
    pub status: OrderStatus,
    pub strategy_sub_id: u32,
    #[serde(default, deserialize_with = "lenient::int")]
    pub first_driven_id: i64,
    #[serde(default, deserialize_with = "lenient::text")]
    pub first_driven_on: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub first_trigger: String,
    #[serde(default, deserialize_with = "lenient::int")]
    pub second_driven_id: i64,
    #[serde(default, deserialize_with = "lenient::text")]
    pub second_driven_on: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub second_trigger: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub security_type: String,
    pub symbol: String,
    pub side: Side,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub quantity: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub reduce_only: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub close_position: bool,
    /// Zero for a pegged leg, whose price the engine took from the book.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub avg_price: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub price_protect: bool,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub stop_price: Dec,
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub activate_price: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub callback_rate: Option<Dec>,
    #[serde(default)]
    pub working_type: Option<WorkingType>,
    #[serde(default)]
    pub peg_price_type: Option<PegPriceType>,
    /// When it triggered, unix ms; 0 before.
    #[serde(default)]
    pub trigger_time: u64,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// A strategy named by the id the venue gave it, or by its client id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StrategyRef {
    Id(u64),
    Client(String),
}

impl StrategyRef {
    fn field(&self) -> (&'static str, String) {
        match self {
            StrategyRef::Id(id) => ("strategyId", id.to_string()),
            StrategyRef::Client(id) => ("clientStrategyId", id.clone()),
        }
    }
}

/// `GET /fapi/v3/strategyOpenOrder`.
#[must_use]
pub fn open_strategy_call(strategy: &StrategyRef, strategy_type: StrategyType) -> Call {
    let (key, value) = strategy.field();
    Call::get("/fapi/v3/strategyOpenOrder").field(key, value).field("strategyType", strategy_type.as_str()).signed()
}

/// A strategy still working.
pub async fn open_strategy(
    transport: &impl Transport,
    strategy: &StrategyRef,
    strategy_type: StrategyType,
) -> Result<StrategyOrder, ApiError> {
    read(transport, open_strategy_call(strategy, strategy_type)).await
}

/// What `strategyHistoryOrder` is asked for besides the strategy: the window (90 days back at
/// most) and the count (500 when `None`, 1,000 at most).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryQuery {
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub limit: Option<u32>,
}

/// `GET /fapi/v3/strategyHistoryOrder`.
#[must_use]
pub fn strategy_history_call(strategy: &StrategyRef, strategy_type: StrategyType, query: &HistoryQuery) -> Call {
    let (key, value) = strategy.field();
    Call::get("/fapi/v3/strategyHistoryOrder")
        .field(key, value)
        .field("strategyType", strategy_type.as_str())
        .maybe("startTime", query.start_ms)
        .maybe("endTime", query.end_ms)
        .maybe("limit", query.limit)
        .signed()
}

/// A strategy that has ended.
pub async fn strategy_history(
    transport: &impl Transport,
    strategy: &StrategyRef,
    strategy_type: StrategyType,
    query: &HistoryQuery,
) -> Result<StrategyOrder, ApiError> {
    read(transport, strategy_history_call(strategy, strategy_type, query)).await
}

#[cfg(test)]
#[path = "tests/strategy.rs"]
mod tests;
