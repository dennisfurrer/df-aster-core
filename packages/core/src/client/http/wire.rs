//! The [`Http`] wire: one request out, with its method, headers and body, and its answer back
//! whatever the status. A host or a dependent plugs in here.

use std::future::Future;

/// One HTTP request out and its answer back, whatever the status. It knows nothing of hosts or
/// limits; an error means no answer at all ([`TransportError::Unreachable`]).
pub trait Http {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// A request with no body.
    #[must_use]
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Request { method, url: url.into(), headers: Vec::new(), body: Vec::new() }
    }

    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Request::new(Method::Get, url)
    }

    /// `body`, already encoded, as `application/x-www-form-urlencoded`: what every POST and PUT
    /// under `/fapi` takes.
    #[must_use]
    pub fn form(method: Method, url: impl Into<String>, body: String) -> Self {
        Request {
            method,
            url: url.into(),
            headers: vec![("Content-Type".to_owned(), "application/x-www-form-urlencoded".to_owned())],
            body: body.into_bytes(),
        }
    }

    /// A JSON body, posted as `application/json`: what a JSON-RPC node and the chain's `/info`
    /// take.
    #[must_use]
    pub fn post_json(url: impl Into<String>, body: String) -> Self {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            body: body.into_bytes(),
        }
    }

    /// The same request with one more header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// A header's value, its name matched whatever its case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// Whether it changes the account: a POST, PUT or DELETE that carries a signature, in its
    /// query or its body. A signed GET only reads.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        const FIELD: &[u8] = b"signature=";
        let query = self.url.split_once('?').map_or("", |(_, query)| query);
        let carries = |bytes: &[u8]| {
            bytes
                .windows(FIELD.len())
                .enumerate()
                .any(|(at, window)| window == FIELD && (at == 0 || bytes[at - 1] == b'&' || bytes[at - 1] == b'?'))
        };
        self.method != Method::Get && (carries(query.as_bytes()) || carries(&self.body))
    }

    /// [`TransportError::Offline`] for a signed request in an `offline` build; otherwise nothing.
    pub fn refuse_if_offline(&self) -> Result<(), TransportError> {
        match OFFLINE && self.is_signed() {
            true => Err(TransportError::Offline(self.url.split('?').next().unwrap_or_default().to_owned())),
            false => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// A header's value, its name matched whatever its case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// `Retry-After` in milliseconds, when it is given in whole seconds.
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        let seconds = self.header("retry-after").and_then(|seconds| seconds.trim().parse::<u64>().ok())?;
        Some(seconds.saturating_mul(1_000))
    }

    /// The weight the venue counts against this IP over the last minute
    /// (`X-MBX-USED-WEIGHT-1M`), when it says.
    #[must_use]
    pub fn used_weight(&self) -> Option<u32> {
        self.header("x-mbx-used-weight-1m").and_then(|weight| weight.trim().parse().ok())
    }

    /// The body as text; bytes that are not UTF-8 read as U+FFFD.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// No answer came back. For an order or a transfer that means unknown, not failed: it may
    /// have landed, so it is never sent again; read the account to find out.
    #[error("no answer from the venue: {0}")]
    Unreachable(String),
    #[error("the venue answered HTTP {status}: {body}")]
    Status { status: u16, body: String },
    /// Refused for the rate limit, by the venue (a 429, a 418 ban, or its firewall's 403) or by
    /// the [`crate::client::Client`]'s gate before it went out. Nothing was done; try again after
    /// the wait.
    #[error("rate limited; try again in {} ms", retry_after_ms.unwrap_or(0))]
    RateLimited { retry_after_ms: Option<u64> },
    /// Not sent: this build has the `offline` feature, and the request changes the account.
    #[error("an offline build sends nothing signed: {0}")]
    Offline(String),
    /// Not sent: the call is an account's own and this client holds no agent to sign it with.
    #[error("{0} is signed by an agent: give the client one")]
    NoAgent(String),
    /// Not sent: the agent did not sign.
    #[error("the agent could not sign {0}")]
    Unsigned(String),
}

/// Whether this build's transports refuse what changes the account ([`TransportError::Offline`]).
pub const OFFLINE: bool = cfg!(feature = "offline");

#[cfg(test)]
#[path = "tests/wire.rs"]
mod tests;
