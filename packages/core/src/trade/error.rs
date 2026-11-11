//! Why a trade call was refused here, before anything was sent, or how the venue answered it.

use crate::client::ApiError;
use crate::utils::{OffGrid, OrderType};

/// A refusal made here: the venue would refuse the same, so nothing is sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    /// The docs' "additional mandatory parameters": `type` needs `param`.
    #[error("a {order_type} order needs `{param}`")]
    Missing { order_type: OrderType, param: &'static str },
    /// The venue answers `PARAM_NOT_REQUIRED` (-1106) for a parameter that does not apply.
    #[error("a {order_type} order takes no `{param}`")]
    NotTaken { order_type: OrderType, param: &'static str },
    /// Two parameters that may not be sent together.
    #[error("`{0}` and `{1}` may not be sent together")]
    Conflict(&'static str, &'static str),
    /// A client id outside `^[.A-Z:/a-z0-9_-]{1,36}$` (or 28 for a strategy's).
    #[error("`{0}` is not a valid client id")]
    ClientId(String),
    /// A figure out of the range the venue takes.
    #[error("`{param}` {value} is out of range: {range}")]
    OutOfRange { param: &'static str, value: String, range: &'static str },
    /// A list of the wrong length.
    #[error("{what} takes {range} items, not {count}")]
    Count { what: &'static str, count: usize, range: &'static str },
    /// The order is off the symbol's grid and could not be fitted.
    #[error(transparent)]
    OffGrid(#[from] OffGrid),
}

/// Why a trade call came to nothing: refused here, or by the venue (or no answer).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TradeError {
    #[error(transparent)]
    Invalid(#[from] Invalid),
    #[error(transparent)]
    Api(#[from] ApiError),
}

impl TradeError {
    /// The venue's answer, when it was sent.
    #[must_use]
    pub const fn api(&self) -> Option<&ApiError> {
        match self {
            TradeError::Api(error) => Some(error),
            TradeError::Invalid(_) => None,
        }
    }

    /// Whether it may have been done: only when it was sent and the answer says it may have.
    #[must_use]
    pub const fn may_have_landed(&self) -> bool {
        match self {
            TradeError::Api(error) => error.may_have_landed(),
            TradeError::Invalid(_) => false,
        }
    }
}

impl From<OffGrid> for TradeError {
    fn from(error: OffGrid) -> Self {
        TradeError::Invalid(Invalid::OffGrid(error))
    }
}

/// Whether `id` matches `^[.A-Z:/a-z0-9_-]{1,most}$`.
pub(crate) fn valid_client_id(id: &str, most: usize) -> bool {
    !id.is_empty()
        && id.len() <= most
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'/' | b'_' | b'-'))
}
