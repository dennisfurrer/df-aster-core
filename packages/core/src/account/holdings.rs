//! What an account holds: its balances (`balance`), the whole account with its assets and every
//! symbol's position (`account`, `accountWithJoinMargin`), its positions with prices
//! (`positionRisk`), its commission rate on a symbol (`commissionRate`), the leverage brackets
//! (`leverageBrackets`), the ADL queue (`adlQuantile`), and a symbol's open interest cap at a
//! leverage (`remainingOpenableNotionalValue`, public).
//!
//! The totals on the account are for USDT only, as the venue writes them; each asset's figures
//! are on its row.

use serde::Deserialize;

use crate::account::{maybe_number_dec, number_dec, whole};
use crate::client::http::{ApiError, Call, Transport, read};
use crate::utils::lenient;
use crate::utils::{Dec, MarginType, PositionSide};

/// One asset's balance (`balance`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    /// The account's unique code.
    #[serde(default, deserialize_with = "lenient::text")]
    pub account_alias: String,
    pub asset: String,
    /// The wallet balance.
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub cross_wallet_balance: Dec,
    /// Unrealized profit of the cross positions.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub cross_un_pnl: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub available_balance: Dec,
    /// The most that may be transferred out.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_withdraw_amount: Dec,
    /// Whether it counts as margin in multi-assets mode.
    #[serde(default, deserialize_with = "lenient::flag")]
    pub margin_available: bool,
    #[serde(default)]
    pub update_time: u64,
}

/// The account (`account`, `accountWithJoinMargin`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    /// The commission tier.
    #[serde(default)]
    pub fee_tier: u32,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_trade: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_deposit: bool,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub can_withdraw: bool,
    #[serde(default)]
    pub update_time: u64,
    /// Initial margin of positions and open orders at the mark price, USDT only.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_maint_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_wallet_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_unrealized_profit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_margin_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_position_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_open_order_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_cross_wallet_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub total_cross_un_pnl: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub available_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_withdraw_amount: Dec,
    #[serde(default, deserialize_with = "lenient::list")]
    pub assets: Vec<AccountAsset>,
    /// Every symbol's position: `BOTH` in one-way mode, `LONG` and `SHORT` in hedge mode.
    #[serde(default, deserialize_with = "lenient::list")]
    pub positions: Vec<AccountPosition>,
}

impl AccountInfo {
    /// The asset's row, by name.
    #[must_use]
    pub fn asset(&self, asset: &str) -> Option<&AccountAsset> {
        self.assets.iter().find(|row| row.asset == asset)
    }

    /// The positions that hold something.
    pub fn open_positions(&self) -> impl Iterator<Item = &AccountPosition> {
        self.positions.iter().filter(|position| !position.position_amt.is_zero())
    }
}

/// One asset on the account.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAsset {
    pub asset: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub wallet_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub unrealized_profit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub margin_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub maint_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub position_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub open_order_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub cross_wallet_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub cross_un_pnl: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub available_balance: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_withdraw_amount: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub margin_available: bool,
    #[serde(default)]
    pub update_time: u64,
}

/// One symbol's position as the account read writes it: margins, no prices.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPosition {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub maint_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub unrealized_profit: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub position_initial_margin: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub open_order_initial_margin: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub leverage: u32,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub isolated: bool,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub entry_price: Dec,
    /// The most notional the current leverage allows.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_notional: Dec,
    pub position_side: PositionSide,
    /// Signed: negative is short.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub position_amt: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub notional: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub isolated_wallet: Dec,
    #[serde(default)]
    pub update_time: u64,
}

/// One position with its prices (`positionRisk`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub symbol: String,
    /// Signed: negative is short.
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub position_amt: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub entry_price: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub mark_price: Dec,
    #[serde(default, rename = "unRealizedProfit", deserialize_with = "lenient::zero_dec")]
    pub unrealized_profit: Dec,
    /// Zero when there is none.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub liquidation_price: Dec,
    #[serde(default, deserialize_with = "whole")]
    pub leverage: u32,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub max_notional_value: Dec,
    /// `isolated` or `cross`.
    #[serde(deserialize_with = "margin_type")]
    pub margin_type: MarginType,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub isolated_margin: Dec,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub is_auto_add_margin: bool,
    #[serde(default = "both")]
    pub position_side: PositionSide,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub notional: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub isolated_wallet: Dec,
    #[serde(default)]
    pub update_time: u64,
}

impl Position {
    /// Whether it holds anything.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        !self.position_amt.is_zero()
    }
}

fn both() -> PositionSide {
    PositionSide::Both
}

fn margin_type<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<MarginType, D::Error> {
    let text = lenient::text(deserializer)?;
    Ok(MarginType::parse_lower(&text))
}

/// The account's commission on a symbol, as fractions (`0.0002` is 0.02%).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommissionRate {
    pub symbol: String,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub maker_commission_rate: Dec,
    #[serde(deserialize_with = "lenient::zero_dec")]
    pub taker_commission_rate: Dec,
}

/// A symbol's notional brackets: the higher the notional, the lower the leverage.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeverageBrackets {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::list")]
    pub brackets: Vec<Bracket>,
}

impl LeverageBrackets {
    /// The bracket a position of `notional` falls in.
    #[must_use]
    pub fn for_notional(&self, notional: Dec) -> Option<&Bracket> {
        let notional = notional.abs();
        self.brackets.iter().find(|bracket| notional >= bracket.notional_floor && notional < bracket.notional_cap)
    }

    /// The highest leverage any bracket allows.
    #[must_use]
    pub fn max_leverage(&self) -> u32 {
        self.brackets.iter().map(|bracket| bracket.initial_leverage).max().unwrap_or(0)
    }
}

/// One bracket. The venue writes its figures as JSON numbers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bracket {
    pub bracket: u32,
    /// The highest initial leverage in the bracket.
    #[serde(deserialize_with = "whole")]
    pub initial_leverage: u32,
    #[serde(deserialize_with = "number_dec")]
    pub notional_cap: Dec,
    #[serde(deserialize_with = "number_dec")]
    pub notional_floor: Dec,
    #[serde(deserialize_with = "number_dec")]
    pub maint_margin_ratio: Dec,
    /// The amount the maintenance margin formula subtracts at this bracket.
    #[serde(default, deserialize_with = "number_dec")]
    pub cum: Dec,
}

/// A symbol's ADL quantiles, 0 (last to be deleveraged) to 4 (first).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdlQuantiles {
    pub symbol: String,
    pub adl_quantile: AdlQuantile,
}

/// The quantile of each position side the venue writes. `hedge` is set (its value is to be
/// ignored) when the symbol's positions are cross margined in hedge mode; then `long` and `short`
/// carry the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub struct AdlQuantile {
    #[serde(default)]
    pub long: Option<u8>,
    #[serde(default)]
    pub short: Option<u8>,
    #[serde(default)]
    pub both: Option<u8>,
    #[serde(default)]
    pub hedge: Option<u8>,
}

/// `GET /fapi/v3/balance`.
#[must_use]
pub fn balances_call() -> Call {
    Call::get("/fapi/v3/balance").signed()
}

/// Every asset's balance.
pub async fn balances(transport: &impl Transport) -> Result<Vec<Balance>, ApiError> {
    read(transport, balances_call()).await
}

/// `GET /fapi/v3/account`: the account without join margin.
#[must_use]
pub fn account_call() -> Call {
    Call::get("/fapi/v3/account").signed()
}

pub async fn account(transport: &impl Transport) -> Result<AccountInfo, ApiError> {
    read(transport, account_call()).await
}

/// `GET /fapi/v3/accountWithJoinMargin`: the same shape, in the join margin view.
#[must_use]
pub fn account_with_join_margin_call() -> Call {
    Call::get("/fapi/v3/accountWithJoinMargin").signed()
}

pub async fn account_with_join_margin(transport: &impl Transport) -> Result<AccountInfo, ApiError> {
    read(transport, account_with_join_margin_call()).await
}

/// `GET /fapi/v3/positionRisk`, for one symbol or every one.
#[must_use]
pub fn positions_call(symbol: Option<&str>) -> Call {
    Call::get("/fapi/v3/positionRisk").maybe("symbol", symbol).signed()
}

/// Positions with prices. Every symbol's comes back, those holding nothing among them.
pub async fn positions(transport: &impl Transport, symbol: Option<&str>) -> Result<Vec<Position>, ApiError> {
    read(transport, positions_call(symbol)).await
}

/// `GET /fapi/v3/commissionRate`.
#[must_use]
pub fn commission_rate_call(symbol: &str) -> Call {
    Call::get("/fapi/v3/commissionRate").field("symbol", symbol).signed()
}

pub async fn commission_rate(transport: &impl Transport, symbol: &str) -> Result<CommissionRate, ApiError> {
    read(transport, commission_rate_call(symbol)).await
}

/// `GET /fapi/v3/leverageBrackets`, the maintained path (the singular `leverageBracket` is
/// deprecated).
#[must_use]
pub fn leverage_brackets_call(symbol: Option<&str>) -> Call {
    Call::get("/fapi/v3/leverageBrackets").maybe("symbol", symbol).signed()
}

/// The brackets of one symbol or every one. With a symbol the venue answers one object rather
/// than a list; either way this gives a list.
pub async fn leverage_brackets(
    transport: &impl Transport,
    symbol: Option<&str>,
) -> Result<Vec<LeverageBrackets>, ApiError> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<LeverageBrackets>),
        One(LeverageBrackets),
    }
    Ok(match read(transport, leverage_brackets_call(symbol)).await? {
        OneOrMany::Many(all) => all,
        OneOrMany::One(one) => vec![one],
    })
}

/// `GET /fapi/v3/adlQuantile`.
#[must_use]
pub fn adl_quantiles_call(symbol: Option<&str>) -> Call {
    Call::get("/fapi/v3/adlQuantile").maybe("symbol", symbol).signed()
}

/// The ADL queue of each symbol held. The venue updates it every 30 seconds.
pub async fn adl_quantiles(transport: &impl Transport, symbol: Option<&str>) -> Result<Vec<AdlQuantiles>, ApiError> {
    read(transport, adl_quantiles_call(symbol)).await
}

/// `GET /fapi/v3/remainingOpenableNotionalValue`: public, weight 50.
#[must_use]
pub fn remaining_openable_notional_call(symbol: &str, leverage: u32) -> Call {
    Call::get("/fapi/v3/remainingOpenableNotionalValue").field("symbol", symbol).field("leverage", leverage)
}

/// The notional, in USDT, still open on `symbol` at `leverage`'s tier: a cap on the symbol, not
/// on the account. `None` when the symbol has no cap (the venue writes `-1`).
pub async fn remaining_openable_notional(
    transport: &impl Transport,
    symbol: &str,
    leverage: u32,
) -> Result<Option<Dec>, ApiError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Remaining {
        #[serde(deserialize_with = "maybe_number_dec")]
        remaining_openable_notional_value: Option<Dec>,
    }
    let remaining: Remaining = read(transport, remaining_openable_notional_call(symbol, leverage)).await?;
    Ok(remaining.remaining_openable_notional_value.filter(|value| !value.is_negative()))
}

#[cfg(test)]
#[path = "tests/holdings.rs"]
mod tests;
