//! A fake Aster on the [`Http`] wire. It rebuilds what was signed from the request alone:
//!
//! - an agent's call: the query or body before `&signature=` is `msg`, signed as
//!   `Message { msg }` on chain 1666; it must recover to an agent approved for `user`, and its
//!   nonce must be new for that agent;
//! - `approveAgent`: every field before `signature`, keys capitalised, typed as the demo infers
//!   them (`true`/`false` as bool, digits as uint256, anything else as string), signed on
//!   chain 56; it must recover to `user`.
//!
//! It keeps the approved agents and the orders placed, and answers as Aster does.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::future::{Future, ready};

use df_aster_core::Address;
use df_aster_core::client::{Http, Request, Response, TransportError};
use df_aster_core::utils::sign::{Domain, Field, Signature, Struct, TypedData, message, recover};
use serde_json::{Value, json};

#[derive(Default)]
pub struct Venue {
    agents: RefCell<BTreeMap<Address, BTreeSet<Address>>>,
    nonces: RefCell<BTreeSet<(Address, u64)>>,
    orders: RefCell<Vec<Value>>,
    next_id: RefCell<u64>,
    pub log: RefCell<Vec<String>>,
}

fn answer(status: u16, body: &Value) -> Response {
    Response { status, headers: Vec::new(), body: body.to_string().into_bytes() }
}

fn refuse(code: i64, msg: &str) -> Response {
    answer(400, &json!({ "code": code, "msg": msg }))
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            out.push(u8::from_str_radix(&text[at + 1..at + 3], 16).unwrap());
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

fn fields(encoded: &str) -> Vec<(String, String)> {
    encoded
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key.to_owned(), decode(value))
        })
        .collect()
}

fn value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
}

impl Venue {
    fn handle(&self, request: &Request) -> Response {
        let (path, query) = request.url.split_once('?').unwrap_or((&request.url, ""));
        let path = path.trim_start_matches("https://fapi.asterdex.com");
        let encoded =
            if query.is_empty() { String::from_utf8_lossy(&request.body).into_owned() } else { query.to_owned() };
        self.log.borrow_mut().push(format!("{} {path}", request.method.as_str()));
        match path {
            "/fapi/v3/exchangeInfo" => answer(
                200,
                &serde_json::from_str::<Value>(include_str!("../../fixtures/aster/markets_exchangeInfo.json")).unwrap(),
            ),
            "/fapi/v3/approveAgent" => self.approve(&encoded),
            _ => match self.agent_call(&encoded) {
                Err(refused) => refused,
                Ok((user, fields)) => self.account(request.method.as_str(), path, user, &fields),
            },
        }
    }

    fn approve(&self, encoded: &str) -> Response {
        let fields = fields(encoded);
        let signed: Vec<&(String, String)> = fields.iter().take_while(|(key, _)| key != "signature").collect();
        let message = signed.iter().fold(Struct::new("ApproveAgent"), |message, (key, value)| {
            let name = format!("{}{}", key[..1].to_uppercase(), &key[1..]);
            let typed = match value.as_str() {
                "true" => Field::Bool(true),
                "false" => Field::Bool(false),
                digits if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
                    Field::Uint256(digits.parse().unwrap())
                }
                text => Field::String(text.to_owned()),
            };
            message.field(&name, typed)
        });
        let data = TypedData::new(Domain::new("AsterSignTransaction", "1", 56), message);
        let signature: Signature = value(&fields, "signature").unwrap().parse().unwrap();
        let user: Address = value(&fields, "user").unwrap().parse().unwrap();
        if recover(&data.hash(), &signature).ok() != Some(user) {
            return refuse(-1022, "Signature for this request is not valid.");
        }
        let agent: Address = value(&fields, "agentAddress").unwrap().parse().unwrap();
        self.agents.borrow_mut().entry(user).or_default().insert(agent);
        answer(200, &json!({ "code": 200, "msg": "success" }))
    }

    fn agent_call(&self, encoded: &str) -> Result<(Address, Vec<(String, String)>), Response> {
        let (msg, signature) = encoded.rsplit_once("&signature=").ok_or_else(|| refuse(-1102, "signature"))?;
        let fields = fields(msg);
        let user: Address = value(&fields, "user").ok_or_else(|| refuse(-1102, "user"))?.parse().unwrap();
        let signer: Address = value(&fields, "signer").ok_or_else(|| refuse(-1102, "signer"))?.parse().unwrap();
        let nonce: u64 = value(&fields, "nonce").ok_or_else(|| refuse(-1102, "nonce"))?.parse().unwrap();
        let signature: Signature = signature.parse().map_err(|_| refuse(-1022, "bad signature"))?;
        if recover(&message(msg).hash(), &signature).ok() != Some(signer) {
            return Err(refuse(-1022, "Signature for this request is not valid."));
        }
        if !self.agents.borrow().get(&user).is_some_and(|agents| agents.contains(&signer)) {
            return Err(refuse(-2015, "Invalid API-key, IP, or permissions for action."));
        }
        if !self.nonces.borrow_mut().insert((signer, nonce)) {
            return Err(refuse(-4225, "Nonce Expired"));
        }
        Ok((user, fields))
    }

    fn account(&self, method: &str, path: &str, _user: Address, fields: &[(String, String)]) -> Response {
        match (method, path) {
            ("GET", "/fapi/v3/balance") => answer(
                200,
                &json!([{
                    "accountAlias": "SgsR", "asset": "USDT", "balance": "100", "crossWalletBalance": "100",
                    "crossUnPnl": "0", "availableBalance": "100", "maxWithdrawAmount": "100",
                    "marginAvailable": true, "updateTime": 1790000000000u64
                }]),
            ),
            ("POST", "/fapi/v3/order") => {
                let id = {
                    let mut next = self.next_id.borrow_mut();
                    *next += 1;
                    *next
                };
                let field = |key: &str| value(fields, key).unwrap_or_default().to_owned();
                let order = json!({
                    "orderId": id, "symbol": field("symbol"), "status": "NEW", "clientOrderId": field("newClientOrderId"),
                    "price": field("price"), "avgPrice": "0", "origQty": field("quantity"), "executedQty": "0",
                    "cumQuote": "0", "timeInForce": field("timeInForce"), "type": field("type"),
                    "reduceOnly": field("reduceOnly") == "true", "side": field("side"), "positionSide": "BOTH",
                    "stopPrice": "0", "workingType": "CONTRACT_PRICE", "origType": field("type"),
                    "time": 1790000000000u64, "updateTime": 1790000000000u64
                });
                self.orders.borrow_mut().push(order.clone());
                answer(200, &order)
            }
            ("GET", "/fapi/v3/openOrders") => answer(200, &Value::Array(self.orders.borrow().clone())),
            ("DELETE", "/fapi/v3/order") => {
                let id: u64 = value(fields, "orderId").unwrap().parse().unwrap();
                let mut orders = self.orders.borrow_mut();
                match orders.iter().position(|order| order["orderId"] == id) {
                    Some(at) => {
                        let mut order = orders.remove(at);
                        order["status"] = json!("CANCELED");
                        answer(200, &order)
                    }
                    None => refuse(-2011, "Unknown order sent."),
                }
            }
            _ => refuse(-1000, "not in the fake venue"),
        }
    }
}

impl Http for &Venue {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        ready(Ok(self.handle(&request)))
    }
}

/// Runs a future that never waits.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

pub fn clock() -> u64 {
    1_790_000_000_000
}
