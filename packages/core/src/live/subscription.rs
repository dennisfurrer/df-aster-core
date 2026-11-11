//! Every stream the futures websocket serves (the docs' "Websocket Market Streams"), as the
//! name a subscribe gives it: `btcusdt@aggTrade`, `btcusdt@depth5@100ms`, `!markPrice@arr@1s`,
//! and an account's stream, which is named by its listen key alone. A symbol is written in lower
//! case in every name.

use std::fmt;

use crate::utils::Interval;

/// How often a book stream pushes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Speed {
    /// Every 250 ms: the name carries no speed.
    #[default]
    Ms250,
    Ms500,
    Ms100,
}

impl Speed {
    /// What the name ends with: nothing for 250 ms.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Speed::Ms250 => "",
            Speed::Ms500 => "@500ms",
            Speed::Ms100 => "@100ms",
        }
    }
}

/// How many levels a side of a partial book stream carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Levels {
    Five,
    Ten,
    Twenty,
}

impl Levels {
    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            Levels::Five => 5,
            Levels::Ten => 10,
            Levels::Twenty => 20,
        }
    }

    #[must_use]
    pub const fn of(count: u32) -> Option<Levels> {
        match count {
            5 => Some(Levels::Five),
            10 => Some(Levels::Ten),
            20 => Some(Levels::Twenty),
            _ => None,
        }
    }
}

/// One stream. A symbol is kept in lower case, as the name writes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subscription {
    /// A symbol's trades, aggregated per taker order every 100 ms (`<symbol>@aggTrade`).
    AggTrade { symbol: String },
    /// A symbol's mark price, index price and funding rate, every 3 s or every second
    /// (`<symbol>@markPrice`, `<symbol>@markPrice@1s`).
    MarkPrice { symbol: String, every_second: bool },
    /// Every symbol's mark price (`!markPrice@arr`, `!markPrice@arr@1s`).
    AllMarkPrices { every_second: bool },
    /// A symbol's current candle, every 250 ms (`<symbol>@kline_<interval>`).
    Kline { symbol: String, interval: Interval },
    /// A symbol's rolling 24 hours, short form, every 500 ms (`<symbol>@miniTicker`).
    MiniTicker { symbol: String },
    /// Every symbol's that changed, every second (`!miniTicker@arr`).
    AllMiniTickers,
    /// A symbol's rolling 24 hours, every 500 ms (`<symbol>@ticker`).
    Ticker { symbol: String },
    /// Every symbol's that changed, every second (`!ticker@arr`).
    AllTickers,
    /// A symbol's best bid and ask, on every change (`<symbol>@bookTicker`).
    BookTicker { symbol: String },
    /// Every symbol's best bid and ask (`!bookTicker`).
    AllBookTickers,
    /// A symbol's latest liquidation in each second there was one (`<symbol>@forceOrder`).
    Liquidations { symbol: String },
    /// Every symbol's (`!forceOrder@arr`).
    AllLiquidations,
    /// The top 5, 10 or 20 levels of a side (`<symbol>@depth<levels>`, with a speed).
    PartialDepth { symbol: String, levels: Levels, speed: Speed },
    /// The levels that changed, for a book kept from a snapshot (`<symbol>@depth`, with a speed).
    Depth { symbol: String, speed: Speed },
    /// A symbol's trades with their transaction hash and both parties (`<symbol>@tradepro`).
    TradePro { symbol: String },
    /// An account's own events, by the listen key `POST /fapi/v3/listenKey` gave.
    Account { listen_key: String },
}

/// Why a stream cannot be named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSubscription {
    #[error("`{0}` is not a symbol a stream can name")]
    Symbol(String),
    #[error("`{0}` is not a candle interval the venue streams")]
    Interval(String),
    #[error("a listen key is not empty and has no `@` or `/`")]
    ListenKey,
    #[error("`{0}` is not a stream name")]
    Name(String),
}

impl Subscription {
    #[must_use]
    pub fn agg_trade(symbol: &str) -> Self {
        Subscription::AggTrade { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn mark_price(symbol: &str) -> Self {
        Subscription::MarkPrice { symbol: lower(symbol), every_second: false }
    }

    #[must_use]
    pub fn kline(symbol: &str, interval: Interval) -> Self {
        Subscription::Kline { symbol: lower(symbol), interval }
    }

    #[must_use]
    pub fn mini_ticker(symbol: &str) -> Self {
        Subscription::MiniTicker { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn ticker(symbol: &str) -> Self {
        Subscription::Ticker { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn book_ticker(symbol: &str) -> Self {
        Subscription::BookTicker { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn liquidations(symbol: &str) -> Self {
        Subscription::Liquidations { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn partial_depth(symbol: &str, levels: Levels, speed: Speed) -> Self {
        Subscription::PartialDepth { symbol: lower(symbol), levels, speed }
    }

    #[must_use]
    pub fn depth(symbol: &str, speed: Speed) -> Self {
        Subscription::Depth { symbol: lower(symbol), speed }
    }

    #[must_use]
    pub fn trade_pro(symbol: &str) -> Self {
        Subscription::TradePro { symbol: lower(symbol) }
    }

    #[must_use]
    pub fn account(listen_key: &str) -> Self {
        Subscription::Account { listen_key: listen_key.trim().to_owned() }
    }

    /// The name a subscribe gives it, and a combined stream's frames carry.
    #[must_use]
    pub fn name(&self) -> String {
        let second = |every: bool| if every { "@1s" } else { "" };
        match self {
            Subscription::AggTrade { symbol } => format!("{symbol}@aggTrade"),
            Subscription::MarkPrice { symbol, every_second } => format!("{symbol}@markPrice{}", second(*every_second)),
            Subscription::AllMarkPrices { every_second } => format!("!markPrice@arr{}", second(*every_second)),
            Subscription::Kline { symbol, interval } => format!("{symbol}@kline_{interval}"),
            Subscription::MiniTicker { symbol } => format!("{symbol}@miniTicker"),
            Subscription::AllMiniTickers => "!miniTicker@arr".to_owned(),
            Subscription::Ticker { symbol } => format!("{symbol}@ticker"),
            Subscription::AllTickers => "!ticker@arr".to_owned(),
            Subscription::BookTicker { symbol } => format!("{symbol}@bookTicker"),
            Subscription::AllBookTickers => "!bookTicker".to_owned(),
            Subscription::Liquidations { symbol } => format!("{symbol}@forceOrder"),
            Subscription::AllLiquidations => "!forceOrder@arr".to_owned(),
            Subscription::PartialDepth { symbol, levels, speed } => {
                format!("{symbol}@depth{}{}", levels.count(), speed.suffix())
            }
            Subscription::Depth { symbol, speed } => format!("{symbol}@depth{}", speed.suffix()),
            Subscription::TradePro { symbol } => format!("{symbol}@tradepro"),
            Subscription::Account { listen_key } => listen_key.clone(),
        }
    }

    /// The stream a name is, the reverse of [`Subscription::name`].
    pub fn parse(name: &str) -> Result<Self, BadSubscription> {
        let bad = || BadSubscription::Name(name.to_owned());
        let parsed = match name {
            "!markPrice@arr" => Subscription::AllMarkPrices { every_second: false },
            "!markPrice@arr@1s" => Subscription::AllMarkPrices { every_second: true },
            "!miniTicker@arr" => Subscription::AllMiniTickers,
            "!ticker@arr" => Subscription::AllTickers,
            "!bookTicker" => Subscription::AllBookTickers,
            "!forceOrder@arr" => Subscription::AllLiquidations,
            _ => {
                let Some((symbol, stream)) = name.split_once('@') else {
                    let key = Subscription::account(name);
                    key.check()?;
                    return Ok(key);
                };
                let symbol = symbol.to_owned();
                match stream {
                    "aggTrade" => Subscription::AggTrade { symbol },
                    "markPrice" => Subscription::MarkPrice { symbol, every_second: false },
                    "markPrice@1s" => Subscription::MarkPrice { symbol, every_second: true },
                    "miniTicker" => Subscription::MiniTicker { symbol },
                    "ticker" => Subscription::Ticker { symbol },
                    "bookTicker" => Subscription::BookTicker { symbol },
                    "forceOrder" => Subscription::Liquidations { symbol },
                    "tradepro" => Subscription::TradePro { symbol },
                    _ => {
                        if let Some(interval) = stream.strip_prefix("kline_") {
                            Subscription::Kline { symbol, interval: Interval::parse(interval) }
                        } else if let Some(depth) = stream.strip_prefix("depth") {
                            let (count, speed) = match depth.split_once('@') {
                                Some((count, "100ms")) => (count, Speed::Ms100),
                                Some((count, "500ms")) => (count, Speed::Ms500),
                                Some(_) => return Err(bad()),
                                None => (depth, Speed::Ms250),
                            };
                            match count {
                                "" => Subscription::Depth { symbol, speed },
                                count => {
                                    let levels = count.parse().ok().and_then(Levels::of).ok_or_else(bad)?;
                                    Subscription::PartialDepth { symbol, levels, speed }
                                }
                            }
                        } else {
                            return Err(bad());
                        }
                    }
                }
            }
        };
        parsed.check()?;
        Ok(parsed)
    }

    /// Whether the venue can serve it: a symbol in lower case with no `@`, `/` or space; an
    /// interval the venue lists; a listen key that is not empty.
    pub fn check(&self) -> Result<(), BadSubscription> {
        if let Some(symbol) = self.symbol()
            && (symbol.is_empty()
                || symbol
                    .chars()
                    .any(|c| c == '@' || c == '/' || c == '!' || c.is_whitespace() || c.is_ascii_uppercase()))
        {
            return Err(BadSubscription::Symbol(symbol.to_owned()));
        }
        match self {
            Subscription::Kline { interval: Interval::Other(other), .. } => {
                Err(BadSubscription::Interval(other.clone()))
            }
            Subscription::Account { listen_key }
                if listen_key.is_empty() || listen_key.contains(['@', '/']) || listen_key.starts_with('!') =>
            {
                Err(BadSubscription::ListenKey)
            }
            _ => Ok(()),
        }
    }

    /// The symbol it follows, in lower case; `None` for an every-symbol stream or an account.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Subscription::AggTrade { symbol }
            | Subscription::MarkPrice { symbol, .. }
            | Subscription::Kline { symbol, .. }
            | Subscription::MiniTicker { symbol }
            | Subscription::Ticker { symbol }
            | Subscription::BookTicker { symbol }
            | Subscription::Liquidations { symbol }
            | Subscription::PartialDepth { symbol, .. }
            | Subscription::Depth { symbol, .. }
            | Subscription::TradePro { symbol } => Some(symbol),
            _ => None,
        }
    }

    /// Whether its updates chain (`pu` is the last update's `u`) into a book kept from a
    /// snapshot.
    #[must_use]
    pub const fn is_diff_depth(&self) -> bool {
        matches!(self, Subscription::Depth { .. })
    }

    #[must_use]
    pub const fn is_account(&self) -> bool {
        matches!(self, Subscription::Account { .. })
    }
}

impl fmt::Display for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

fn lower(symbol: &str) -> String {
    symbol.trim().to_lowercase()
}

#[cfg(test)]
#[path = "tests/subscription.rs"]
mod tests;
