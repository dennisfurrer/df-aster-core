//! An order as the venue answers it: the row a new order, a cancel, a query, the open orders and
//! the order history all come back as. Fields only some answers carry are `Option`s; the
//! venue's decimals stay exact.

use serde::Deserialize;

use crate::utils::lenient;
use crate::utils::{Dec, OrderStatus, OrderType, PegPriceType, PositionSide, Side, StpMode, TimeInForce, WorkingType};

/// One order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub symbol: String,
    pub order_id: u64,
    #[serde(default, deserialize_with = "lenient::text")]
    pub client_order_id: String,
    pub side: Side,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    /// The type it was placed as: a stop that triggered is `MARKET` in `order_type` and
    /// `STOP_MARKET` here.
    #[serde(default)]
    pub orig_type: Option<OrderType>,
    pub status: OrderStatus,
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
    /// The limit price; zero for a market order.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    /// The average fill price; zero before any fill.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub avg_price: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub orig_qty: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub executed_qty: Dec,
    /// The quote filled: price times quantity, summed over the fills.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub cum_quote: Dec,
    /// The trigger of a stop or take-profit; zero otherwise, and to be ignored for a trailing
    /// stop.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub stop_price: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub reduce_only: bool,
    /// A stop that closes the whole position.
    #[serde(default, deserialize_with = "lenient::flag")]
    pub close_position: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub price_protect: bool,
    #[serde(default)]
    pub working_type: Option<WorkingType>,
    /// A trailing stop's activation price and callback rate (1 is 1%).
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub activate_price: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub price_rate: Option<Dec>,
    #[serde(default, deserialize_with = "blank_enum")]
    pub peg_price_type: Option<PegPriceType>,
    #[serde(default, deserialize_with = "blank_enum")]
    pub stp_mode: Option<StpMode>,
    /// When it was placed, unix ms: queries and history carry it, a new order's answer does not.
    #[serde(default)]
    pub time: Option<u64>,
    #[serde(default)]
    pub update_time: Option<u64>,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// An enum the venue may leave as `""` for none.
fn blank_enum<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let text = Option::<String>::deserialize(deserializer)?.unwrap_or_default();
    match text.trim().is_empty() {
        true => Ok(None),
        false => T::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(text)).map(Some),
    }
}

impl Order {
    /// Whether it still rests or waits.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.status.is_open()
    }

    /// What is left to fill.
    #[must_use]
    pub fn remaining(&self) -> Dec {
        self.orig_qty.checked_sub(self.executed_qty).unwrap_or(Dec::ZERO)
    }
}

#[cfg(test)]
#[path = "tests/order.rs"]
mod tests;
