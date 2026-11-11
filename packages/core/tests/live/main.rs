//! Against Aster itself, read only, run by hand (`just live`): every public read once, its answer
//! held against its recording (every key the recording has is still there) and read into its
//! type; then the market streams for a few seconds. Nothing is signed.

#![cfg(feature = "native")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod markets;
mod shape;
mod streams;
