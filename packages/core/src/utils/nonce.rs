//! Microsecond nonces. Aster keeps each agent's last 100 nonces and refuses a repeat, or one
//! older than all of them, or one more than 60 seconds from its clock. One counter per agent
//! hands out the clock's microseconds, moved past the last one handed out, so two calls in the
//! same microsecond (or a clock that steps back) never share or reuse a nonce.

use std::sync::{Mutex, PoisonError};

/// The nonces of one agent.
#[derive(Debug, Default)]
pub struct Nonces(Mutex<u64>);

impl Nonces {
    /// The next nonce at `now_us`: `now_us`, or one past the last when that is later.
    pub fn next(&self, now_us: u64) -> u64 {
        let mut last = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let next = now_us.max(last.saturating_add(1));
        *last = next;
        next
    }

    /// The last nonce handed out; 0 before the first.
    #[must_use]
    pub fn last(&self) -> u64 {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "tests/nonce.rs"]
mod tests;
