//! HTTP to Aster's REST APIs: the [`Http`] wire, the [`Transport`] every read and send goes
//! through, and the rate limit.

mod api;
pub mod calls;
pub mod limit;
pub mod wire;

pub(crate) use api::Api;
pub use calls::{ApiError, Call, Host, Transport, codes, decode, read};
pub use limit::{Kind, Spent};
pub use wire::{Http, Method, OFFLINE, Request, Response, TransportError};
