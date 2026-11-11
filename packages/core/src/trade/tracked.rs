//! Calls signed at a nonce the program chooses, through a [`Client`] that holds an [`Agent`].
//!
//! Aster keys an order to the nonce it was signed with, and two calls take this further:
//!
//! - a guarded cancel (`DELETE /fapi/v3/guardedCancelOrder`, `DELETE
//!   /fapi/v3/guardedBatchOrders`) is signed at the nonce the order was placed with, not a new
//!   one, so a cancel can never be replayed against a later order;
//! - a noop (`POST /fapi/v3/noop`) signed at the nonce of an order still queued for the chain
//!   takes that nonce first, so the order is refused as expired (-4225). It is not certain to
//!   win the race.
//!
//! [`place_tracked`] places an order and gives back the nonce it was signed at, whatever the
//! answer, so the program can guard a cancel or race a noop against it. None of these is signed
//! again on a refused nonce.

use crate::client::{ApiError, Call, Client, Http, TransportError, read};
use crate::trade::Order;
use crate::trade::error::{Invalid, TradeError};
use crate::trade::manage::{OrderRef, id_lists};
use crate::trade::orders::{NewOrder, place_call};
use crate::utils::Done;
use crate::utils::sign::{Agent, sign_params};

/// An answer and the nonce its call was signed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracked<T> {
    pub nonce: u64,
    pub answer: Result<T, ApiError>,
}

/// `call` signed by `agent` at `nonce`, as a call that carries its own `nonce`, `user`,
/// `signer` and `signature`, so the client sends it as it is.
pub fn signed_at(call: Call, nonce: u64, agent: &Agent) -> Result<Call, TransportError> {
    let signed =
        sign_params(&call.fields, nonce, agent).map_err(|error| TransportError::Unsigned(error.to_string()))?;
    let mut fields = call.fields.clone();
    fields.push(("nonce", nonce.to_string()));
    fields.push(("user", agent.user.to_checksum()));
    fields.push(("signer", agent.signer().to_checksum()));
    fields.push(("signature", signed.signature.to_hex()));
    Ok(Call { fields, signed: false, ..call })
}

async fn send_at<H: Http, T: serde::de::DeserializeOwned>(
    client: &Client<H, Agent>,
    call: Call,
    nonce: u64,
) -> Result<T, ApiError> {
    let call = signed_at(call, nonce, client.key())?;
    read(client, call).await
}

/// Places `order` at the client's next nonce and gives back that nonce with the answer.
pub async fn place_tracked<H: Http>(client: &Client<H, Agent>, order: &NewOrder) -> Result<Tracked<Order>, Invalid> {
    let call = place_call(order)?;
    let nonce = client.next_nonce();
    Ok(Tracked { nonce, answer: send_at(client, call, nonce).await })
}

/// Cancels an order with the nonce it was placed at (`placed_nonce`).
pub async fn guarded_cancel<H: Http>(
    client: &Client<H, Agent>,
    symbol: &str,
    order: &OrderRef,
    placed_nonce: u64,
) -> Result<Order, ApiError> {
    let (key, value) = order.field();
    let call = Call::delete("/fapi/v3/guardedCancelOrder").field("symbol", symbol).field(key, value);
    send_at(client, call, placed_nonce).await
}

/// Cancels up to ten orders placed together (one `batchOrders`) with the nonce they were placed
/// at; a row an order.
pub async fn guarded_cancel_batch<H: Http>(
    client: &Client<H, Agent>,
    symbol: &str,
    orders: &[OrderRef],
    placed_nonce: u64,
) -> Result<Vec<Result<Order, ApiError>>, TradeError> {
    let mut call = Call::delete("/fapi/v3/guardedBatchOrders").field("symbol", symbol);
    call.fields.extend(id_lists(orders)?);
    let path = call.path;
    let answer: Vec<serde_json::Value> = send_at(client, call, placed_nonce).await?;
    Ok(crate::trade::orders::rows(path, answer))
}

/// A noop at `nonce`: the nonce of a call still queued, which it may stop.
pub async fn noop_at<H: Http>(client: &Client<H, Agent>, nonce: u64) -> Result<(), ApiError> {
    send_at::<H, Done>(client, Call::post("/fapi/v3/noop"), nonce).await.map(|_| ())
}

#[cfg(test)]
#[path = "tests/tracked.rs"]
mod tests;
