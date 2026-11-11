//! Every signature this crate makes, held to what Aster's own reference code makes from the same
//! inputs: eth_account running the signing functions of Aster's official Python demo
//! (`vectors/aster_python.json`, made by `vectors/gen/gen.py`), and globe.li's TypeScript on viem
//! (`vectors/aster_web.json`).

#![cfg(feature = "sign")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod actions;
mod messages;
mod register;
mod vector;
