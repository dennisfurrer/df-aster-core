//! Aster's REST APIs behind one rate-limit gate, the part of a [`crate::client::Client`] that
//! calls out. Every request passes the gate and the meter; the [`Http`] underneath knows
//! nothing of hosts.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::client::Clock;
use crate::client::hosts::Hosts;
use crate::client::http::calls::{Call, Host};
use crate::client::http::limit::{Gate, Meter, Spent};
use crate::client::http::wire::{Http, Request, TransportError};

#[derive(Debug, Default)]
struct Limiter {
    gate: Gate,
    meter: Meter,
}

/// Aster's hosts, rate limited for one IP and one account.
pub(crate) struct Api<H> {
    http: H,
    hosts: Hosts,
    clock: Clock,
    limiter: Mutex<Limiter>,
}

impl<H: Http> Api<H> {
    pub(crate) fn new(http: H, hosts: Hosts, clock: Clock) -> Self {
        Api { http, hosts, clock, limiter: Mutex::new(Limiter::default()) }
    }

    pub(crate) fn with_hosts(mut self, hosts: Hosts) -> Self {
        self.hosts = hosts;
        self
    }

    pub(crate) const fn hosts(&self) -> &Hosts {
        &self.hosts
    }

    pub(crate) const fn http(&self) -> &H {
        &self.http
    }

    pub(crate) const fn clock(&self) -> Clock {
        self.clock
    }

    /// What went out over the last minute, and how long the gate stays closed.
    pub(crate) fn spent(&self) -> Spent {
        let now = (self.clock)();
        let mut limiter = self.limiter();
        let cooldown_ms = limiter.gate.cooldown_ms(now);
        Spent { cooldown_ms, ..limiter.meter.spent(now) }
    }

    fn limiter(&self) -> MutexGuard<'_, Limiter> {
        self.limiter.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The URL a call goes to, with `query` (already encoded) when it has one.
    pub(crate) fn url(&self, call: &Call, query: &str) -> String {
        let root = match call.host {
            Host::Futures => &self.hosts.rest,
            Host::Chain => &self.hosts.chain,
            Host::Web => &self.hosts.web,
        };
        match query.is_empty() {
            true => format!("{root}{}", call.path),
            false => format!("{root}{}?{query}", call.path),
        }
    }

    /// The request a call makes, its fields already encoded (and signed, when signed).
    pub(crate) fn request(&self, call: &Call, encoded: String) -> Request {
        match call.uses_query() {
            true => Request::new(call.method, self.url(call, &encoded)),
            false => Request::form(call.method, self.url(call, ""), encoded),
        }
    }

    /// Past the gate and the meter, then out. A request near the IP's budget, or past the
    /// account's order limits, is held back as [`TransportError::RateLimited`] without going
    /// out; a closed gate holds everything. Nothing is retried.
    pub(crate) async fn send(&self, call: &Call, encoded: String) -> Result<String, TransportError> {
        let ticket = {
            let now = (self.clock)();
            let mut limiter = self.limiter();
            let ticket = limiter.gate.open(now).map_err(|wait| rate_limited(Some(wait)))?;
            if let Some(wait) = limiter.meter.wait_for(now, call.weight, call.orders) {
                return Err(rate_limited(Some(wait)));
            }
            ticket
        };
        let answer = self.http.send(self.request(call, encoded)).await;
        let now = (self.clock)();
        let mut limiter = self.limiter();
        let response = answer?;
        if let Some(used) = response.used_weight() {
            limiter.meter.report(now, used);
        }
        let wait = match response.status {
            429 => Some(limiter.gate.struck(ticket, now, response.retry_after_ms())),
            418 => Some(limiter.gate.banned(ticket, now, response.retry_after_ms())),
            403 => Some(limiter.gate.blocked(ticket, now)),
            _ => None,
        };
        if let Some(wait) = wait {
            limiter.meter.refuse(now);
            return Err(rate_limited(Some(wait)));
        }
        limiter.meter.record(now, call.path, call.weight, call.orders);
        let text = response.text();
        match response.is_success() {
            true => {
                limiter.gate.passed(ticket);
                Ok(text)
            }
            false => Err(TransportError::Status { status: response.status, body: text }),
        }
    }
}

const fn rate_limited(retry_after_ms: Option<u64>) -> TransportError {
    TransportError::RateLimited { retry_after_ms }
}
