//! The enums orders, positions and markets are described by, spelled as the venue spells them
//! (the docs' "ENUM definitions"). Values the venue may add later read as `Other` where an
//! answer carries them, so a new one never makes a whole answer unreadable.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! venue_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident = $text:literal),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
            /// A value this build does not know, as the venue wrote it.
            Other(String),
        }

        impl $name {
            /// Every value this build knows.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// As the venue writes it.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $($name::$variant => $text,)+
                    $name::Other(text) => text.as_str(),
                }
            }

            /// From the venue's spelling; anything else is `Other`.
            #[must_use]
            pub fn parse(text: &str) -> Self {
                match text {
                    $($text => $name::$variant,)+
                    other => $name::Other(other.to_owned()),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                Ok($name::parse(&text))
            }
        }
    };
}

venue_enum! {
    /// Which way an order trades.
    Side { Buy = "BUY", Sell = "SELL" }
}

impl Side {
    #[must_use]
    pub fn opposite(&self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
            Side::Other(text) => Side::Other(text.clone()),
        }
    }
}

venue_enum! {
    /// The position an order or a row belongs to: `Both` in one-way mode, `Long` or `Short` in
    /// hedge mode.
    PositionSide { Both = "BOTH", Long = "LONG", Short = "SHORT" }
}

venue_enum! {
    OrderType {
        Limit = "LIMIT",
        Market = "MARKET",
        /// A limit order placed when the stop price is reached.
        Stop = "STOP",
        StopMarket = "STOP_MARKET",
        /// A limit order placed when the take-profit price is reached.
        TakeProfit = "TAKE_PROFIT",
        TakeProfitMarket = "TAKE_PROFIT_MARKET",
        TrailingStopMarket = "TRAILING_STOP_MARKET",
        /// The type a liquidation's order has in the user stream.
        Liquidation = "LIQUIDATION",
    }
}

impl OrderType {
    /// Whether it waits for a trigger price.
    #[must_use]
    pub const fn is_conditional(&self) -> bool {
        matches!(
            self,
            OrderType::Stop
                | OrderType::StopMarket
                | OrderType::TakeProfit
                | OrderType::TakeProfitMarket
                | OrderType::TrailingStopMarket
        )
    }
}

venue_enum! {
    TimeInForce {
        /// Good till cancelled.
        Gtc = "GTC",
        /// Immediate or cancel.
        Ioc = "IOC",
        /// Post only: good till crossing.
        Gtx = "GTX",
        /// Rests unseen in the book.
        Hidden = "HIDDEN",
        /// Fill or kill: listed by the enum, though no symbol takes it today.
        Fok = "FOK",
    }
}

venue_enum! {
    /// The price a stop's trigger watches.
    WorkingType { MarkPrice = "MARK_PRICE", ContractPrice = "CONTRACT_PRICE" }
}

venue_enum! {
    OrderStatus {
        New = "NEW",
        PartiallyFilled = "PARTIALLY_FILLED",
        Filled = "FILLED",
        Canceled = "CANCELED",
        Rejected = "REJECTED",
        Expired = "EXPIRED",
        /// Seen in the user stream for a conditional order that was triggered.
        NewInsurance = "NEW_INSURANCE",
        NewAdl = "NEW_ADL",
    }
}

impl OrderStatus {
    /// Whether the order still rests or waits.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, OrderStatus::New | OrderStatus::PartiallyFilled)
    }
}

venue_enum! {
    /// What a new order's answer carries: `Ack` at once, `Result` after a market or IOC order
    /// has finished.
    ResponseType { Ack = "ACK", Result = "RESULT" }
}

venue_enum! {
    /// Self-trade prevention: which side of a self-trade is cancelled.
    StpMode { None = "NONE", ExpireTaker = "EXPIRE_TAKER", ExpireMaker = "EXPIRE_MAKER", ExpireBoth = "EXPIRE_BOTH" }
}

venue_enum! {
    /// BBO peg for a limit or a conditional limit order: the price is taken from the book.
    PegPriceType { Counterparty1 = "COUNTERPARTY_1", Queue1 = "QUEUE_1" }
}

venue_enum! {
    /// A position's margin: its own (isolated) or the account's (cross).
    MarginType { Isolated = "ISOLATED", Crossed = "CROSSED" }
}

impl MarginType {
    /// How `positionRisk` and the user stream write it: `isolated` and `cross`.
    #[must_use]
    pub fn parse_lower(text: &str) -> MarginType {
        match text.to_ascii_lowercase().as_str() {
            "isolated" => MarginType::Isolated,
            "cross" | "crossed" => MarginType::Crossed,
            _ => MarginType::Other(text.to_owned()),
        }
    }
}

venue_enum! {
    ContractStatus {
        PendingTrading = "PENDING_TRADING",
        Trading = "TRADING",
        PreSettle = "PRE_SETTLE",
        Settling = "SETTLING",
        Close = "CLOSE",
    }
}

venue_enum! {
    /// A candle's length.
    Interval {
        M1 = "1m",
        M3 = "3m",
        M5 = "5m",
        M15 = "15m",
        M30 = "30m",
        H1 = "1h",
        H2 = "2h",
        H4 = "4h",
        H6 = "6h",
        H8 = "8h",
        H12 = "12h",
        D1 = "1d",
        D3 = "3d",
        W1 = "1w",
        Mo1 = "1M",
    }
}

impl Interval {
    /// Its length in milliseconds; a month counts as 30 days.
    #[must_use]
    pub const fn ms(&self) -> Option<u64> {
        const M: u64 = 60_000;
        Some(match self {
            Interval::M1 => M,
            Interval::M3 => 3 * M,
            Interval::M5 => 5 * M,
            Interval::M15 => 15 * M,
            Interval::M30 => 30 * M,
            Interval::H1 => 60 * M,
            Interval::H2 => 120 * M,
            Interval::H4 => 240 * M,
            Interval::H6 => 360 * M,
            Interval::H8 => 480 * M,
            Interval::H12 => 720 * M,
            Interval::D1 => 1_440 * M,
            Interval::D3 => 4_320 * M,
            Interval::W1 => 10_080 * M,
            Interval::Mo1 => 43_200 * M,
            Interval::Other(_) => return None,
        })
    }
}

#[cfg(test)]
#[path = "tests/order.rs"]
mod tests;
