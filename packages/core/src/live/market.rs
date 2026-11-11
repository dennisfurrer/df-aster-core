//! What the market streams send, read with exact decimals. The venue writes each field under one
//! letter or two; each is named here in full, with the venue's key beside it. A field the venue
//! adds later is ignored, and one it leaves out reads as zero or none.

use serde::{Deserialize, Deserializer};

use crate::utils::lenient;
use crate::utils::{Dec, Interval, OrderStatus, OrderType, Side, TimeInForce};

/// A trade, aggregated per taker order (`aggTrade`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AggTrade {
    /// `E`: when the venue sent it, unix ms.
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    /// `a`: the aggregate trade's id.
    #[serde(rename = "a", default)]
    pub id: u64,
    #[serde(rename = "p")]
    pub price: Dec,
    #[serde(rename = "q")]
    pub quantity: Dec,
    /// `f` and `l`: the first and last trade ids it covers.
    #[serde(rename = "f", default)]
    pub first_trade_id: u64,
    #[serde(rename = "l", default)]
    pub last_trade_id: u64,
    /// `T`: when it traded, unix ms.
    #[serde(rename = "T", default)]
    pub trade_time: u64,
    /// `m`: the buyer was the maker, so the taker sold.
    #[serde(rename = "m", default)]
    pub buyer_is_maker: bool,
}

impl AggTrade {
    /// The taker's side.
    #[must_use]
    pub const fn taker_side(&self) -> Side {
        if self.buyer_is_maker { Side::Sell } else { Side::Buy }
    }
}

/// A symbol's mark price, index price and funding (`markPriceUpdate`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MarkPrice {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "p")]
    pub mark_price: Dec,
    #[serde(rename = "i", default, deserialize_with = "lenient::zero_dec")]
    pub index_price: Dec,
    /// `P`: the estimated settle price, of use only in the last hour before a settlement.
    #[serde(rename = "P", default, deserialize_with = "lenient::blank_dec")]
    pub settle_price: Option<Dec>,
    /// `r`: the funding rate for the interval, as a fraction.
    #[serde(rename = "r", default, deserialize_with = "lenient::zero_dec")]
    pub funding_rate: Dec,
    /// `T`: the next funding settlement, unix ms.
    #[serde(rename = "T", default)]
    pub next_funding_time: u64,
}

/// A candle as it stands (`kline`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Kline {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "k")]
    pub bar: Bar,
}

/// One candle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bar {
    /// `t` and `T`: its first and last millisecond.
    #[serde(rename = "t")]
    pub open_time: u64,
    #[serde(rename = "T")]
    pub close_time: u64,
    #[serde(rename = "i")]
    pub interval: Interval,
    #[serde(rename = "f", default, deserialize_with = "lenient::int")]
    pub first_trade_id: i64,
    #[serde(rename = "L", default, deserialize_with = "lenient::int")]
    pub last_trade_id: i64,
    #[serde(rename = "o")]
    pub open: Dec,
    #[serde(rename = "h")]
    pub high: Dec,
    #[serde(rename = "l")]
    pub low: Dec,
    #[serde(rename = "c")]
    pub close: Dec,
    /// `v`: base traded; `q`: quote traded.
    #[serde(rename = "v", default, deserialize_with = "lenient::zero_dec")]
    pub volume: Dec,
    #[serde(rename = "q", default, deserialize_with = "lenient::zero_dec")]
    pub quote_volume: Dec,
    /// `n`: trades in it.
    #[serde(rename = "n", default)]
    pub trades: u64,
    /// `x`: it has closed and will not change.
    #[serde(rename = "x", default)]
    pub closed: bool,
    /// `V` and `Q`: base and quote bought by takers.
    #[serde(rename = "V", default, deserialize_with = "lenient::zero_dec")]
    pub taker_buy_volume: Dec,
    #[serde(rename = "Q", default, deserialize_with = "lenient::zero_dec")]
    pub taker_buy_quote_volume: Dec,
}

/// A rolling 24 hours, short form (`24hrMiniTicker`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MiniTicker {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "o")]
    pub open: Dec,
    #[serde(rename = "h")]
    pub high: Dec,
    #[serde(rename = "l")]
    pub low: Dec,
    #[serde(rename = "c")]
    pub close: Dec,
    #[serde(rename = "v", default, deserialize_with = "lenient::zero_dec")]
    pub volume: Dec,
    #[serde(rename = "q", default, deserialize_with = "lenient::zero_dec")]
    pub quote_volume: Dec,
}

/// A rolling 24 hours (`24hrTicker`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ticker {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    /// `p` and `P`: the change in price, and in percent (`-0.518` is -0.518%).
    #[serde(rename = "p", default, deserialize_with = "lenient::zero_dec")]
    pub price_change: Dec,
    #[serde(rename = "P", default, deserialize_with = "lenient::zero_dec")]
    pub price_change_percent: Dec,
    /// `w`: the volume-weighted average price.
    #[serde(rename = "w", default, deserialize_with = "lenient::zero_dec")]
    pub weighted_avg_price: Dec,
    /// `c` and `Q`: the last trade's price and quantity.
    #[serde(rename = "c")]
    pub last_price: Dec,
    #[serde(rename = "Q", default, deserialize_with = "lenient::zero_dec")]
    pub last_quantity: Dec,
    #[serde(rename = "o")]
    pub open: Dec,
    #[serde(rename = "h")]
    pub high: Dec,
    #[serde(rename = "l")]
    pub low: Dec,
    #[serde(rename = "v", default, deserialize_with = "lenient::zero_dec")]
    pub volume: Dec,
    #[serde(rename = "q", default, deserialize_with = "lenient::zero_dec")]
    pub quote_volume: Dec,
    /// `O` and `C`: the window, unix ms.
    #[serde(rename = "O", default)]
    pub open_time: u64,
    #[serde(rename = "C", default)]
    pub close_time: u64,
    /// `F`, `L` and `n`: the first and last trade ids, and the count.
    #[serde(rename = "F", default, deserialize_with = "lenient::int")]
    pub first_trade_id: i64,
    #[serde(rename = "L", default, deserialize_with = "lenient::int")]
    pub last_trade_id: i64,
    #[serde(rename = "n", default)]
    pub trades: u64,
}

/// The best bid and ask (`bookTicker`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BookTicker {
    /// `u`: the book's update id.
    #[serde(rename = "u", default)]
    pub update_id: u64,
    #[serde(rename = "E", default)]
    pub event_time: u64,
    /// `T`: the transaction time, unix ms.
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub bid_price: Dec,
    #[serde(rename = "B")]
    pub bid_quantity: Dec,
    #[serde(rename = "a")]
    pub ask_price: Dec,
    #[serde(rename = "A")]
    pub ask_quantity: Dec,
}

/// A liquidation (`forceOrder`): the latest in its symbol's second.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Liquidation {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "o")]
    pub order: LiquidationOrder,
}

/// The order a liquidation sent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LiquidationOrder {
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "S")]
    pub side: Side,
    #[serde(rename = "o")]
    pub order_type: OrderType,
    #[serde(rename = "f")]
    pub time_in_force: TimeInForce,
    #[serde(rename = "q", default, deserialize_with = "lenient::zero_dec")]
    pub quantity: Dec,
    #[serde(rename = "p", default, deserialize_with = "lenient::zero_dec")]
    pub price: Dec,
    #[serde(rename = "ap", default, deserialize_with = "lenient::zero_dec")]
    pub avg_price: Dec,
    #[serde(rename = "X")]
    pub status: OrderStatus,
    /// `l` and `z`: the last fill's quantity and the quantity filled in all.
    #[serde(rename = "l", default, deserialize_with = "lenient::zero_dec")]
    pub last_filled: Dec,
    #[serde(rename = "z", default, deserialize_with = "lenient::zero_dec")]
    pub filled: Dec,
    #[serde(rename = "T", default)]
    pub trade_time: u64,
}

/// One price level: a price and the quantity resting there, zero when the level is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub price: Dec,
    pub quantity: Dec,
}

/// The venue writes a level as `["price", "quantity"]`.
impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (price, quantity) = <(Dec, Dec)>::deserialize(deserializer)?;
        Ok(Level { price, quantity })
    }
}

/// Book levels that changed (`depthUpdate`). From a diff stream (`<symbol>@depth`) each update
/// follows the last: its `previous_final_id` is the last one's `final_id`. From a partial stream
/// (`<symbol>@depth5`) it is the top of the book as it stands.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DepthUpdate {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "T", default)]
    pub transaction_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    /// `U`: the first update id in it.
    #[serde(rename = "U")]
    pub first_id: u64,
    /// `u`: the last update id in it.
    #[serde(rename = "u")]
    pub final_id: u64,
    /// `pu`: the last update id of the update before it.
    #[serde(rename = "pu", default)]
    pub previous_final_id: u64,
    #[serde(rename = "b", default, deserialize_with = "lenient::list")]
    pub bids: Vec<Level>,
    #[serde(rename = "a", default, deserialize_with = "lenient::list")]
    pub asks: Vec<Level>,
}

/// A trade with its hash and both parties' addresses (`tradepro`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TradePro {
    #[serde(rename = "E", default)]
    pub event_time: u64,
    #[serde(rename = "T", default)]
    pub trade_time: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t", default)]
    pub id: u64,
    #[serde(rename = "p")]
    pub price: Dec,
    #[serde(rename = "q")]
    pub quantity: Dec,
    /// `h`: the transaction hash.
    #[serde(rename = "h", default, deserialize_with = "lenient::text")]
    pub hash: String,
    /// `m`: the taker's address then the maker's; `hidden` where the venue hides one.
    #[serde(rename = "m", default, deserialize_with = "lenient::list")]
    pub parties: Vec<String>,
}

impl TradePro {
    /// The taker's address, as the venue wrote it.
    #[must_use]
    pub fn taker(&self) -> Option<&str> {
        self.parties.first().map(String::as_str)
    }

    /// The maker's address, as the venue wrote it.
    #[must_use]
    pub fn maker(&self) -> Option<&str> {
        self.parties.get(1).map(String::as_str)
    }
}
