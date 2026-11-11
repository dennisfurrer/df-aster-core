//! New orders: a [`NewOrder`] built from exact decimals, checked against the docs' rules for its
//! type before it is sent, fitted to a symbol's [`Grid`], and placed one at a time
//! (`POST /fapi/v3/order`) or up to five at once (`POST /fapi/v3/batchOrders`).
//!
//! What each type needs (the docs' "additional mandatory parameters"), and what it may not carry
//! (the venue answers `PARAM_NOT_REQUIRED`, -1106, for a parameter that does not apply):
//!
//! | Type | Needs | Takes no |
//! |---|---|---|
//! | `LIMIT` | `timeInForce`, `quantity`, `price` | `stopPrice`, `closePosition`, trailing, `workingType`, `priceProtect` |
//! | `MARKET` | `quantity` | `timeInForce`, `price`, `stopPrice`, `closePosition`, trailing, `workingType`, `priceProtect`, peg |
//! | `STOP`, `TAKE_PROFIT` | `quantity`, `price`, `stopPrice` | `closePosition`, trailing |
//! | `STOP_MARKET`, `TAKE_PROFIT_MARKET` | `stopPrice`, and `quantity` unless `closePosition` | `price`, trailing, peg; a `timeInForce` other than `GTC` |
//! | `TRAILING_STOP_MARKET` | `callbackRate` (0.1 to 5), `quantity` | `price`, `stopPrice`, `closePosition`, peg; a `timeInForce` other than `GTC` |
//!
//! `closePosition` takes neither `quantity` nor `reduceOnly`; `reduceOnly` is not sent in hedge
//! mode (a `LONG` or `SHORT` position side). A BBO peg (`pegPriceType`) goes on a `LIMIT`, `STOP`
//! or `TAKE_PROFIT` order, whose price the engine then takes from the book.
//!
//! The fields go out in one fixed order, the one Aster's web app writes (`symbol`, `side`,
//! `type`, `quantity`, `price`, `timeInForce`, `reduceOnly`, `newOrderRespType`), so an order
//! signs to the same bytes as it does there.

use serde_json::Value;

use crate::client::{ApiError, Call, Transport, read};
use crate::trade::Order;
use crate::trade::error::{Invalid, TradeError, valid_client_id};
use crate::utils::{
    Address, Dec, Grid, OrderType, PegPriceType, PositionSide, ResponseType, Round, Side, StpMode, TimeInForce,
    WorkingType,
};

/// The most orders one `batchOrders` carries.
pub const MOST_IN_BATCH: usize = 5;

/// The longest client order id.
pub const CLIENT_ID_MAX: usize = 36;

/// A trailing stop's callback rate, in percent: at least 0.1, at most 5.
pub const CALLBACK_RATE_MIN: Dec = Dec::lit::<1>(1);
pub const CALLBACK_RATE_MAX: Dec = Dec::lit::<0>(5);

/// An order to place. Build it with one of the constructors, add what else it carries, and
/// [`place`] it; [`NewOrder::fit`] puts it on the symbol's grid first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOrder {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    /// `None` sends nothing: `BOTH` in one-way mode. Hedge mode needs `LONG` or `SHORT`.
    pub position_side: Option<PositionSide>,
    pub quantity: Option<Dec>,
    pub price: Option<Dec>,
    pub stop_price: Option<Dec>,
    pub time_in_force: Option<TimeInForce>,
    pub reduce_only: Option<bool>,
    /// A stop or take-profit that closes the whole position when it triggers.
    pub close_position: bool,
    /// A trailing stop's activation price; the latest price when `None`.
    pub activation_price: Option<Dec>,
    /// A trailing stop's callback rate in percent: 1 is 1%.
    pub callback_rate: Option<Dec>,
    pub working_type: Option<WorkingType>,
    pub price_protect: Option<bool>,
    pub client_id: Option<String>,
    pub peg: Option<PegPriceType>,
    pub stp_mode: Option<StpMode>,
    /// `Ack` at once (the venue's default), or `Result` once a market or IOC order is done.
    pub response: Option<ResponseType>,
    /// The builder an order is placed through, and the fee rate it charges (0.0001 is 0.01%).
    /// The user must have approved the builder for at least that rate, or the venue answers
    /// `INVALID_BUILDER_PARAMETER` (-2031).
    pub builder: Option<Address>,
    pub fee_rate: Option<Dec>,
}

impl NewOrder {
    fn new(symbol: &str, side: Side, order_type: OrderType) -> NewOrder {
        NewOrder {
            symbol: symbol.to_owned(),
            side,
            order_type,
            position_side: None,
            quantity: None,
            price: None,
            stop_price: None,
            time_in_force: None,
            reduce_only: None,
            close_position: false,
            activation_price: None,
            callback_rate: None,
            working_type: None,
            price_protect: None,
            client_id: None,
            peg: None,
            stp_mode: None,
            response: None,
            builder: None,
            fee_rate: None,
        }
    }

    /// A limit order resting at `price` until `time_in_force` ends it.
    #[must_use]
    pub fn limit(symbol: &str, side: Side, quantity: Dec, price: Dec, time_in_force: TimeInForce) -> NewOrder {
        NewOrder {
            quantity: Some(quantity),
            price: Some(price),
            time_in_force: Some(time_in_force),
            ..NewOrder::new(symbol, side, OrderType::Limit)
        }
    }

    /// A limit order whose price the engine takes from the book at placement (`pegPriceType`).
    #[must_use]
    pub fn pegged_limit(
        symbol: &str,
        side: Side,
        quantity: Dec,
        peg: PegPriceType,
        time_in_force: TimeInForce,
    ) -> NewOrder {
        NewOrder {
            quantity: Some(quantity),
            peg: Some(peg),
            time_in_force: Some(time_in_force),
            ..NewOrder::new(symbol, side, OrderType::Limit)
        }
    }

    /// A market order for `quantity` of the base.
    #[must_use]
    pub fn market(symbol: &str, side: Side, quantity: Dec) -> NewOrder {
        NewOrder { quantity: Some(quantity), ..NewOrder::new(symbol, side, OrderType::Market) }
    }

    /// A limit order at `price`, placed when the price reaches `stop_price`: a stop loss.
    #[must_use]
    pub fn stop(symbol: &str, side: Side, quantity: Dec, price: Dec, stop_price: Dec) -> NewOrder {
        NewOrder {
            quantity: Some(quantity),
            price: Some(price),
            stop_price: Some(stop_price),
            ..NewOrder::new(symbol, side, OrderType::Stop)
        }
    }

    /// A limit order at `price`, placed when the price reaches `stop_price`: a take-profit.
    #[must_use]
    pub fn take_profit(symbol: &str, side: Side, quantity: Dec, price: Dec, stop_price: Dec) -> NewOrder {
        NewOrder { order_type: OrderType::TakeProfit, ..NewOrder::stop(symbol, side, quantity, price, stop_price) }
    }

    /// A market order placed when the price reaches `stop_price`: a stop loss.
    #[must_use]
    pub fn stop_market(symbol: &str, side: Side, quantity: Dec, stop_price: Dec) -> NewOrder {
        NewOrder {
            quantity: Some(quantity),
            stop_price: Some(stop_price),
            ..NewOrder::new(symbol, side, OrderType::StopMarket)
        }
    }

    /// A market order placed when the price reaches `stop_price`: a take-profit.
    #[must_use]
    pub fn take_profit_market(symbol: &str, side: Side, quantity: Dec, stop_price: Dec) -> NewOrder {
        NewOrder {
            order_type: OrderType::TakeProfitMarket,
            ..NewOrder::stop_market(symbol, side, quantity, stop_price)
        }
    }

    /// A stop that closes the whole position when the price reaches `stop_price`: a `SELL`
    /// closes a long, a `BUY` a short.
    #[must_use]
    pub fn close_stop_market(symbol: &str, side: Side, stop_price: Dec) -> NewOrder {
        NewOrder {
            stop_price: Some(stop_price),
            close_position: true,
            ..NewOrder::new(symbol, side, OrderType::StopMarket)
        }
    }

    /// A take-profit that closes the whole position when the price reaches `stop_price`.
    #[must_use]
    pub fn close_take_profit_market(symbol: &str, side: Side, stop_price: Dec) -> NewOrder {
        NewOrder { order_type: OrderType::TakeProfitMarket, ..NewOrder::close_stop_market(symbol, side, stop_price) }
    }

    /// A market order placed once the price turns back by `callback_rate` percent from its best
    /// since the order (or since `activation_price`, with [`NewOrder::activation_price`]).
    #[must_use]
    pub fn trailing_stop_market(symbol: &str, side: Side, quantity: Dec, callback_rate: Dec) -> NewOrder {
        NewOrder {
            quantity: Some(quantity),
            callback_rate: Some(callback_rate),
            ..NewOrder::new(symbol, side, OrderType::TrailingStopMarket)
        }
    }

    #[must_use]
    pub fn position_side(mut self, side: PositionSide) -> NewOrder {
        self.position_side = Some(side);
        self
    }

    #[must_use]
    pub const fn reduce_only(mut self, reduce_only: bool) -> NewOrder {
        self.reduce_only = Some(reduce_only);
        self
    }

    #[must_use]
    pub fn time_in_force(mut self, time_in_force: TimeInForce) -> NewOrder {
        self.time_in_force = Some(time_in_force);
        self
    }

    #[must_use]
    pub const fn activation_price(mut self, price: Dec) -> NewOrder {
        self.activation_price = Some(price);
        self
    }

    #[must_use]
    pub fn working_type(mut self, working_type: WorkingType) -> NewOrder {
        self.working_type = Some(working_type);
        self
    }

    #[must_use]
    pub const fn price_protect(mut self, protect: bool) -> NewOrder {
        self.price_protect = Some(protect);
        self
    }

    /// The client order id, unique among the account's open orders.
    #[must_use]
    pub fn client_id(mut self, id: impl Into<String>) -> NewOrder {
        self.client_id = Some(id.into());
        self
    }

    #[must_use]
    pub fn peg(mut self, peg: PegPriceType) -> NewOrder {
        self.peg = Some(peg);
        self
    }

    #[must_use]
    pub fn stp_mode(mut self, mode: StpMode) -> NewOrder {
        self.stp_mode = Some(mode);
        self
    }

    #[must_use]
    pub fn response(mut self, response: ResponseType) -> NewOrder {
        self.response = Some(response);
        self
    }

    /// Placed through `builder`, which charges `fee_rate` (0.0001 is 0.01%).
    #[must_use]
    pub const fn builder(mut self, builder: Address, fee_rate: Dec) -> NewOrder {
        self.builder = Some(builder);
        self.fee_rate = Some(fee_rate);
        self
    }

    /// Whether its quantity follows `MARKET_LOT_SIZE`: it fills as a market order.
    #[must_use]
    pub const fn fills_at_market(&self) -> bool {
        matches!(
            self.order_type,
            OrderType::Market | OrderType::StopMarket | OrderType::TakeProfitMarket | OrderType::TrailingStopMarket
        )
    }

    /// The docs' rules for its type, and the ranges the venue takes.
    pub fn check(&self) -> Result<(), Invalid> {
        let ty = &self.order_type;
        let missing = |param| Invalid::Missing { order_type: ty.clone(), param };
        let not_taken = |param| Invalid::NotTaken { order_type: ty.clone(), param };
        let trailing = self.activation_price.is_some() || self.callback_rate.is_some();
        let gtc_only = |tif: &Option<TimeInForce>| match tif {
            Some(tif) if *tif != TimeInForce::Gtc => Err(not_taken("timeInForce other than GTC")),
            _ => Ok(()),
        };
        match ty {
            OrderType::Limit => {
                none(&self.stop_price, not_taken("stopPrice"))?;
                need(&self.time_in_force, missing("timeInForce"))?;
                need(&self.quantity, missing("quantity"))?;
                if self.peg.is_none() {
                    need(&self.price, missing("price"))?;
                }
                no_trigger_fields(self, not_taken)?;
            }
            OrderType::Market => {
                need(&self.quantity, missing("quantity"))?;
                none(&self.time_in_force, not_taken("timeInForce"))?;
                none(&self.price, not_taken("price"))?;
                none(&self.stop_price, not_taken("stopPrice"))?;
                none(&self.peg, not_taken("pegPriceType"))?;
                no_trigger_fields(self, not_taken)?;
            }
            OrderType::Stop | OrderType::TakeProfit => {
                need(&self.quantity, missing("quantity"))?;
                if self.peg.is_none() {
                    need(&self.price, missing("price"))?;
                }
                need(&self.stop_price, missing("stopPrice"))?;
                if self.close_position {
                    return Err(not_taken("closePosition"));
                }
                if trailing {
                    return Err(not_taken("activationPrice or callbackRate"));
                }
            }
            OrderType::StopMarket | OrderType::TakeProfitMarket => {
                need(&self.stop_price, missing("stopPrice"))?;
                if !self.close_position {
                    need(&self.quantity, missing("quantity"))?;
                }
                none(&self.price, not_taken("price"))?;
                none(&self.peg, not_taken("pegPriceType"))?;
                gtc_only(&self.time_in_force)?;
                if trailing {
                    return Err(not_taken("activationPrice or callbackRate"));
                }
            }
            OrderType::TrailingStopMarket => {
                let rate = self.callback_rate.ok_or_else(|| missing("callbackRate"))?;
                if rate < CALLBACK_RATE_MIN || rate > CALLBACK_RATE_MAX {
                    return Err(Invalid::OutOfRange {
                        param: "callbackRate",
                        value: rate.to_wire(),
                        range: "0.1 to 5 percent",
                    });
                }
                need(&self.quantity, missing("quantity"))?;
                none(&self.price, not_taken("price"))?;
                none(&self.stop_price, not_taken("stopPrice"))?;
                none(&self.peg, not_taken("pegPriceType"))?;
                if self.close_position {
                    return Err(not_taken("closePosition"));
                }
                gtc_only(&self.time_in_force)?;
            }
            OrderType::Liquidation | OrderType::Other(_) => {
                return Err(Invalid::OutOfRange {
                    param: "type",
                    value: ty.to_string(),
                    range: "LIMIT, MARKET, STOP, STOP_MARKET, TAKE_PROFIT, TAKE_PROFIT_MARKET, TRAILING_STOP_MARKET",
                });
            }
        }
        if self.close_position {
            if self.quantity.is_some() {
                return Err(Invalid::Conflict("closePosition", "quantity"));
            }
            if self.reduce_only.is_some() {
                return Err(Invalid::Conflict("closePosition", "reduceOnly"));
            }
        }
        if self.reduce_only.is_some() && matches!(self.position_side, Some(PositionSide::Long | PositionSide::Short)) {
            return Err(Invalid::Conflict("reduceOnly", "a hedge mode positionSide"));
        }
        for (param, value) in [
            ("quantity", self.quantity),
            ("price", self.price),
            ("stopPrice", self.stop_price),
            ("activationPrice", self.activation_price),
        ] {
            if let Some(value) = value.filter(|value| !value.is_positive()) {
                return Err(Invalid::OutOfRange { param, value: value.to_wire(), range: "above zero" });
            }
        }
        if let Some(id) = self.client_id.as_deref().filter(|id| !valid_client_id(id, CLIENT_ID_MAX)) {
            return Err(Invalid::ClientId(id.to_owned()));
        }
        match (&self.builder, self.fee_rate) {
            (None, Some(_)) => Err(Invalid::Conflict("feeRate", "no builder")),
            (_, Some(rate)) if rate.is_negative() => {
                Err(Invalid::OutOfRange { param: "feeRate", value: rate.to_wire(), range: "zero or above" })
            }
            _ => Ok(()),
        }
    }

    /// The order on `grid`: prices to the tick (rounded as `round` says), the quantity down to
    /// the step (`MARKET_LOT_SIZE` for an order that fills at market), within the bounds, and
    /// at least the least notional at its price or trigger. A reduce-only or close-position order
    /// is held to no notional; a market order has no price here to hold it to.
    pub fn fit(mut self, grid: &Grid, round: Round) -> Result<NewOrder, Invalid> {
        for price in [&mut self.price, &mut self.stop_price, &mut self.activation_price] {
            if let Some(value) = *price {
                *price = Some(grid.price(value, round)?);
            }
        }
        if let Some(quantity) = self.quantity {
            let fitted = match self.fills_at_market() {
                true => grid.market_quantity(quantity)?,
                false => grid.quantity(quantity)?,
            };
            self.quantity = Some(fitted);
        }
        let exempt = self.reduce_only == Some(true) || self.close_position;
        if let (Some(quantity), Some(price), false) = (self.quantity, self.price.or(self.stop_price), exempt) {
            grid.check_notional(price, quantity)?;
        }
        Ok(self)
    }

    /// The fields as they go out, checked.
    pub fn fields(&self) -> Result<Vec<(&'static str, String)>, Invalid> {
        self.check()?;
        let mut fields: Vec<(&'static str, String)> =
            vec![("symbol", self.symbol.clone()), ("side", self.side.to_string())];
        let mut put = |key: &'static str, value: Option<String>| {
            if let Some(value) = value {
                fields.push((key, value));
            }
        };
        let wire = |value: Option<Dec>| value.map(Dec::to_wire);
        put("positionSide", self.position_side.as_ref().map(ToString::to_string));
        put("type", Some(self.order_type.to_string()));
        put("quantity", wire(self.quantity));
        put("price", wire(self.price));
        put("stopPrice", wire(self.stop_price));
        put("timeInForce", self.time_in_force.as_ref().map(ToString::to_string));
        put("reduceOnly", self.reduce_only.map(|flag| flag.to_string()));
        put("closePosition", self.close_position.then(|| "true".to_owned()));
        put("activationPrice", wire(self.activation_price));
        put("callbackRate", wire(self.callback_rate));
        put("workingType", self.working_type.as_ref().map(ToString::to_string));
        put("priceProtect", self.price_protect.map(|flag| if flag { "TRUE" } else { "FALSE" }.to_owned()));
        put("newClientOrderId", self.client_id.clone());
        put("pegPriceType", self.peg.as_ref().map(ToString::to_string));
        put("stpMode", self.stp_mode.as_ref().map(ToString::to_string));
        put("newOrderRespType", self.response.as_ref().map(ToString::to_string));
        put("builder", self.builder.map(Address::to_checksum));
        put("feeRate", wire(self.fee_rate));
        Ok(fields)
    }
}

fn need<T>(value: &Option<T>, otherwise: Invalid) -> Result<(), Invalid> {
    match value {
        Some(_) => Ok(()),
        None => Err(otherwise),
    }
}

fn none<T>(value: &Option<T>, otherwise: Invalid) -> Result<(), Invalid> {
    match value {
        Some(_) => Err(otherwise),
        None => Ok(()),
    }
}

/// What only a conditional order carries: a limit or market order takes none of it.
fn no_trigger_fields(order: &NewOrder, not_taken: impl Fn(&'static str) -> Invalid) -> Result<(), Invalid> {
    if order.close_position {
        return Err(not_taken("closePosition"));
    }
    none(&order.activation_price, not_taken("activationPrice"))?;
    none(&order.callback_rate, not_taken("callbackRate"))?;
    none(&order.working_type, not_taken("workingType"))?;
    none(&order.price_protect, not_taken("priceProtect"))
}

/// `POST /fapi/v3/order`.
pub fn place_call(order: &NewOrder) -> Result<Call, Invalid> {
    let mut call = Call::post("/fapi/v3/order").signed().places(1);
    call.fields = order.fields()?;
    Ok(call)
}

/// Places one order. Checked here first; never retried. An answer that never came
/// ([`TradeError::may_have_landed`]) means the order may be live: read the open orders.
pub async fn place(transport: &impl Transport, order: &NewOrder) -> Result<Order, TradeError> {
    Ok(read(transport, place_call(order)?).await?)
}

/// `POST /fapi/v3/batchOrders`: one to five orders, as a JSON list.
pub fn place_batch_call(orders: &[NewOrder]) -> Result<Call, Invalid> {
    count("batchOrders", orders.len(), MOST_IN_BATCH, "1 to 5")?;
    let rows = orders.iter().map(NewOrder::fields).collect::<Result<Vec<_>, _>>()?;
    let n = u32::try_from(orders.len()).unwrap_or(u32::MAX);
    Ok(Call::post("/fapi/v3/batchOrders").field("batchOrders", json_list(&rows)).signed().places(n))
}

/// Places one to five orders. The venue takes or refuses each on its own; the answer holds one
/// row an order, in the order sent: the order, or why it was refused.
pub async fn place_batch(
    transport: &impl Transport,
    orders: &[NewOrder],
) -> Result<Vec<Result<Order, ApiError>>, TradeError> {
    let call = place_batch_call(orders)?;
    let path = call.path;
    let answer: Vec<Value> = read(transport, call).await?;
    Ok(rows(path, answer))
}

pub(crate) fn count(what: &'static str, count: usize, most: usize, range: &'static str) -> Result<(), Invalid> {
    match count == 0 || count > most {
        true => Err(Invalid::Count { what, count, range }),
        false => Ok(()),
    }
}

/// Objects of string fields, in order, as a JSON list with no spaces: what `batchOrders` and a
/// strategy's `subOrderList` carry.
pub(crate) fn json_list(rows: &[Vec<(&'static str, String)>]) -> String {
    let objects: Vec<String> = rows
        .iter()
        .map(|fields| {
            let pairs: Vec<String> =
                fields.iter().map(|(key, value)| format!("{}:{}", quoted(key), quoted(value))).collect();
            format!("{{{}}}", pairs.join(","))
        })
        .collect();
    format!("[{}]", objects.join(","))
}

/// A JSON string.
pub(crate) fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// A batch's answer, a row an order: an order, or a refusal (`{code, msg}`) in its place.
pub(crate) fn rows(path: &str, answer: Vec<Value>) -> Vec<Result<Order, ApiError>> {
    answer
        .into_iter()
        .map(|row| {
            if row.get("orderId").is_none()
                && let Some(code) = row.get("code").and_then(code_of)
            {
                let message = row.get("msg").and_then(Value::as_str).unwrap_or_default().to_owned();
                return Err(ApiError::Refused { code, message });
            }
            let text = row.to_string();
            serde_json::from_value(row)
                .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: format!("{error} in {text}") })
        })
        .collect()
}

fn code_of(code: &Value) -> Option<i64> {
    match code {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "tests/orders.rs"]
mod tests;
