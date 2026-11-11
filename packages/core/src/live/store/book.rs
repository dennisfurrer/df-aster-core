//! A symbol's order book kept from a REST snapshot (`GET /fapi/v3/depth`) and the diff stream
//! (`<symbol>@depth`), as the docs' "How to manage a local order book correctly" says:
//!
//! 1. Updates that arrive before the snapshot are buffered.
//! 2. With the snapshot, buffered updates whose `u` is below its `lastUpdateId` are dropped; the
//!    first one applied must have `U` at or below `lastUpdateId` and `u` at or above it.
//!    With nothing buffered, the next update to arrive must do the same.
//! 3. From then on each update's `pu` must be the last one's `u`; one that is not breaks the
//!    chain, and the book waits for a fresh snapshot, buffering again.
//! 4. A level's quantity is absolute; zero removes it. Removing a level the book does not hold is
//!    normal.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::live::market::{DepthUpdate, Level};
use crate::utils::Dec;

/// What `GET /fapi/v3/depth` answers: the levels as of `last_update_id`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub last_update_id: u64,
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
}

/// What an update or a snapshot did to the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Kept until a snapshot comes.
    Buffered,
    /// Applied: the book is current to `final_id`.
    Applied { final_id: u64 },
    /// Older than what the book holds: dropped.
    Stale,
    /// The chain broke: the book is empty and waits for a fresh snapshot, buffering from this
    /// update on.
    Gap { expected: u64, got: u64 },
}

/// One symbol's book.
#[derive(Debug, Clone, Default)]
pub struct Book {
    bids: BTreeMap<Dec, Dec>,
    asks: BTreeMap<Dec, Dec>,
    /// The last update applied; `None` while waiting for a snapshot.
    last: Option<u64>,
    /// `last` is a snapshot's id and no update has followed it yet: the next must straddle it.
    fresh: bool,
    buffer: Vec<DepthUpdate>,
}

/// The most updates held while waiting for a snapshot; older ones are dropped first.
const MOST_BUFFERED: usize = 1_000;

impl Book {
    #[must_use]
    pub fn new() -> Self {
        Book::default()
    }

    /// Whether the book is current: it has a snapshot and every update since.
    #[must_use]
    pub const fn is_synced(&self) -> bool {
        self.last.is_some()
    }

    /// The last update id the book is current to.
    #[must_use]
    pub const fn last_update_id(&self) -> Option<u64> {
        self.last
    }

    /// Updates held for the next snapshot.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// One diff update.
    pub fn update(&mut self, update: DepthUpdate) -> Applied {
        let Some(last) = self.last else {
            if self.buffer.len() >= MOST_BUFFERED {
                self.buffer.remove(0);
            }
            self.buffer.push(update);
            return Applied::Buffered;
        };
        if self.fresh {
            if update.final_id < last {
                return Applied::Stale;
            }
            if (update.first_id <= last && update.final_id >= last) || update.previous_final_id == last {
                return self.apply(&update);
            }
        } else if update.final_id <= last {
            return Applied::Stale;
        }
        if update.previous_final_id != last {
            let gap = Applied::Gap { expected: last, got: update.previous_final_id };
            self.clear();
            self.buffer.push(update);
            return gap;
        }
        self.apply(&update)
    }

    /// A REST snapshot: the book starts from it, then applies the updates buffered since. A
    /// snapshot older than every buffered update leaves a hole: the book stays unsynced and says
    /// [`Applied::Gap`], and a newer snapshot is needed.
    pub fn snapshot(&mut self, snapshot: &Snapshot) -> Applied {
        self.bids = levels(&snapshot.bids);
        self.asks = levels(&snapshot.asks);
        let id = snapshot.last_update_id;
        let buffered = std::mem::take(&mut self.buffer);
        let mut pending = buffered.into_iter().filter(|update| update.final_id >= id).peekable();
        if let Some(first) = pending.peek()
            && first.first_id > id
        {
            let gap = Applied::Gap { expected: id, got: first.first_id };
            self.bids.clear();
            self.asks.clear();
            self.last = None;
            self.fresh = false;
            self.buffer = pending.collect();
            return gap;
        }
        self.last = Some(id);
        self.fresh = true;
        let mut result = Applied::Applied { final_id: id };
        for update in pending {
            // The first applied straddles the snapshot; the rest must chain.
            let follows = self.fresh || self.last.is_some_and(|last| update.previous_final_id == last);
            if !follows {
                let gap = Applied::Gap { expected: self.last.unwrap_or(id), got: update.previous_final_id };
                self.clear();
                self.buffer.push(update);
                return gap;
            }
            result = self.apply(&update);
        }
        result
    }

    /// Bids, best first.
    pub fn bids(&self) -> impl Iterator<Item = Level> + '_ {
        self.bids.iter().rev().map(|(price, quantity)| Level { price: *price, quantity: *quantity })
    }

    /// Asks, best first.
    pub fn asks(&self) -> impl Iterator<Item = Level> + '_ {
        self.asks.iter().map(|(price, quantity)| Level { price: *price, quantity: *quantity })
    }

    #[must_use]
    pub fn best_bid(&self) -> Option<Level> {
        self.bids().next()
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Level> {
        self.asks().next()
    }

    /// The best ask less the best bid.
    #[must_use]
    pub fn spread(&self) -> Option<Dec> {
        self.best_ask()?.price.checked_sub(self.best_bid()?.price)
    }

    fn apply(&mut self, update: &DepthUpdate) -> Applied {
        set(&mut self.bids, &update.bids);
        set(&mut self.asks, &update.asks);
        self.last = Some(update.final_id);
        self.fresh = false;
        Applied::Applied { final_id: update.final_id }
    }

    /// Empty, waiting for a snapshot.
    fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.last = None;
        self.fresh = false;
        self.buffer.clear();
    }
}

fn levels(levels: &[Level]) -> BTreeMap<Dec, Dec> {
    levels.iter().filter(|level| level.quantity.is_positive()).map(|level| (level.price, level.quantity)).collect()
}

fn set(side: &mut BTreeMap<Dec, Dec>, levels: &[Level]) {
    for level in levels {
        if level.quantity.is_positive() {
            side.insert(level.price, level.quantity);
        } else {
            side.remove(&level.price);
        }
    }
}

#[cfg(test)]
#[path = "tests/book.rs"]
mod tests;
