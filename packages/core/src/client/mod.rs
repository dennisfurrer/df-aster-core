//! Connecting to Aster: the [`Client`] that reads, signs and sends over the rate-limited REST APIs
//! ([`http`]), the hosts, and the native and browser wires.

pub mod hosts;
pub mod http;
#[cfg(feature = "native")]
pub mod native;
#[cfg(feature = "web")]
pub mod web;

use std::sync::atomic::{AtomicI64, Ordering};

use crate::utils::Nonces;
use crate::utils::query::query;
use crate::utils::sign::Signed;
#[cfg(feature = "sign")]
use crate::utils::sign::{Agent, sign_params};
use crate::utils::time::ms_to_us;

pub use hosts::{ASTER_CHAIN, Hosts, WALLET_CHAIN_ID};
pub use http::{
    ApiError, Call, Host, Http, Kind, Method, OFFLINE, Request, Response, Spent, Transport, TransportError, codes,
    decode, read,
};

/// Unix milliseconds. Injected: std has no clock in the browser or Workers, and a test pins the
/// time.
pub type Clock = fn() -> u64;

/// What a client holds to sign an account's calls: nothing, or an [`Agent`].
pub trait Credentials {
    /// `fields` signed at `nonce`, or why not.
    fn sign(&self, fields: &[(&str, String)], nonce: u64) -> Result<Signed, TransportError>;

    /// The main account the calls are for, when there is one.
    fn user(&self) -> Option<crate::utils::Address>;
}

impl Credentials for () {
    fn sign(&self, _fields: &[(&str, String)], _nonce: u64) -> Result<Signed, TransportError> {
        Err(TransportError::NoAgent("an account call".to_owned()))
    }

    fn user(&self) -> Option<crate::utils::Address> {
        None
    }
}

#[cfg(feature = "sign")]
impl Credentials for Agent {
    fn sign(&self, fields: &[(&str, String)], nonce: u64) -> Result<Signed, TransportError> {
        sign_params(fields, nonce, self).map_err(|error| TransportError::Unsigned(error.to_string()))
    }

    fn user(&self) -> Option<crate::utils::Address> {
        Some(self.user)
    }
}

/// Reads for any module (`markets::exchange_info(&client)`) and, with an [`Agent`], the
/// account's own calls. `K` is `()` for a client that only reads.
///
/// Every signed call carries a fresh microsecond nonce from one counter, so give each agent one
/// client. A nonce the venue calls expired is signed again at a fresh one, once; nothing else is
/// retried.
pub struct Client<H, K = ()> {
    api: http::Api<H>,
    key: K,
    nonces: Nonces,
    /// The venue's clock less this machine's, in milliseconds ([`Client::sync_clock`]).
    offset_ms: AtomicI64,
    /// The deadline a signed POST, PUT or DELETE carries (`nonceWindow`), when set.
    nonce_window_ms: Option<u32>,
}

/// Its hosts; never its key or its transport.
impl<H: Http, K> std::fmt::Debug for Client<H, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("hosts", self.hosts()).finish_non_exhaustive()
    }
}

impl<H: Http> Client<H> {
    /// A client that reads and never signs.
    pub fn reader(http: H, clock: Clock) -> Self {
        Client::new(http, clock, ())
    }
}

impl<H: Http, K> Client<H, K> {
    /// Aster's own hosts, through `http`, signing with `key`.
    pub fn new(http: H, clock: Clock, key: K) -> Self {
        Client {
            api: http::Api::new(http, Hosts::mainnet(), clock),
            key,
            nonces: Nonces::default(),
            offset_ms: AtomicI64::new(0),
            nonce_window_ms: None,
        }
    }

    /// Other hosts: a proxy, or a test's fake venue.
    #[must_use]
    pub fn with_hosts(mut self, hosts: Hosts) -> Self {
        self.api = self.api.with_hosts(hosts);
        self
    }

    /// Every signed POST, PUT and DELETE carries `nonceWindow`: the sequencer refuses it
    /// ([`codes::NONCE_WINDOW_PASSED`]) rather than run it more than `ms` after it was signed.
    /// 1 to 60,000; anything else is clamped.
    #[must_use]
    pub fn with_nonce_window(mut self, ms: u32) -> Self {
        self.nonce_window_ms = Some(ms.clamp(1, 60_000));
        self
    }

    #[must_use]
    pub const fn hosts(&self) -> &Hosts {
        self.api.hosts()
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        self.api.http()
    }

    #[must_use]
    pub const fn key(&self) -> &K {
        &self.key
    }

    /// Now, by the venue's clock as far as this client knows it: unix milliseconds.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        let local = (self.api.clock())();
        local.saturating_add_signed(self.offset_ms.load(Ordering::Relaxed))
    }

    /// The venue's clock less this machine's, in milliseconds: 0 until [`Client::sync_clock`].
    #[must_use]
    pub fn clock_offset_ms(&self) -> i64 {
        self.offset_ms.load(Ordering::Relaxed)
    }

    /// Reads the venue's time and keeps the difference from this machine's clock, so nonces stay
    /// within the venue's 60 seconds on a machine whose clock is off. Gives the offset.
    pub async fn sync_clock(&self) -> Result<i64, ApiError> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Time {
            server_time: i64,
        }
        let before = (self.api.clock())();
        let time: Time = read(&Unsigned(self), Call::get("/fapi/v3/time")).await?;
        let after = (self.api.clock())();
        let middle = i64::try_from(before / 2 + after / 2).unwrap_or(i64::MAX);
        let offset = time.server_time.saturating_sub(middle);
        self.offset_ms.store(offset, Ordering::Relaxed);
        Ok(offset)
    }

    /// What went out over the last minute, and whether the gate is closed.
    pub fn spent(&self) -> Spent {
        self.api.spent()
    }

    /// Opens, or refreshes, the connection orders travel on, with `ping`, counted like any other
    /// read. Nothing is signed and the answer is dropped: a program warms before its first order
    /// so that order skips DNS and the TLS handshake.
    pub async fn warm(&self) {
        let _ = self.api.send(&Call::get("/fapi/v3/ping"), String::new()).await;
    }

    /// The next nonce: the venue's clock in microseconds, past the last one handed out.
    #[must_use]
    pub fn next_nonce(&self) -> u64 {
        self.nonces.next(ms_to_us(self.now_ms()))
    }
}

impl<H: Http, K: Credentials> Client<H, K> {
    /// The main account this client's agent acts for.
    #[must_use]
    pub fn user(&self) -> Option<crate::utils::Address> {
        self.key.user()
    }

    /// Signs `call`'s fields at the next nonce, for a program that sends it itself.
    pub fn sign(&self, call: &Call) -> Result<Signed, TransportError> {
        self.key.sign(&self.fields_of(call), self.next_nonce())
    }

    fn fields_of(&self, call: &Call) -> Vec<(&'static str, String)> {
        let mut fields = call.fields.clone();
        if let Some(window) = self.nonce_window_ms
            && call.method != Method::Get
            && call.value("nonceWindow").is_none()
        {
            fields.push(("nonceWindow", window.to_string()));
        }
        fields
    }
}

impl<H: Http, K: Credentials> Transport for Client<H, K> {
    async fn call(&self, call: Call) -> Result<String, TransportError> {
        if !call.signed {
            return self.api.send(&call, query(&call.fields)).await;
        }
        let fields = self.fields_of(&call);
        let mut again = true;
        loop {
            let signed = self.key.sign(&fields, self.next_nonce())?;
            let answer = self.api.send(&call, signed.encoded()).await;
            let expired = match &answer {
                Ok(text) => http::calls::refusal(text),
                Err(TransportError::Status { body, .. }) => http::calls::refusal(body),
                Err(_) => None,
            }
            .is_some_and(|refused| refused.is_nonce());
            if !(expired && std::mem::take(&mut again)) {
                return answer;
            }
        }
    }
}

/// A client's transport for a call that is never signed, whatever it holds.
struct Unsigned<'a, H, K>(&'a Client<H, K>);

impl<H: Http, K> Transport for Unsigned<'_, H, K> {
    async fn call(&self, call: Call) -> Result<String, TransportError> {
        self.0.api.send(&call, query(&call.fields)).await
    }
}

#[cfg(test)]
#[cfg(feature = "sign")]
#[path = "tests/client.rs"]
mod tests;
