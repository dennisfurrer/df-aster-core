//! Frames the socket receives, read into [`Frame`]s: a stream's event, from a combined socket
//! (`{"stream": .., "data": ..}`) or a raw one; a request's answer (`{"result": .., "id": n}`); or
//! its refusal (`{"error": {"code", "msg"}, "id": n}`). An event is told by its `e`; one this
//! build does not know is kept whole as [`Data::Other`], and a field the venue adds is ignored.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::live::Asked;
use crate::live::account::{AccountUpdate, Announcement, ConfigUpdate, ListenKeyExpired, MarginCall, OrderUpdate};
use crate::live::market::{
    AggTrade, BookTicker, DepthUpdate, Kline, Liquidation, MarkPrice, MiniTicker, Ticker, TradePro,
};

/// One frame, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A stream's event. `stream` is the name a combined socket wraps it with; `None` on a raw
    /// socket (`/ws/<name>`).
    Data { stream: Option<String>, data: Data },
    /// A request's answer: `null` for a subscribe, the names for a list, a flag for a property.
    /// `asked` is what the request was, when a [`crate::live::Session`] sent it.
    Answer { id: u64, result: Value, asked: Option<Asked> },
    /// A request's refusal.
    Refusal { id: Option<u64>, code: i64, message: String, asked: Option<Asked> },
    /// A diff book update that does not follow the last on this socket: a book kept from it
    /// needs a fresh snapshot. The update is kept: it belongs to the new chain.
    Gap { gap: Gap, update: DepthUpdate },
}

impl Frame {
    /// The event, when it is one.
    #[must_use]
    pub const fn data(&self) -> Option<&Data> {
        match self {
            Frame::Data { data, .. } => Some(data),
            _ => None,
        }
    }
}

/// Where a diff book's chain broke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    /// The stream's name.
    pub stream: String,
    /// The last update's `u`, which this one's `pu` should have been.
    pub expected: u64,
    /// This one's `pu`.
    pub got: u64,
}

/// A stream's event. The largest are boxed, so a small one does not carry their size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data {
    AggTrade(AggTrade),
    MarkPrice(MarkPrice),
    /// `!markPrice@arr`.
    MarkPrices(Vec<MarkPrice>),
    Kline(Box<Kline>),
    MiniTicker(MiniTicker),
    /// `!miniTicker@arr`: the symbols that changed.
    MiniTickers(Vec<MiniTicker>),
    Ticker(Box<Ticker>),
    /// `!ticker@arr`: the symbols that changed.
    Tickers(Vec<Ticker>),
    BookTicker(BookTicker),
    Liquidation(Box<Liquidation>),
    Depth(DepthUpdate),
    TradePro(TradePro),
    ListenKeyExpired(ListenKeyExpired),
    MarginCall(MarginCall),
    AccountUpdate(AccountUpdate),
    OrderUpdate(Box<OrderUpdate>),
    ConfigUpdate(ConfigUpdate),
    Announcement(Announcement),
    /// An event this build does not know, by its `e`, kept whole.
    Other {
        event: String,
        raw: Value,
    },
}

/// A frame that did not read. The socket is still alive.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a frame did not read ({reason}): {excerpt}")]
pub struct BadFrame {
    pub reason: String,
    /// Its first 200 characters.
    pub excerpt: String,
}

impl BadFrame {
    fn new(reason: impl ToString, text: &str) -> Self {
        BadFrame { reason: reason.to_string(), excerpt: text.chars().take(200).collect() }
    }
}

/// Reads one frame's text.
pub fn decode(text: &str) -> Result<Frame, BadFrame> {
    let value: Value = serde_json::from_str(text).map_err(|error| BadFrame::new(error, text))?;
    if let Value::Object(object) = &value {
        if let Some(error) = object.get("error") {
            let id = object.get("id").and_then(Value::as_u64);
            return Ok(refusal(id, error));
        }
        if object.contains_key("result") {
            let id =
                object.get("id").and_then(Value::as_u64).ok_or_else(|| BadFrame::new("an answer with no id", text))?;
            return Ok(Frame::Answer { id, result: object.get("result").cloned().unwrap_or(Value::Null), asked: None });
        }
        if let (Some(Value::String(stream)), Some(data)) = (object.get("stream"), object.get("data")) {
            let data = data_of(data, Some(stream)).map_err(|reason| BadFrame::new(reason, text))?;
            return Ok(Frame::Data { stream: Some(stream.clone()), data });
        }
        if !object.contains_key("e") && object.contains_key("code") && object.contains_key("msg") {
            return Ok(refusal(object.get("id").and_then(Value::as_u64), &value));
        }
    }
    let data = data_of(&value, None).map_err(|reason| BadFrame::new(reason, text))?;
    Ok(Frame::Data { stream: None, data })
}

fn refusal(id: Option<u64>, error: &Value) -> Frame {
    let code = error.get("code").and_then(Value::as_i64).unwrap_or_default();
    let message = error.get("msg").and_then(Value::as_str).unwrap_or_default().to_owned();
    Frame::Refusal { id, code, message, asked: None }
}

/// An event's payload, by its `e`; an array by its first element's, or by the stream's name when
/// it is empty.
fn data_of(value: &Value, stream: Option<&str>) -> Result<Data, String> {
    if let Value::Array(items) = value {
        let event = items
            .first()
            .and_then(|first| first.get("e"))
            .and_then(Value::as_str)
            .or_else(|| match stream.unwrap_or_default() {
                name if name.starts_with("!markPrice@arr") => Some("markPriceUpdate"),
                "!miniTicker@arr" => Some("24hrMiniTicker"),
                "!ticker@arr" => Some("24hrTicker"),
                _ => None,
            })
            .unwrap_or_default();
        return Ok(match event {
            "markPriceUpdate" => Data::MarkPrices(typed(value)?),
            "24hrMiniTicker" => Data::MiniTickers(typed(value)?),
            "24hrTicker" => Data::Tickers(typed(value)?),
            other => Data::Other { event: other.to_owned(), raw: value.clone() },
        });
    }
    let event = value.get("e").and_then(Value::as_str).unwrap_or_default();
    Ok(match event {
        "aggTrade" => Data::AggTrade(typed(value)?),
        "markPriceUpdate" => Data::MarkPrice(typed(value)?),
        "kline" => Data::Kline(Box::new(typed(value)?)),
        "24hrMiniTicker" => Data::MiniTicker(typed(value)?),
        "24hrTicker" => Data::Ticker(Box::new(typed(value)?)),
        "bookTicker" => Data::BookTicker(typed(value)?),
        "forceOrder" => Data::Liquidation(Box::new(typed(value)?)),
        "depthUpdate" => Data::Depth(typed(value)?),
        "tradepro" => Data::TradePro(typed(value)?),
        "listenKeyExpired" => Data::ListenKeyExpired(typed(value)?),
        "MARGIN_CALL" => Data::MarginCall(typed(value)?),
        "ACCOUNT_UPDATE" => Data::AccountUpdate(typed(value)?),
        "ORDER_TRADE_UPDATE" => Data::OrderUpdate(Box::new(typed(value)?)),
        "ACCOUNT_CONFIG_UPDATE" => Data::ConfigUpdate(typed(value)?),
        "announcement" => Data::Announcement(typed(value)?),
        other => Data::Other { event: other.to_owned(), raw: value.clone() },
    })
}

fn typed<T: DeserializeOwned>(value: &Value) -> Result<T, String> {
    T::deserialize(value).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "tests/frame.rs"]
mod tests;
