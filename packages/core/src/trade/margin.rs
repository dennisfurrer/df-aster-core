//! A position's leverage and margin: the initial leverage on a symbol
//! (`POST /fapi/v3/leverage`), cross or isolated margin (`POST /fapi/v3/marginType`), margin
//! added to or taken from an isolated position (`POST /fapi/v3/positionMargin`), and the history
//! of those changes (`GET /fapi/v3/positionMargin/history`).

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::client::{ApiError, Call, Transport, read};
use crate::trade::error::{Invalid, TradeError};
use crate::utils::{Dec, Done, MarginType, PositionSide, lenient};

/// The highest initial leverage the venue takes; a symbol's brackets may allow less.
pub const MAX_LEVERAGE: u32 = 125;

/// The longest `clientTranId` a margin change may carry.
pub const TRAN_ID_MAX: usize = 64;

/// What a leverage change answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leverage {
    pub symbol: String,
    pub leverage: u32,
    /// The largest position notional at this leverage.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_notional_value: Dec,
}

/// `POST /fapi/v3/leverage`.
pub fn set_leverage_call(symbol: &str, leverage: u32) -> Result<Call, Invalid> {
    if leverage == 0 || leverage > MAX_LEVERAGE {
        return Err(Invalid::OutOfRange { param: "leverage", value: leverage.to_string(), range: "1 to 125" });
    }
    Ok(Call::post("/fapi/v3/leverage").field("symbol", symbol).field("leverage", leverage).signed())
}

/// Sets the initial leverage on `symbol`, 1 to 125.
pub async fn set_leverage(transport: &impl Transport, symbol: &str, leverage: u32) -> Result<Leverage, TradeError> {
    Ok(read(transport, set_leverage_call(symbol, leverage)?).await?)
}

/// `POST /fapi/v3/marginType`.
#[must_use]
pub fn set_margin_type_call(symbol: &str, margin: &MarginType) -> Call {
    Call::post("/fapi/v3/marginType").field("symbol", symbol).field("marginType", margin).signed()
}

/// Sets `symbol` to cross or isolated margin. Refused while it has a position or open orders
/// (-4047, -4048), and with -4046 when it already is.
pub async fn set_margin_type(transport: &impl Transport, symbol: &str, margin: &MarginType) -> Result<(), ApiError> {
    read::<Done>(transport, set_margin_type_call(symbol, margin)).await.map(|_| ())
}

/// Which way margin moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarginChange {
    Add,
    Reduce,
}

impl MarginChange {
    /// As the venue writes it: 1 adds, 2 reduces.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            MarginChange::Add => 1,
            MarginChange::Reduce => 2,
        }
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Option<MarginChange> {
        match code {
            1 => Some(MarginChange::Add),
            2 => Some(MarginChange::Reduce),
            _ => None,
        }
    }
}

/// What a margin change answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarginChanged {
    #[serde(default, deserialize_with = "number_dec")]
    pub amount: Dec,
    #[serde(default, rename = "type")]
    pub change: u8,
    #[serde(default, deserialize_with = "lenient::text")]
    pub msg: String,
}

/// `POST /fapi/v3/positionMargin`.
pub fn change_margin_call(
    symbol: &str,
    position_side: Option<&PositionSide>,
    amount: Dec,
    change: MarginChange,
    client_tran_id: Option<&str>,
) -> Result<Call, Invalid> {
    if !amount.is_positive() {
        return Err(Invalid::OutOfRange { param: "amount", value: amount.to_wire(), range: "above zero" });
    }
    if let Some(id) = client_tran_id.filter(|id| id.is_empty() || id.len() > TRAN_ID_MAX) {
        return Err(Invalid::ClientId(id.to_owned()));
    }
    Ok(Call::post("/fapi/v3/positionMargin")
        .field("symbol", symbol)
        .maybe("positionSide", position_side)
        .field("amount", amount.to_wire())
        .field("type", change.code())
        .maybe("clientTranId", client_tran_id)
        .signed())
}

/// Adds margin to an isolated position, or takes some away. `client_tran_id` makes it safe to
/// send again: the venue refuses the same id for the same symbol within seven days.
pub async fn change_margin(
    transport: &impl Transport,
    symbol: &str,
    position_side: Option<&PositionSide>,
    amount: Dec,
    change: MarginChange,
    client_tran_id: Option<&str>,
) -> Result<MarginChanged, TradeError> {
    let call = change_margin_call(symbol, position_side, amount, change, client_tran_id)?;
    Ok(read(transport, call).await?)
}

/// One change to a position's margin.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginEntry {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub asset: String,
    #[serde(deserialize_with = "number_dec")]
    pub amount: Dec,
    /// 1 added, 2 reduced.
    #[serde(rename = "type")]
    pub change: u8,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    /// `TRADE` or `USER_ADJUST`, as the venue writes it.
    #[serde(default, deserialize_with = "lenient::text")]
    pub delta_type: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub client_tran_id: String,
    pub time: u64,
}

fn both() -> PositionSide {
    PositionSide::Both
}

/// What `positionMargin/history` is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarginQuery {
    pub change: Option<MarginChange>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    /// 500 when `None`.
    pub limit: Option<u32>,
}

/// `GET /fapi/v3/positionMargin/history`.
#[must_use]
pub fn margin_history_call(symbol: &str, query: &MarginQuery) -> Call {
    Call::get("/fapi/v3/positionMargin/history")
        .field("symbol", symbol)
        .maybe("type", query.change.map(MarginChange::code))
        .maybe("startTime", query.start_ms)
        .maybe("endTime", query.end_ms)
        .maybe("limit", query.limit)
        .signed()
}

/// The changes to `symbol`'s margin, oldest first.
pub async fn margin_history(
    transport: &impl Transport,
    symbol: &str,
    query: &MarginQuery,
) -> Result<Vec<MarginEntry>, ApiError> {
    read(transport, margin_history_call(symbol, query)).await
}

/// A decimal written as a string, or as a bare JSON number (`100.0`), read from its text so no
/// float is computed with.
fn number_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    let text = match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => return Ok(Dec::ZERO),
        Some(Value::String(text)) => text,
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => return Err(serde::de::Error::custom(format!("not a decimal: {other}"))),
    };
    Dec::parse(&text).map_err(serde::de::Error::custom)
}

#[cfg(test)]
#[path = "tests/margin.rs"]
mod tests;
