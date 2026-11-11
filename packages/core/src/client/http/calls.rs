//! The [`Transport`] every read and send goes through, the [`Call`] it carries (a path on one of
//! Aster's hosts, its fields in order, and whether the agent signs it), and reading the venue's
//! answer: the value asked for, or its refusal by code.

use std::future::Future;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::client::http::limit;
use crate::client::http::wire::{Method, TransportError};

/// Which of Aster's REST hosts a call goes to ([`crate::client::Hosts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Host {
    /// The futures API: `/fapi/v3/*`.
    Futures,
    /// Aster Chain's API: `/aster-chain/v3/*`, withdrawals and transfers on the chain.
    Chain,
    /// The web app's API: `/bapi/*`, deposit and withdrawal asset lists.
    Web,
}

/// One request to Aster: its method, its host and path, its fields in order, whether the agent
/// signs it, and its weight against the IP's minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: Method,
    pub host: Host,
    pub path: &'static str,
    pub fields: Vec<(&'static str, String)>,
    /// An account call: the client adds `nonce`, `user`, `signer` and the agent's `signature`.
    pub signed: bool,
    /// Its fields go in the query string even on a POST (`approveAgent`, as Aster's web app
    /// sends it). GET and DELETE always use the query; POST and PUT a form body.
    pub in_query: bool,
    /// What it costs against the IP's 2,400 a minute ([`limit::weight`] unless set).
    pub weight: u32,
    /// It places or changes orders: it counts against the account's order limits too.
    pub orders: u32,
}

impl Call {
    /// A call to `path` on the futures host; its weight from the docs' table.
    #[must_use]
    pub fn new(method: Method, path: &'static str) -> Call {
        Call {
            method,
            host: Host::Futures,
            path,
            fields: Vec::new(),
            signed: false,
            in_query: false,
            weight: limit::weight(method, path),
            orders: 0,
        }
    }

    #[must_use]
    pub fn get(path: &'static str) -> Call {
        Call::new(Method::Get, path)
    }

    #[must_use]
    pub fn post(path: &'static str) -> Call {
        Call::new(Method::Post, path)
    }

    #[must_use]
    pub fn put(path: &'static str) -> Call {
        Call::new(Method::Put, path)
    }

    #[must_use]
    pub fn delete(path: &'static str) -> Call {
        Call::new(Method::Delete, path)
    }

    /// On another host.
    #[must_use]
    pub const fn on(mut self, host: Host) -> Call {
        self.host = host;
        self
    }

    /// One more field.
    #[must_use]
    pub fn field(mut self, key: &'static str, value: impl ToString) -> Call {
        self.fields.push((key, value.to_string()));
        self
    }

    /// One more field when there is a value.
    #[must_use]
    pub fn maybe(self, key: &'static str, value: Option<impl ToString>) -> Call {
        match value {
            Some(value) => self.field(key, value),
            None => self,
        }
    }

    /// Signed by the agent.
    #[must_use]
    pub const fn signed(mut self) -> Call {
        self.signed = true;
        self
    }

    /// Its fields in the query string, whatever the method.
    #[must_use]
    pub const fn query_only(mut self) -> Call {
        self.in_query = true;
        self
    }

    /// Its weight, where it depends on what is asked (a book's depth, a list's length).
    #[must_use]
    pub const fn weigh(mut self, weight: u32) -> Call {
        self.weight = weight;
        self
    }

    /// It places or changes `count` orders.
    #[must_use]
    pub const fn places(mut self, count: u32) -> Call {
        self.orders = count;
        self
    }

    /// The field a value was given for.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|(name, _)| *name == key).map(|(_, value)| value.as_str())
    }

    /// Whether its fields go in the query string.
    #[must_use]
    pub const fn uses_query(&self) -> bool {
        self.in_query || matches!(self.method, Method::Get | Method::Delete)
    }
}

/// Sends a [`Call`] and returns the answer's text, whatever its `code`.
/// [`crate::client::Client`] implements it behind the rate limit, signing what is signed. No
/// `Send` bound, since the browser is single-threaded.
pub trait Transport {
    fn call(&self, call: Call) -> impl Future<Output = Result<String, TransportError>>;
}

/// Why a read or a send came to nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("the venue's answer on {path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },
    /// The venue's own `{"code":..,"msg":..}`: the code says why ([`ApiError::code`]), the
    /// message is shown as it came.
    #[error("refused ({code}): {message}")]
    Refused { code: i64, message: String },
}

/// Codes worth telling apart, from the docs' "Error Codes".
pub mod codes {
    pub const UNKNOWN: i64 = -1000;
    pub const DISCONNECTED: i64 = -1001;
    pub const UNAUTHORIZED: i64 = -1002;
    pub const TOO_MANY_REQUESTS: i64 = -1003;
    /// "Execution status unknown": the order may have been placed.
    pub const UNEXPECTED_RESP: i64 = -1006;
    /// "Send status unknown; execution status unknown."
    pub const TIMEOUT: i64 = -1007;
    pub const TOO_MANY_ORDERS: i64 = -1015;
    pub const INVALID_SIGNATURE: i64 = -1022;
    pub const MANDATORY_PARAM_EMPTY_OR_MALFORMED: i64 = -1102;
    pub const PARAM_NOT_REQUIRED: i64 = -1106;
    pub const BAD_PRECISION: i64 = -1111;
    pub const BAD_SYMBOL: i64 = -1121;
    pub const INVALID_LISTEN_KEY: i64 = -1125;
    pub const INVALID_PARAMETER: i64 = -1130;
    pub const NEW_ORDER_REJECTED: i64 = -2010;
    pub const CANCEL_REJECTED: i64 = -2011;
    pub const NO_SUCH_ORDER: i64 = -2013;
    pub const BAD_API_KEY_FMT: i64 = -2014;
    pub const REJECTED_MBX_KEY: i64 = -2015;
    pub const BALANCE_NOT_SUFFICIENT: i64 = -2018;
    pub const MARGIN_NOT_SUFFICIENT: i64 = -2019;
    pub const UNABLE_TO_FILL: i64 = -2020;
    pub const ORDER_WOULD_IMMEDIATELY_TRIGGER: i64 = -2021;
    pub const REDUCE_ONLY_REJECT: i64 = -2022;
    pub const USER_IN_LIQUIDATION: i64 = -2023;
    pub const POSITION_NOT_SUFFICIENT: i64 = -2024;
    pub const MAX_OPEN_ORDER_EXCEEDED: i64 = -2025;
    pub const PRICE_NOT_INCREASED_BY_TICK_SIZE: i64 = -4014;
    pub const QTY_NOT_INCREASED_BY_STEP_SIZE: i64 = -4023;
    pub const NO_NEED_TO_CHANGE_MARGIN_TYPE: i64 = -4046;
    pub const CROSS_BALANCE_INSUFFICIENT: i64 = -4050;
    pub const ISOLATED_BALANCE_INSUFFICIENT: i64 = -4051;
    pub const NO_NEED_TO_CHANGE_POSITION_SIDE: i64 = -4059;
    pub const MIN_NOTIONAL: i64 = -4164;
    /// Not in multi-assets mode, which an asset exchange needs.
    pub const NOT_JOINT_MARGIN: i64 = -4212;
    /// The nonce was older than the agent's last hundred, or out of the venue's window.
    pub const NONCE_EXPIRED: i64 = -4225;
    /// The sequencer did not take it within `nonceWindow`: it was never executed.
    pub const NONCE_WINDOW_PASSED: i64 = -4227;
    /// The main wallet has never deposited, which every account call needs.
    pub const DEPOSIT_REQUIRED: i64 = -5050;
}

impl ApiError {
    /// The venue's code, when it gave one.
    #[must_use]
    pub const fn code(&self) -> Option<i64> {
        match self {
            ApiError::Refused { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// Refused for the rate limit, here or by the venue: wait and ask again.
    #[must_use]
    pub const fn is_rate_limited(&self) -> bool {
        matches!(self, ApiError::Transport(TransportError::RateLimited { .. }))
            || matches!(self.code(), Some(codes::TOO_MANY_REQUESTS | codes::TOO_MANY_ORDERS))
    }

    /// The nonce was refused: sign again with a fresh one.
    #[must_use]
    pub const fn is_nonce(&self) -> bool {
        matches!(self.code(), Some(codes::NONCE_EXPIRED))
    }

    /// The agent is not, or no longer, approved for the account, or its signature did not hold.
    #[must_use]
    pub const fn is_unauthorized(&self) -> bool {
        matches!(
            self.code(),
            Some(codes::UNAUTHORIZED | codes::INVALID_SIGNATURE | codes::BAD_API_KEY_FMT | codes::REJECTED_MBX_KEY)
        )
    }

    /// The account must deposit once before any account call ([`codes::DEPOSIT_REQUIRED`]).
    #[must_use]
    pub const fn needs_deposit(&self) -> bool {
        matches!(self.code(), Some(codes::DEPOSIT_REQUIRED))
    }

    /// Refused for want of margin or balance.
    #[must_use]
    pub const fn is_insufficient(&self) -> bool {
        matches!(
            self.code(),
            Some(
                codes::BALANCE_NOT_SUFFICIENT
                    | codes::MARGIN_NOT_SUFFICIENT
                    | codes::CROSS_BALANCE_INSUFFICIENT
                    | codes::ISOLATED_BALANCE_INSUFFICIENT
            )
        )
    }

    /// Whether what this answered may have been done: no answer, an unreadable one, a 5xx (a
    /// 503 is "execution status unknown"), or the codes that say the same. A refusal with any
    /// other code, or a rate limit, was turned away before it counted.
    #[must_use]
    pub const fn may_have_landed(&self) -> bool {
        match self {
            ApiError::Transport(TransportError::Unreachable(_)) | ApiError::Unreadable { .. } => true,
            ApiError::Transport(TransportError::Status { status, .. }) => *status >= 500,
            ApiError::Transport(_) => false,
            ApiError::Refused { code, .. } => {
                matches!(*code, codes::UNKNOWN | codes::DISCONNECTED | codes::UNEXPECTED_RESP | codes::TIMEOUT)
            }
        }
    }
}

/// `code` and `msg`, which an answer may carry. `code` comes as a number or a string.
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// One read or send, not retried: the caller decides.
pub async fn read<R: DeserializeOwned>(transport: &impl Transport, call: Call) -> Result<R, ApiError> {
    let path = call.path;
    match transport.call(call).await {
        Ok(text) => decode(path, &text),
        Err(TransportError::Status { status, body }) => match refusal(&body) {
            Some(refused) => Err(refused),
            None => Err(ApiError::Transport(TransportError::Status { status, body })),
        },
        Err(error) => Err(ApiError::Transport(error)),
    }
}

/// An answer's text read as `R`, or the venue's refusal: a negative `code`.
pub fn decode<R: DeserializeOwned>(path: &str, text: &str) -> Result<R, ApiError> {
    if let Some(refused) = refusal(text) {
        return Err(refused);
    }
    serde_json::from_str(text)
        .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: unreadable(&error, text) })
}

/// The refusal an answer carries, if it is one: a negative `code`, or on the chain's and web
/// app's hosts a `code` that is neither 0, 200 nor `"000000"`.
pub(crate) fn refusal(text: &str) -> Option<ApiError> {
    if !text.trim_start().starts_with('{') {
        return None;
    }
    let envelope = serde_json::from_str::<Envelope>(text).ok()?;
    let code = match envelope.code? {
        serde_json::Value::Number(number) => number.as_i64()?,
        serde_json::Value::String(text) => match text.trim() {
            "000000" | "0" | "200" => return None,
            other => other.parse().ok()?,
        },
        _ => return None,
    };
    match code {
        0 | 200 => None,
        code => Some(ApiError::Refused { code, message: envelope.msg.or(envelope.message).unwrap_or_default() }),
    }
}

pub(crate) fn unreadable(error: &serde_json::Error, text: &str) -> String {
    let shown: String = text.chars().take(300).collect();
    format!("{error} in {shown}")
}

#[cfg(test)]
#[path = "tests/calls.rs"]
mod tests;
