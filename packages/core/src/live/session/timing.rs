//! The session's clock: its rules in milliseconds ([`Timing`]), what a tick asks of the socket's
//! owner, and how long a closed socket waits before it connects again.
//!
//! The venue pings every 5 minutes at the protocol level and closes a socket that has not
//! answered in 15; the socket libraries answer those pings themselves, and a browser shows them
//! to no one. So the session tells a dead socket by silence: after [`Timing::quiet_ms`] with
//! nothing received it asks the venue for its subscriptions (`LIST_SUBSCRIPTIONS`), and a socket
//! that answers nothing at all for [`Timing::silent_ms`] is given up.

use crate::live::session::{Link, Session};

/// The clock's rules, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// After this long with nothing received, ask the venue for the subscriptions: its answer
    /// shows the socket still works.
    pub quiet_ms: u64,
    /// A socket nothing has arrived on for this long, not even that answer, is dead.
    pub silent_ms: u64,
    /// A socket open this long that heard something worked: the next failure starts the
    /// reconnect delays over. One dropped sooner keeps backing off.
    pub steady_ms: u64,
    /// The first reconnect delay; each failure after doubles it.
    pub first_retry_ms: u64,
    /// The longest reconnect delay, before jitter adds up to a quarter.
    pub last_retry_ms: u64,
    /// Reconnect once a socket has been open this long, before the venue cuts it at 24 hours.
    pub renew_after_ms: u64,
}

impl Timing {
    /// A question after 60 s of quiet; dead after 90 s of nothing; reconnects 250 ms doubling to
    /// 20 s; a fresh socket after 23 hours 50 minutes.
    pub const DEFAULT: Timing = Timing {
        quiet_ms: 60_000,
        silent_ms: 90_000,
        steady_ms: 30_000,
        first_retry_ms: 250,
        last_retry_ms: 20_000,
        renew_after_ms: 85_800_000,
    };
}

impl Default for Timing {
    fn default() -> Self {
        Timing::DEFAULT
    }
}

/// What a tick asks of the socket's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Wait,
    /// Send what [`Session::owed`] gives now.
    Send,
    /// Nothing has arrived for too long: close the socket, call [`Session::closed`], and connect
    /// again after the delay it gives.
    Reconnect,
    /// The socket is near the venue's 24 hours: close it and connect again now, as for
    /// [`Tick::Reconnect`].
    Renew,
}

/// What a closed socket leaves behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    /// Wait this long, then connect again.
    pub after_ms: u64,
}

impl Session {
    /// What the socket needs at `now`: a fresh socket near 24 hours, a reconnect after
    /// [`Timing::silent_ms`] of nothing, a question after [`Timing::quiet_ms`] of quiet, or the
    /// frames held back for the rate limit once there is room.
    pub fn tick(&mut self, now: u64) -> Tick {
        let Link::Open { since, heard, asked } = self.link else { return Tick::Wait };
        if now.saturating_sub(since) >= self.timing.renew_after_ms {
            return Tick::Renew;
        }
        if now.saturating_sub(heard) >= self.timing.silent_ms {
            return Tick::Reconnect;
        }
        if now.saturating_sub(heard) >= self.timing.quiet_ms && now.saturating_sub(asked) >= self.timing.quiet_ms {
            self.link = Link::Open { since, heard, asked: now };
            self.heartbeat = true;
        }
        match self.has_owed() && self.room(now) > 0 {
            true => Tick::Send,
            false => Tick::Wait,
        }
    }

    /// When a tick next has something to do; `None` while closed.
    #[must_use]
    pub fn due(&self) -> Option<u64> {
        let Link::Open { since, heard, asked } = self.link else { return None };
        let renew = since.saturating_add(self.timing.renew_after_ms);
        let silent = heard.saturating_add(self.timing.silent_ms);
        let quiet = heard.max(asked).saturating_add(self.timing.quiet_ms);
        let mut due = renew.min(silent).min(quiet);
        if self.has_owed()
            && let Some(first) = self.sent.front()
        {
            due = due.min(first.saturating_add(1_000));
        }
        Some(due)
    }

    /// The socket closed at `now`, or a connect failed. Gives how long to wait before the next
    /// connect. The wait doubles from [`Timing::first_retry_ms`] to [`Timing::last_retry_ms`] and
    /// `random` adds up to a quarter more, so many clients dropped at once do not return at once.
    /// It starts over after a socket that held ([`Timing::steady_ms`]). There is no last attempt.
    pub fn closed(&mut self, now: u64, random: u64) -> Retry {
        if let Link::Open { since, heard, .. } = self.link
            && heard > since
            && now.saturating_sub(since) >= self.timing.steady_ms
        {
            self.failures = 0;
        }
        self.link = Link::Closed;
        self.reset_socket();
        let doubled = 1u64.checked_shl(self.failures).unwrap_or(u64::MAX);
        let step = self.timing.first_retry_ms.saturating_mul(doubled).min(self.timing.last_retry_ms);
        let jitter = random % (step / 4).saturating_add(1);
        self.failures = self.failures.saturating_add(1);
        Retry { after_ms: step.saturating_add(jitter) }
    }

    /// Closed to dial again at once, after [`Tick::Renew`]: no failure counted, no wait.
    pub fn renewed(&mut self) -> Retry {
        self.link = Link::Closed;
        self.reset_socket();
        Retry { after_ms: 0 }
    }
}

#[cfg(test)]
#[path = "tests/timing.rs"]
mod tests;
