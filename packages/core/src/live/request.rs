//! What a client sends on the socket: `SUBSCRIBE` and `UNSUBSCRIBE` with stream names,
//! `LIST_SUBSCRIPTIONS`, and `SET_PROPERTY` or `GET_PROPERTY` for `combined`, each with an id its
//! answer names (`{"result": .., "id": n}`).

use serde_json::{Value, json};

/// The requests the socket takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Subscribe(Vec<String>),
    Unsubscribe(Vec<String>),
    ListSubscriptions,
    /// Whether frames come wrapped as `{"stream", "data"}`.
    SetCombined(bool),
    GetCombined,
}

impl Asked {
    /// The method's name on the wire.
    #[must_use]
    pub const fn method(&self) -> &'static str {
        match self {
            Asked::Subscribe(_) => "SUBSCRIBE",
            Asked::Unsubscribe(_) => "UNSUBSCRIBE",
            Asked::ListSubscriptions => "LIST_SUBSCRIPTIONS",
            Asked::SetCombined(_) => "SET_PROPERTY",
            Asked::GetCombined => "GET_PROPERTY",
        }
    }

    /// The frame that asks it, under `id`.
    #[must_use]
    pub fn frame(&self, id: u64) -> String {
        let value = match self {
            Asked::Subscribe(names) | Asked::Unsubscribe(names) => {
                json!({ "method": self.method(), "params": names, "id": id })
            }
            Asked::ListSubscriptions => json!({ "method": self.method(), "id": id }),
            Asked::SetCombined(combined) => {
                json!({ "method": self.method(), "params": ["combined", combined], "id": id })
            }
            Asked::GetCombined => json!({ "method": self.method(), "params": ["combined"], "id": id }),
        };
        Value::to_string(&value)
    }
}

/// `SUBSCRIBE` to `names` under `id`.
#[must_use]
pub fn subscribe(id: u64, names: &[String]) -> String {
    Asked::Subscribe(names.to_vec()).frame(id)
}

/// `UNSUBSCRIBE` from `names` under `id`.
#[must_use]
pub fn unsubscribe(id: u64, names: &[String]) -> String {
    Asked::Unsubscribe(names.to_vec()).frame(id)
}

/// `LIST_SUBSCRIPTIONS` under `id`: the answer's `result` lists what the socket streams.
#[must_use]
pub fn list_subscriptions(id: u64) -> String {
    Asked::ListSubscriptions.frame(id)
}

#[cfg(test)]
#[path = "tests/request.rs"]
mod tests;
