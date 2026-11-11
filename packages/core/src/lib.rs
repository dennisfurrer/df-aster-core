//! Aster SDK for Rust: Aster's futures (perpetuals) API.
//!
//! - [`client`]: the [`Client`], the REST APIs over HTTP, rate limits, agent signing, the hosts,
//!   and the native and browser wires.
//! - [`markets`]: what is listed and what it trades at.
//! - [`account`]: what an account holds, its history, its settings and its agents.
//! - [`trade`]: orders, cancels, modifies, leverage and margin.
//! - [`transfer`]: between futures and spot, withdrawals, deposits, sub-accounts.
//! - [`builder`]: what a builder reads about the users that approved it.
//! - [`live`]: websocket streams, market and account, and a kept book.
//! - [`utils`]: decimals, addresses, enums, nonces and the signing primitives.
//!
//! Money is exact decimals, never floats. Features: `sign` and `native` (default), `web`.

pub mod account;
pub mod builder;
pub mod client;
pub mod live;
pub mod markets;
pub mod trade;
pub mod transfer;
pub mod utils;

#[cfg(test)]
pub(crate) mod test_support;

pub use client::{Client, Hosts};
pub use utils::{Address, Dec};
