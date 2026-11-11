//! Whether a live answer still has the shape its recording has: every key of every object the
//! recording holds, at the same place, with a value of the same kind. Arrays are compared by
//! their first element; a key the venue added is fine.

use serde_json::Value;

pub fn holds(recorded: &Value, live: &Value, at: &str) -> Result<(), String> {
    match (recorded, live) {
        (Value::Object(want), Value::Object(got)) => {
            for (key, value) in want {
                let here = format!("{at}.{key}");
                let found = got.get(key).ok_or_else(|| format!("{here} is gone"))?;
                holds(value, found, &here)?;
            }
            Ok(())
        }
        (Value::Array(want), Value::Array(got)) => match (want.first(), got.first()) {
            (Some(want), Some(got)) => holds(want, got, &format!("{at}[0]")),
            _ => Ok(()),
        },
        (Value::Null, _) | (_, Value::Null) => Ok(()),
        (want, got) if kind(want) == kind(got) => Ok(()),
        (want, got) => Err(format!("{at} was {} and is now {}", kind(want), kind(got))),
    }
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a flag",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}
