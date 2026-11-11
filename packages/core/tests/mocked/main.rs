//! Whole journeys through a [`Client`](df_aster_core::Client) against a fake Aster that checks
//! every signature the way the venue does: an agent's against the agents the main wallet
//! approved, a main wallet's against typed data it rebuilds from the request's own fields.
//! Nothing leaves the process.

#![cfg(feature = "sign")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod onboarding;
mod trade;
mod venue;
