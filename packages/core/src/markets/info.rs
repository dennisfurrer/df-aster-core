//! The listing (`exchangeInfo`): every symbol with its contract, its precisions and its filters,
//! the assets that may be margin, and the venue's rate limits. [`Symbol::grid`] reads a symbol's
//! filters into the [`Grid`] orders are fitted to; [`Markets`] looks symbols up by name.
//!
//! The docs say `pricePrecision` and `quantityPrecision` are not the tick and step; the filters
//! are. `maintMarginPercent` and `requiredMarginPercent` are marked "ignore" in the docs and are
//! kept as they came.

use std::collections::HashMap;

use serde::Deserialize;

use crate::client::{ApiError, Call, Transport, read};
use crate::utils::lenient;
use crate::utils::{ContractStatus, Dec, Grid, OrderType, TimeInForce};

/// What `exchangeInfo` answers.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInfo {
    #[serde(default, deserialize_with = "lenient::text")]
    pub timezone: String,
    /// When the listing was built, not the venue's clock: read `time` for that.
    #[serde(default)]
    pub server_time: u64,
    /// `U_MARGINED`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub futures_type: String,
    #[serde(default, deserialize_with = "lenient::list")]
    pub rate_limits: Vec<RateLimit>,
    /// Exchange-wide filters: none today, kept as they came.
    #[serde(default, deserialize_with = "lenient::list")]
    pub exchange_filters: Vec<serde_json::Value>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub assets: Vec<AssetInfo>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub symbols: Vec<Symbol>,
}

/// One of the venue's limits: `REQUEST_WEIGHT` per IP, or `ORDERS` per account, over
/// `interval_num` `interval`s.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub rate_limit_type: String,
    /// `SECOND`, `MINUTE` or `DAY`.
    pub interval: String,
    pub interval_num: u32,
    pub limit: u32,
}

impl RateLimit {
    /// The window in milliseconds; `None` for an interval this build does not know.
    #[must_use]
    pub fn window_ms(&self) -> Option<u64> {
        let unit: u64 = match self.interval.as_str() {
            "SECOND" => 1_000,
            "MINUTE" => 60_000,
            "HOUR" => 3_600_000,
            "DAY" => 86_400_000,
            _ => return None,
        };
        unit.checked_mul(u64::from(self.interval_num))
    }
}

/// An asset and whether it may be margin in multi-assets mode.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    pub asset: String,
    #[serde(default, deserialize_with = "lenient::flag")]
    pub margin_available: bool,
    /// The balance below which multi-assets mode exchanges the asset automatically; `None` when
    /// the venue gives none.
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub auto_asset_exchange: Option<Dec>,
    /// `crypto` or `stable_coin`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub asset_type: String,
}

/// One symbol's filter, by its `filterType`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "filterType")]
pub enum Filter {
    /// The price bounds and step; a zero turns that rule off.
    #[serde(rename = "PRICE_FILTER", rename_all = "camelCase")]
    Price {
        #[serde(deserialize_with = "lenient::zero_dec")]
        min_price: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        max_price: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        tick_size: Dec,
    },
    /// A limit order's quantity bounds and step.
    #[serde(rename = "LOT_SIZE", rename_all = "camelCase")]
    LotSize {
        #[serde(deserialize_with = "lenient::zero_dec")]
        min_qty: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        max_qty: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        step_size: Dec,
    },
    /// A market order's quantity bounds and step.
    #[serde(rename = "MARKET_LOT_SIZE", rename_all = "camelCase")]
    MarketLotSize {
        #[serde(deserialize_with = "lenient::zero_dec")]
        min_qty: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        max_qty: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        step_size: Dec,
    },
    /// The most open orders an account may have on the symbol, conditional ones included.
    #[serde(rename = "MAX_NUM_ORDERS")]
    MaxNumOrders {
        #[serde(deserialize_with = "lenient::int")]
        limit: i64,
    },
    /// The most open conditional orders (stops, take-profits, trailing stops).
    #[serde(rename = "MAX_NUM_ALGO_ORDERS")]
    MaxNumAlgoOrders {
        #[serde(deserialize_with = "lenient::int")]
        limit: i64,
    },
    /// The least price times quantity; a market order is valued at the mark price.
    #[serde(rename = "MIN_NOTIONAL")]
    MinNotional {
        #[serde(deserialize_with = "lenient::zero_dec")]
        notional: Dec,
    },
    /// How far from the mark price a limit price may be: a buy at most `multiplier_up` times
    /// it, a sell at least `multiplier_down` times it.
    #[serde(rename = "PERCENT_PRICE", rename_all = "camelCase")]
    PercentPrice {
        #[serde(deserialize_with = "lenient::zero_dec")]
        multiplier_up: Dec,
        #[serde(deserialize_with = "lenient::zero_dec")]
        multiplier_down: Dec,
        #[serde(default, deserialize_with = "lenient::int")]
        multiplier_decimal: i64,
        /// Undocumented: a second, tighter pair some symbols carry, `0` for none.
        #[serde(default, deserialize_with = "lenient::blank_dec")]
        lt_multiplier_up: Option<Dec>,
        #[serde(default, deserialize_with = "lenient::blank_dec")]
        lt_multiplier_down: Option<Dec>,
    },
    /// A filter this build does not know.
    #[serde(other)]
    Other,
}

/// One symbol: a perpetual contract.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub pair: String,
    /// `PERPETUAL`, or `""` for a symbol not yet trading.
    #[serde(default, deserialize_with = "lenient::text")]
    pub contract_type: String,
    pub status: ContractStatus,
    #[serde(default, deserialize_with = "lenient::text")]
    pub base_asset: String,
    #[serde(default, deserialize_with = "lenient::text")]
    pub quote_asset: String,
    /// The asset its margin and profit are in.
    #[serde(default, deserialize_with = "lenient::text")]
    pub margin_asset: String,
    /// Unix ms. A perpetual's delivery date is far in the future.
    #[serde(default)]
    pub delivery_date: u64,
    #[serde(default)]
    pub onboard_date: u64,
    #[serde(default)]
    pub create_time: Option<u64>,
    /// Places a price is shown with; not the tick ([`Symbol::grid`]).
    #[serde(default)]
    pub price_precision: u32,
    /// Places a quantity is shown with; not the step.
    #[serde(default)]
    pub quantity_precision: u32,
    #[serde(default)]
    pub base_asset_precision: u32,
    #[serde(default)]
    pub quote_precision: u32,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub maint_margin_percent: Dec,
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub required_margin_percent: Dec,
    /// `COIN`.
    #[serde(default, deserialize_with = "lenient::text")]
    pub underlying_type: String,
    /// Sectors: `AI`, `Commodities`, `Top` and the like.
    #[serde(default, deserialize_with = "lenient::list")]
    pub underlying_sub_type: Vec<String>,
    #[serde(default, deserialize_with = "lenient::int")]
    pub settle_plan: i64,
    /// How far mark and last price may differ when a stop with price protection triggers: a
    /// fraction.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub trigger_protect: Dec,
    /// The liquidation fee, a fraction of the notional.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub liquidation_fee: Dec,
    /// How far from the mark price a market order may fill, a fraction.
    #[serde(default, deserialize_with = "lenient::zero_dec")]
    pub market_take_bound: Dec,
    #[serde(default, deserialize_with = "lenient::list")]
    pub filters: Vec<Filter>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub order_types: Vec<OrderType>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub time_in_force: Vec<TimeInForce>,
    /// The least notional of a TWAP order.
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub twap_min_notional: Option<Dec>,
    /// Undocumented; kept as it came, `None` when blank.
    #[serde(default, deserialize_with = "lenient::blank_dec")]
    pub imn: Option<Dec>,
    /// Undocumented: 0 for a crypto perp, 1 for others (stocks, commodities, forex).
    #[serde(default, deserialize_with = "lenient::int")]
    pub symbol_type: i64,
    /// Undocumented; kept as it came.
    #[serde(default, deserialize_with = "lenient::int")]
    pub trading_mode: i64,
    /// A display name some symbols carry (`Gold`, `Crude Oil`); usually empty.
    #[serde(default, deserialize_with = "lenient::text")]
    pub name: String,
    /// The market a non-crypto symbol follows (`nasdaq`, `hkstock`, `forex`); usually empty.
    #[serde(default, deserialize_with = "lenient::text")]
    pub channel: String,
    #[serde(default, deserialize_with = "lenient::int")]
    pub sequence_no: i64,
    /// Search names, in several languages.
    #[serde(default, deserialize_with = "lenient::list")]
    pub tags: Vec<String>,
}

impl Symbol {
    /// Whether it trades now.
    #[must_use]
    pub fn is_trading(&self) -> bool {
        self.status == ContractStatus::Trading
    }

    /// Its filters as the grid an order is fitted to. A filter it lacks leaves zeros, which
    /// turn that rule off.
    #[must_use]
    pub fn grid(&self) -> Grid {
        let mut grid = Grid::default();
        for filter in &self.filters {
            match filter {
                Filter::Price { min_price, max_price, tick_size } => {
                    (grid.min_price, grid.max_price, grid.tick) = (*min_price, *max_price, *tick_size);
                }
                Filter::LotSize { min_qty, max_qty, step_size } => {
                    (grid.min_qty, grid.max_qty, grid.step) = (*min_qty, *max_qty, *step_size);
                }
                Filter::MarketLotSize { min_qty, max_qty, step_size } => {
                    (grid.market_min_qty, grid.market_max_qty, grid.market_step) = (*min_qty, *max_qty, *step_size);
                }
                Filter::MinNotional { notional } => grid.min_notional = *notional,
                _ => {}
            }
        }
        grid
    }

    /// The most open orders an account may have on it, when the filter is there.
    #[must_use]
    pub fn max_orders(&self) -> Option<i64> {
        self.filters.iter().find_map(|filter| match filter {
            Filter::MaxNumOrders { limit } => Some(*limit),
            _ => None,
        })
    }

    /// The most open conditional orders, when the filter is there.
    #[must_use]
    pub fn max_algo_orders(&self) -> Option<i64> {
        self.filters.iter().find_map(|filter| match filter {
            Filter::MaxNumAlgoOrders { limit } => Some(*limit),
            _ => None,
        })
    }

    /// How far above and below the mark price a limit price may be: `(up, down)` multipliers.
    #[must_use]
    pub fn percent_price(&self) -> Option<(Dec, Dec)> {
        self.filters.iter().find_map(|filter| match filter {
            Filter::PercentPrice { multiplier_up, multiplier_down, .. } => Some((*multiplier_up, *multiplier_down)),
            _ => None,
        })
    }

    /// Whether it takes `time_in_force`.
    #[must_use]
    pub fn takes(&self, time_in_force: &TimeInForce) -> bool {
        self.time_in_force.contains(time_in_force)
    }
}

/// A listing kept for lookups by symbol.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Markets {
    symbols: Vec<Symbol>,
    by_name: HashMap<String, usize>,
}

impl Markets {
    #[must_use]
    pub fn new(symbols: Vec<Symbol>) -> Self {
        let by_name = symbols.iter().enumerate().map(|(at, symbol)| (symbol.symbol.clone(), at)).collect();
        Markets { symbols, by_name }
    }

    /// The symbol named `name`, exactly as the venue spells it.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.by_name.get(name).and_then(|at| self.symbols.get(*at))
    }

    /// Every symbol, in the venue's order.
    #[must_use]
    pub fn all(&self) -> &[Symbol] {
        &self.symbols
    }

    /// The symbols trading now.
    pub fn trading(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter().filter(|symbol| symbol.is_trading())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

impl From<ExchangeInfo> for Markets {
    fn from(info: ExchangeInfo) -> Self {
        Markets::new(info.symbols)
    }
}

/// `GET /fapi/v3/exchangeInfo`.
#[must_use]
pub fn exchange_info_call() -> Call {
    Call::get("/fapi/v3/exchangeInfo")
}

/// The whole listing.
pub async fn exchange_info(transport: &impl Transport) -> Result<ExchangeInfo, ApiError> {
    read(transport, exchange_info_call()).await
}

/// The listing as a [`Markets`] snapshot.
pub async fn markets(transport: &impl Transport) -> Result<Markets, ApiError> {
    exchange_info(transport).await.map(Markets::from)
}

#[cfg(test)]
#[path = "tests/info.rs"]
mod tests;
