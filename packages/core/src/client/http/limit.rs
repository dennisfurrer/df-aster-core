//! Request weights against Aster's rate limits (the docs' "LIMITS" and each endpoint's weight),
//! the last minute's spend, and the gate a 429, a 418 or the firewall's 403 closes. No clock and
//! no I/O: every call is handed the time.
//!
//! - **Weight:** 2,400 a minute per IP (`REQUEST_WEIGHT`), counted by the venue across every
//!   program on the IP. Each answer says what it has counted (`X-MBX-USED-WEIGHT-1M`); the meter
//!   takes the larger of that and its own count.
//! - **Orders:** 1,200 a minute and 300 every ten seconds per account (`ORDERS`).
//! - **Refusals:** a 429 asks the program to back off; one that keeps sending is banned (418),
//!   for two minutes up to three days. The gate stops everything until the wait is over.

use std::collections::VecDeque;

use crate::client::http::wire::Method;

/// The window the weight limit is over.
pub const WINDOW_MS: u64 = 60_000;

/// The IP's weight a minute.
pub const WEIGHT_BUDGET: u32 = 2_400;

/// Orders an account may place a minute.
pub const ORDERS_PER_MINUTE: u32 = 1_200;

/// Orders an account may place every ten seconds.
pub const ORDERS_PER_10S: u32 = 300;

const TEN_SECONDS_MS: u64 = 10_000;

/// The first cooldown a 429 closes the gate for; each strike doubles it.
pub const BASE_COOLDOWN_MS: u64 = 2_000;

/// The longest cooldown a strike sets, though a `Retry-After` may ask longer.
pub const MAX_COOLDOWN_MS: u64 = 60_000;

/// The shortest ban a 418 means, when it gives no `Retry-After`.
pub const BAN_MS: u64 = 120_000;

const MAX_STRIKES: u32 = 6;

/// How long the venue's own count (`X-MBX-USED-WEIGHT-1M`) is taken as current.
const REPORT_FRESH_MS: u64 = 5_000;

/// A path's weight, from the docs. Where it depends on what is asked (a book's depth, a candle
/// count, every symbol or one), the builder sets it with [`crate::client::Call::weigh`]; this is
/// the weight for one symbol and the default size.
#[must_use]
pub fn weight(method: Method, path: &str) -> u32 {
    let tail = path.rsplit_once("/v3/").map_or(path, |(_, tail)| tail);
    match (method, tail) {
        (Method::Post, "order") => 0,
        (_, "historicalTrades" | "aggTrades" | "commissionRate" | "staking/getLockedAster") => 20,
        (Method::Get, "positionSide/dual" | "stpMode" | "multiAssetsMargin" | "income" | "incomePage") => 30,
        (_, "ticker/24hr") => 1,
        (_, "openOrders") => 1,
        (_, "forceOrders") => 20,
        (_, "batchOrders") if matches!(method, Method::Post | Method::Put) => 5,
        (_, "countdownCancelAll") => 10,
        (
            _,
            "remainingOpenableNotionalValue"
            | "asset/wallet/transfer"
            | "placeStrategyOrder"
            | "updateStrategyOrder"
            | "asset/migrateUser"
            | "asset/migrateUser/history"
            | "registerAndApproveAgent"
            | "transfer"
            | "batch-transfer",
        ) => 50,
        (
            _,
            "allOrders"
            | "balance"
            | "account"
            | "accountWithJoinMargin"
            | "positionRisk"
            | "userTrades"
            | "adlQuantile"
            | "strategyOpenOrder"
            | "strategyHistoryOrder"
            | "sub-accounts/bind"
            | "createSubAccount"
            | "getSubAccountList"
            | "updateSubAccount"
            | "subAccountTransfer"
            | "account/modify-status"
            | "batch-transfer/history"
            | "staking/create"
            | "staking/deposit"
            | "staking/updateLockPeriod"
            | "staking/claimRewards"
            | "perp/user-withdraw"
            | "perp/user-solana-withdraw"
            | "perp/deposit-withdraw-history"
            | "perp/wallet/transfer"
            | "spot/user-withdraw"
            | "spot/user-solana-withdraw"
            | "spot/wallet/transfer",
        ) => 5,
        (_, tail) if tail.starts_with("builder/") => 5,
        _ => 1,
    }
}

/// What went out over the last minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spent {
    /// The IP's weight a minute.
    pub budget: u32,
    /// This client's own count over the last minute.
    pub minute: u32,
    /// What the venue last said it has counted for the IP, which other programs add to.
    pub venue_minute: Option<u32>,
    /// By path, the heaviest first.
    pub kinds: Vec<Kind>,
    /// Orders this client placed over the last ten seconds and the last minute.
    pub orders_10s: u32,
    pub orders_minute: u32,
    /// 429s, 418s and 403s over the last minute.
    pub refused: u32,
    /// How long the gate stays closed; 0 when it is open.
    pub cooldown_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kind {
    pub name: String,
    pub count: u32,
    pub weight: u32,
}

/// The last minute of requests: when each went out, its path and weight; the orders placed; the
/// venue's last count; and when each refusal came.
#[derive(Debug, Default)]
pub(crate) struct Meter {
    sent: VecDeque<(u64, String, u32)>,
    orders: VecDeque<(u64, u32)>,
    refused: VecDeque<u64>,
    reported: Option<(u64, u32)>,
}

impl Meter {
    /// Drops what is a whole window old at `now`.
    fn prune(&mut self, now: u64) {
        let gone = |at: u64| at.saturating_add(WINDOW_MS) <= now;
        while self.sent.front().is_some_and(|(at, _, _)| gone(*at)) {
            self.sent.pop_front();
        }
        while self.orders.front().is_some_and(|(at, _)| gone(*at)) {
            self.orders.pop_front();
        }
        while self.refused.front().is_some_and(|at| gone(*at)) {
            self.refused.pop_front();
        }
    }

    pub(crate) fn record(&mut self, now: u64, path: &str, weight: u32, orders: u32) {
        self.prune(now);
        self.sent.push_back((now, path.to_owned(), weight));
        if orders > 0 {
            self.orders.push_back((now, orders));
        }
    }

    /// The venue's `X-MBX-USED-WEIGHT-1M` at `now`.
    pub(crate) fn report(&mut self, now: u64, used: u32) {
        self.reported = Some((now, used));
    }

    pub(crate) fn refuse(&mut self, now: u64) {
        self.prune(now);
        self.refused.push_back(now);
    }

    fn own(&self) -> u32 {
        self.sent.iter().fold(0u32, |sum, (_, _, weight)| sum.saturating_add(*weight))
    }

    fn venue(&self, now: u64) -> Option<u32> {
        self.reported.filter(|(at, _)| now.saturating_sub(*at) < REPORT_FRESH_MS).map(|(_, used)| used)
    }

    /// Orders placed over the `window` before `now`.
    fn orders_within(&self, now: u64, window: u64) -> u32 {
        self.orders
            .iter()
            .filter(|(at, _)| at.saturating_add(window) > now)
            .fold(0u32, |sum, (_, count)| sum.saturating_add(*count))
    }

    /// How long a request of `weight` that places `orders` waits at `now`: none while the IP's
    /// minute stays within four fifths of the budget (the rest is left to other programs on the
    /// IP) and the account's order limits hold, else until enough has rolled off.
    pub(crate) fn wait_for(&mut self, now: u64, weight: u32, orders: u32) -> Option<u64> {
        self.prune(now);
        if orders > 0 {
            if self.orders_within(now, TEN_SECONDS_MS).saturating_add(orders) > ORDERS_PER_10S {
                return Some(TEN_SECONDS_MS);
            }
            if self.orders_within(now, WINDOW_MS).saturating_add(orders) > ORDERS_PER_MINUTE {
                return Some(self.orders.front().map_or(WINDOW_MS, |(at, _)| at + WINDOW_MS - now).max(1));
            }
        }
        let soft = WEIGHT_BUDGET / 5 * 4;
        let spent = self.own().max(self.venue(now).unwrap_or(0));
        let mut over = spent.saturating_add(weight).checked_sub(soft).filter(|over| *over > 0)?;
        for (at, _, cost) in &self.sent {
            if *cost >= over {
                return Some(at.saturating_add(WINDOW_MS).saturating_sub(now).max(1));
            }
            over -= cost;
        }
        Some(WINDOW_MS)
    }

    /// The last minute at `now`, with no cooldown: the [`Gate`] knows that.
    pub(crate) fn spent(&mut self, now: u64) -> Spent {
        self.prune(now);
        let mut kinds: Vec<Kind> = Vec::new();
        for (_, name, cost) in &self.sent {
            match kinds.iter_mut().find(|kind| kind.name == *name) {
                Some(kind) => {
                    kind.count += 1;
                    kind.weight = kind.weight.saturating_add(*cost);
                }
                None => kinds.push(Kind { name: name.clone(), count: 1, weight: *cost }),
            }
        }
        kinds.sort_by(|a, b| b.weight.cmp(&a.weight).then_with(|| a.name.cmp(&b.name)));
        Spent {
            budget: WEIGHT_BUDGET,
            minute: self.own(),
            venue_minute: self.venue(now),
            kinds,
            orders_10s: self.orders_within(now, TEN_SECONDS_MS),
            orders_minute: self.orders_within(now, WINDOW_MS),
            refused: u32::try_from(self.refused.len()).unwrap_or(u32::MAX),
            cooldown_ms: 0,
        }
    }
}

/// What a request carries out through the [`Gate`]: how many strikes there had been when it went
/// out, so a burst sent before a 429 counts once.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ticket(u64);

/// Closed by a 429, a 418 or a 403 for every request: the venue limits by IP.
#[derive(Debug, Default)]
pub(crate) struct Gate {
    until: u64,
    /// Consecutive strikes, a burst counting once.
    strikes: u32,
    /// Strikes ever.
    epoch: u64,
}

impl Gate {
    /// A ticket out, or how long the gate stays closed.
    pub(crate) fn open(&self, now: u64) -> Result<Ticket, u64> {
        match self.until > now {
            true => Err(self.until - now),
            false => Ok(Ticket(self.epoch)),
        }
    }

    /// The request was answered: room again, unless a refusal struck since it went out.
    pub(crate) fn passed(&mut self, ticket: Ticket) {
        if ticket.0 == self.epoch {
            self.strikes = 0;
        }
    }

    /// A 429 came back at `now`. The first of a burst strikes: two seconds, doubling to a minute,
    /// or a longer `Retry-After`. The rest of the burst only moves the gate for a longer
    /// `Retry-After`. Gives the wait.
    pub(crate) fn struck(&mut self, ticket: Ticket, now: u64, retry_after_ms: Option<u64>) -> u64 {
        let wait = match ticket.0 == self.epoch {
            true => {
                self.epoch += 1;
                self.strikes = (self.strikes + 1).min(MAX_STRIKES);
                let backoff = (BASE_COOLDOWN_MS << (self.strikes - 1)).min(MAX_COOLDOWN_MS);
                backoff.max(retry_after_ms.unwrap_or(0))
            }
            false => retry_after_ms.unwrap_or(0),
        };
        self.until = self.until.max(now.saturating_add(wait));
        self.until.saturating_sub(now)
    }

    /// A 418 ban at `now`: its `Retry-After`, or two minutes.
    pub(crate) fn banned(&mut self, ticket: Ticket, now: u64, retry_after_ms: Option<u64>) -> u64 {
        self.struck(ticket, now, Some(retry_after_ms.unwrap_or(BAN_MS).max(BAN_MS)))
    }

    /// The firewall's 403 at `now`: a whole minute, at once.
    pub(crate) fn blocked(&mut self, ticket: Ticket, now: u64) -> u64 {
        self.struck(ticket, now, Some(MAX_COOLDOWN_MS))
    }

    #[must_use]
    pub(crate) const fn cooldown_ms(&self, now: u64) -> u64 {
        self.until.saturating_sub(now)
    }
}

#[cfg(test)]
#[path = "tests/limit.rs"]
mod tests;
