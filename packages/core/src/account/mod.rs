//! What an account holds and who may act for it: every account read, the account's settings, its
//! sub-accounts, its agents and builders, and the notices sent to it. Each is a plain row and a
//! free function over any [`crate::client::Transport`]; nothing is cached here.
//!
//! - [`holdings`]: balances (`balance`), the account (`account`, `accountWithJoinMargin`),
//!   positions (`positionRisk`), commission rates (`commissionRate`), leverage brackets
//!   (`leverageBrackets`), ADL quantiles (`adlQuantile`) and a symbol's open interest cap
//!   (`remainingOpenableNotionalValue`).
//! - [`history`]: an order (`order`, `openOrder`), the open orders (`openOrders`), every order
//!   (`allOrders`), fills (`userTrades`), income (`income`, `incomePage`), liquidations and ADL
//!   (`forceOrders`) and isolated margin changes (`positionMargin/history`).
//! - [`settings`]: position mode (`positionSide/dual`), self-trade prevention (`stpMode`),
//!   multi-assets mode (`multiAssetsMargin`), market maker protection (`mmp`, `mmpReset`) and a
//!   referral lookup (`referral/inviteeInfo`).
//! - [`subaccounts`]: binding, creating, listing and updating sub-accounts
//!   (`sub-accounts/bind`, `createSubAccount`, `getSubAccountList`, `updateSubAccount`).
//! - [`agents`](mod@agents): what the main wallet signs itself: approving, updating and removing an agent
//!   (`approveAgent`, `updateAgent`, `agent`) or a builder (`approveBuilder`, `updateBuilder`,
//!   `builder`), registering an agent in one call (`registerAndApproveAgent`), moving another
//!   account's assets here (`asset/migrateUser`), and the lists of agents and builders.
//! - [`notices`]: announcements sent to the account (`announcement/direct`,
//!   `announcement/directById`).
//!
//! Every call but the open interest cap is the account's own. Most are signed by the agent
//! ([`crate::client::Call::signed`]); the ones the main wallet must sign take a
//! [`crate::utils::sign::Wallet`], and its signature must recover to the wallet before anything
//! is sent.

pub mod agents;
pub mod history;
pub mod holdings;
pub mod notices;
pub mod settings;
pub mod subaccounts;

pub use agents::{
    AGENT_TTL_MS, ApproveAgent, DEMO_SIGN_CHAIN_ID, MainAction, MigrateDetail, MigrateHistory, MigrateUser, Migrated,
    RegisterAgent, agent_expiry, agents, builders, migrate_history,
};
#[cfg(feature = "sign")]
pub use agents::{migrate_user, new_agent, register_and_approve_agent, send_main_action};
pub use history::{
    AutoCloseType, Fill, Income, IncomePage, IncomeQuery, IncomeType, MarginChange, OrderLookup, TimeRange, all_orders,
    force_orders, income, income_page, margin_history, open_order, open_orders, query_order, user_trades,
};
pub use holdings::{
    AccountAsset, AccountInfo, AccountPosition, AdlQuantile, AdlQuantiles, Balance, Bracket, CommissionRate,
    LeverageBrackets, Position, account, account_with_join_margin, adl_quantiles, balances, commission_rate,
    leverage_brackets, positions, remaining_openable_notional,
};
pub use notices::{Announcement, AnnouncementContent, Announcements, announcement, announcements};
pub use settings::{
    InviteeInfo, Mmp, delete_mmp, invitee_info, mmp, multi_assets_mode, position_mode, reset_mmp,
    set_multi_assets_mode, set_position_mode, set_stp_mode, stp_mode, update_mmp,
};
pub use subaccounts::{SubAccount, SubAccountStatus, sub_accounts};
#[cfg(feature = "sign")]
pub use subaccounts::{bind_sub_account, create_sub_account, update_sub_account};

use serde::{Deserialize, Deserializer};

use crate::client::ApiError;
use crate::utils::dec::Dec;
use crate::utils::sign::WalletError;

/// Why a call the main wallet signs came to nothing: the wallet did not sign (or signed as
/// someone else, and nothing was sent), or the venue refused it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalletCallError {
    #[error(transparent)]
    Wallet(#[from] WalletError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

/// A decimal the venue writes as a JSON number, a fraction among them (`"maintMarginRatio":
/// 0.0065`), or as a string. A fraction is read through the shortest text that gives it back,
/// which is the figure the venue wrote; an exponent (`1e-7`) is applied exactly.
pub(crate) fn number_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Dec, D::Error> {
    Ok(maybe_number_dec(deserializer)?.unwrap_or(Dec::ZERO))
}

/// [`number_dec`], with `null`, absent or `""` read as `None`.
pub(crate) fn maybe_number_dec<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Dec>, D::Error> {
    let text = match Option::<serde_json::Value>::deserialize(deserializer)? {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(serde_json::Value::Number(number)) => number.to_string(),
        Some(serde_json::Value::String(text)) if text.trim().is_empty() => return Ok(None),
        Some(serde_json::Value::String(text)) => text,
        Some(other) => return Err(serde::de::Error::custom(format!("not a decimal: {other}"))),
    };
    with_exponent(&text).map(Some).ok_or_else(|| serde::de::Error::custom(format!("not a decimal: {text}")))
}

/// `1.5`, `-2`, `1e-7` or `2.5E3` as an exact decimal.
fn with_exponent(text: &str) -> Option<Dec> {
    let text = text.trim();
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.trim_start_matches('+').parse::<i32>().ok()?),
        None => (text, 0),
    };
    let mantissa = Dec::parse(mantissa).ok()?;
    let power = exponent.unsigned_abs();
    match exponent >= 0 {
        true => mantissa.checked_mul(Dec::new(10i128.checked_pow(power)?, 0)?),
        false => mantissa.checked_mul(Dec::new(1, power)?),
    }
}

/// A whole number the venue writes as a number or a string (`"leverage": "10"`).
pub(crate) fn whole<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let value = crate::utils::lenient::int(deserializer)?;
    u32::try_from(value).map_err(|_| serde::de::Error::custom(format!("not a count: {value}")))
}

/// A list of addresses as one comma-separated field, EIP-55 spelled.
pub(crate) fn address_list(addresses: &[crate::utils::Address]) -> Option<String> {
    match addresses.is_empty() {
        true => None,
        false => Some(addresses.iter().map(|address| address.to_checksum()).collect::<Vec<_>>().join(",")),
    }
}

#[cfg(test)]
#[path = "tests/numbers.rs"]
mod tests;
