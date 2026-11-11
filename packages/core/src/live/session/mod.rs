//! One connection's state with no socket and no clock: the socket's owner hands in what happened
//! and when, and sends what comes back. It keeps what is wanted and subscribes it on every
//! socket; holds frames back so the venue's ten a second is never reached, putting what waits
//! into one `SUBSCRIBE` or `UNSUBSCRIBE`; matches each answer to what it asked; checks each diff
//! book's update chain; and tells a dead socket, or one near its 24 hours, by the clock.
//!
//! The socket is the combined stream (`wss://fstream.asterdex.com/stream`), so every frame names
//! its stream, and an account's stream is subscribed by its listen key beside the market's.
//!
//! - [`limits`]: the venue's limits and why a session refuses.
//! - [`timing`]: the quiet, the silence, the reconnect delays and the 24 hours.

pub mod limits;
pub mod timing;

pub use limits::{CONNECTION_LIFETIME_MS, Limits, MAX_MESSAGES_PER_SECOND, MAX_STREAMS_PER_CONNECTION, Refused};
pub use timing::{Retry, Tick, Timing};

use std::collections::{BTreeMap, VecDeque};

use crate::client::Hosts;
use crate::live::{Asked, BadFrame, Data, Frame, Gap, Subscription, decode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Link {
    Closed,
    Open {
        /// When it opened.
        since: u64,
        /// When anything last arrived; `since` until something does.
        heard: u64,
        /// When the venue was last asked for the subscriptions because it was quiet.
        asked: u64,
    },
}

/// The most names one `SUBSCRIBE` or `UNSUBSCRIBE` carries.
const NAMES_PER_FRAME: usize = 100;

/// One connection to the futures websocket.
#[derive(Debug, Clone)]
pub struct Session {
    url: String,
    limits: Limits,
    timing: Timing,
    wanted: Vec<Subscription>,
    link: Link,
    /// Reconnects since a socket last worked.
    failures: u32,
    next_id: u64,
    /// Names to subscribe and unsubscribe on the open socket, not yet sent.
    to_subscribe: Vec<String>,
    to_unsubscribe: Vec<String>,
    /// A quiet socket's question is owed.
    heartbeat: bool,
    /// When each frame this second went out.
    sent: VecDeque<u64>,
    /// What each request still unanswered asked, by id.
    waiting: BTreeMap<u64, Asked>,
    /// Each diff book stream's last `u` on this socket.
    chains: BTreeMap<String, u64>,
}

impl Session {
    /// A closed session on Aster's combined stream, with the default limits and timing.
    #[must_use]
    pub fn new() -> Self {
        Session::at(&Hosts::mainnet())
    }

    /// On `hosts`' websocket: a proxy, or a test.
    #[must_use]
    pub fn at(hosts: &Hosts) -> Self {
        Session {
            url: hosts.market_stream(),
            limits: Limits::DEFAULT,
            timing: Timing::DEFAULT,
            wanted: Vec::new(),
            link: Link::Closed,
            failures: 0,
            next_id: 1,
            to_subscribe: Vec::new(),
            to_unsubscribe: Vec::new(),
            heartbeat: false,
            sent: VecDeque::new(),
            waiting: BTreeMap::new(),
            chains: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    #[must_use]
    pub const fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    /// Where to connect.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self.link, Link::Open { .. })
    }

    /// What this session subscribes to on every socket, in the order it was asked for.
    #[must_use]
    pub fn wanted(&self) -> &[Subscription] {
        &self.wanted
    }

    /// Requests sent on this socket and not yet answered, by id.
    #[must_use]
    pub const fn waiting(&self) -> &BTreeMap<u64, Asked> {
        &self.waiting
    }

    /// Wants `subscription` from now on, on this socket and every later one. On an open socket
    /// it goes out with the next [`Session::owed`]. Past the limit, or one the venue cannot
    /// serve, it is refused and not kept.
    pub fn subscribe(&mut self, subscription: Subscription) -> Result<(), Refused> {
        if self.wanted.contains(&subscription) {
            return Ok(());
        }
        subscription.check()?;
        if self.wanted.len() >= self.limits.streams {
            return Err(Refused::Streams { limit: self.limits.streams });
        }
        let name = subscription.name();
        self.wanted.push(subscription);
        if self.is_open() {
            match self.to_unsubscribe.iter().position(|waiting| *waiting == name) {
                // Still subscribed on the venue: nothing to send.
                Some(at) => {
                    self.to_unsubscribe.remove(at);
                }
                None => self.to_subscribe.push(name),
            }
        }
        Ok(())
    }

    /// Stops wanting `subscription`. Gives whether it was wanted.
    pub fn unsubscribe(&mut self, subscription: &Subscription) -> bool {
        let Some(at) = self.wanted.iter().position(|wanted| wanted == subscription) else { return false };
        let gone = self.wanted.remove(at);
        let name = gone.name();
        self.chains.remove(&name);
        if self.is_open() {
            match self.to_subscribe.iter().position(|waiting| *waiting == name) {
                Some(at) => {
                    self.to_subscribe.remove(at);
                }
                None => self.to_unsubscribe.push(name),
            }
        }
        true
    }

    /// Follows the account stream with `listen_key` in place of any key followed before: after a
    /// `listenKeyExpired`, with the new key `POST /fapi/v3/listenKey` gave.
    pub fn set_listen_key(&mut self, listen_key: &str) -> Result<(), Refused> {
        let fresh = Subscription::account(listen_key);
        fresh.check()?;
        let old: Vec<Subscription> =
            self.wanted.iter().filter(|wanted| wanted.is_account() && **wanted != fresh).cloned().collect();
        for subscription in &old {
            self.unsubscribe(subscription);
        }
        self.subscribe(fresh)
    }

    /// Asks the venue what this socket streams; the answer comes as a [`Frame::Answer`] whose
    /// `result` lists the names.
    pub fn ask_subscriptions(&mut self) {
        if self.is_open() {
            self.heartbeat = true;
        }
    }

    /// The socket opened at `now`. Gives the frames to send at once: what is wanted, subscribed.
    pub fn opened(&mut self, now: u64) -> Vec<String> {
        self.link = Link::Open { since: now, heard: now, asked: now };
        self.reset_socket();
        self.to_subscribe = self.wanted.iter().map(Subscription::name).collect();
        self.owed(now)
    }

    /// Something that is not a text frame arrived at `now` (the venue's protocol ping): the
    /// socket is alive.
    pub fn heard(&mut self, now: u64) {
        if let Link::Open { heard, .. } = &mut self.link {
            *heard = (*heard).max(now);
        }
    }

    /// A text frame arrived at `now`. Anything that arrives shows the socket is alive, whether it
    /// reads or not. An answer or a refusal names what it answers; a diff book update that does
    /// not follow the last on its stream comes back as [`Frame::Gap`].
    pub fn received(&mut self, now: u64, text: &str) -> Result<Frame, BadFrame> {
        self.heard(now);
        Ok(match decode(text)? {
            Frame::Answer { id, result, .. } => Frame::Answer { id, result, asked: self.waiting.remove(&id) },
            Frame::Refusal { id, code, message, .. } => {
                let asked = id.and_then(|id| self.waiting.remove(&id));
                Frame::Refusal { id, code, message, asked }
            }
            Frame::Data { stream: Some(stream), data: Data::Depth(update) } if self.is_diff(&stream) => {
                match self.chains.insert(stream.clone(), update.final_id) {
                    Some(expected) if update.previous_final_id != expected => {
                        let gap = Gap { stream, expected, got: update.previous_final_id };
                        Frame::Gap { gap, update }
                    }
                    _ => Frame::Data { stream: Some(stream), data: Data::Depth(update) },
                }
            }
            other => other,
        })
    }

    /// The frames to send at `now`, and once: what is held back goes out as the venue's rate
    /// allows, the unsubscribes first, many names to a frame.
    pub fn owed(&mut self, now: u64) -> Vec<String> {
        if !self.is_open() {
            return Vec::new();
        }
        let mut frames = Vec::new();
        while self.room(now) > 0 {
            let asked = if !self.to_unsubscribe.is_empty() {
                Asked::Unsubscribe(take(&mut self.to_unsubscribe))
            } else if !self.to_subscribe.is_empty() {
                Asked::Subscribe(take(&mut self.to_subscribe))
            } else if std::mem::take(&mut self.heartbeat) {
                Asked::ListSubscriptions
            } else {
                break;
            };
            let id = self.next_id;
            self.next_id = self.next_id.saturating_add(1);
            frames.push(asked.frame(id));
            self.waiting.insert(id, asked);
            self.sent.push_back(now);
        }
        frames
    }

    fn has_owed(&self) -> bool {
        !self.to_subscribe.is_empty() || !self.to_unsubscribe.is_empty() || self.heartbeat
    }

    /// Frames that may still go out in the second up to `now`.
    fn room(&mut self, now: u64) -> usize {
        while self.sent.front().is_some_and(|at| at.saturating_add(1_000) <= now) {
            self.sent.pop_front();
        }
        self.limits.messages_per_second.saturating_sub(self.sent.len())
    }

    /// Whether `stream` is a wanted diff book stream.
    fn is_diff(&self, stream: &str) -> bool {
        self.wanted.iter().any(|wanted| wanted.is_diff_depth() && wanted.name() == stream)
    }

    /// What one socket held, forgotten.
    fn reset_socket(&mut self) {
        self.to_subscribe.clear();
        self.to_unsubscribe.clear();
        self.heartbeat = false;
        self.sent.clear();
        self.waiting.clear();
        self.chains.clear();
    }
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

/// Up to [`NAMES_PER_FRAME`] names from the front of `names`.
fn take(names: &mut Vec<String>) -> Vec<String> {
    let count = names.len().min(NAMES_PER_FRAME);
    names.drain(..count).collect()
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
