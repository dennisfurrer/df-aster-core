//! Reading the venue's answers leniently where it is loose: a list it writes as `null`, a decimal
//! it leaves as `""`, an id it writes as a number in one place and a string in another, a flag it
//! writes as `"true"`.

use serde::{Deserialize, Deserializer};

use crate::utils::dec::Dec;

/// A list, `null` or absent read as empty.
pub(crate) fn list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// A decimal string, `""` or `null` read as `None`.
pub(crate) fn blank_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Dec>, D::Error> {
    match Option::<Text>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Text::Number(units)) => Ok(Some(Dec::from_int(units))),
        Some(Text::Words(text)) if text.trim().is_empty() => Ok(None),
        Some(Text::Words(text)) => Dec::parse(&text).map(Some).map_err(serde::de::Error::custom),
        Some(Text::Flag(_)) => Err(serde::de::Error::custom("a flag where a decimal was expected")),
    }
}

/// A decimal string, `""` or `null` read as zero.
pub(crate) fn zero_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    Ok(blank_dec(deserializer)?.unwrap_or(Dec::ZERO))
}

/// A string that may be absent or `null`, read as empty.
pub(crate) fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// An id the venue writes as a number in one answer and as a string in another, as its text.
pub(crate) fn id_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match Option::<Text>::deserialize(deserializer)? {
        None => Ok(String::new()),
        Some(Text::Number(number)) => Ok(number.to_string()),
        Some(Text::Words(text)) => Ok(text),
        Some(Text::Flag(flag)) => Ok(flag.to_string()),
    }
}

/// A whole number written as a number or a string; absent, `null` or `""` read as 0.
pub(crate) fn int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    match Option::<Text>::deserialize(deserializer)? {
        None | Some(Text::Flag(_)) => Ok(0),
        Some(Text::Number(number)) => Ok(number),
        Some(Text::Words(text)) if text.trim().is_empty() => Ok(0),
        Some(Text::Words(text)) => text.trim().parse().map_err(serde::de::Error::custom),
    }
}

/// A flag written as `true` or as `"true"` (any case); absent or `null` read as false.
pub(crate) fn flag<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    match Option::<Text>::deserialize(deserializer)? {
        None | Some(Text::Number(0)) => Ok(false),
        Some(Text::Flag(flag)) => Ok(flag),
        Some(Text::Number(_)) => Ok(true),
        Some(Text::Words(text)) => Ok(text.trim().eq_ignore_ascii_case("true")),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    Flag(bool),
    Number(i64),
    Words(String),
}

/// An answer whose content is not needed: `{"code":200,"msg":"success"}` and the like, which
/// [`crate::client::read`] has checked for a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Done {}
