//! A symbol's grid: the steps its prices and quantities move in, their bounds, and the smallest
//! notional an order may have, from its `exchangeInfo` filters. An order off the grid is refused
//! by the venue (`-4014`, `-4023`, `-4164`), so an order is fitted to it before it is sent.

use crate::utils::dec::{Dec, Round};

/// What a symbol's filters allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Grid {
    /// `PRICE_FILTER`: the price step, and the bounds (zero for none).
    pub tick: Dec,
    pub min_price: Dec,
    pub max_price: Dec,
    /// `LOT_SIZE`: the quantity step and bounds of a limit order.
    pub step: Dec,
    pub min_qty: Dec,
    pub max_qty: Dec,
    /// `MARKET_LOT_SIZE`: the same for a market order.
    pub market_step: Dec,
    pub market_min_qty: Dec,
    pub market_max_qty: Dec,
    /// `MIN_NOTIONAL`: the least price times quantity.
    pub min_notional: Dec,
}

/// Why a figure does not fit the grid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OffGrid {
    #[error("{what} {value} is not positive")]
    NotPositive { what: &'static str, value: Dec },
    #[error("{what} {value} is below the least, {least}")]
    BelowMin { what: &'static str, value: Dec, least: Dec },
    #[error("{what} {value} is above the most, {most}")]
    AboveMax { what: &'static str, value: Dec, most: Dec },
    #[error("the notional {notional} is below the least, {least}")]
    BelowNotional { notional: Dec, least: Dec },
}

impl Grid {
    /// `price` on the tick, rounded as asked, within the bounds.
    pub fn price(&self, price: Dec, round: Round) -> Result<Dec, OffGrid> {
        let fitted = to_step(price, self.tick, round);
        bounded("price", fitted, self.min_price, self.max_price)
    }

    /// `quantity` on a limit order's step, rounded down, within the bounds.
    pub fn quantity(&self, quantity: Dec) -> Result<Dec, OffGrid> {
        let fitted = to_step(quantity, self.step, Round::Down);
        bounded("quantity", fitted, self.min_qty, self.max_qty)
    }

    /// `quantity` on a market order's step, rounded down, within its bounds.
    pub fn market_quantity(&self, quantity: Dec) -> Result<Dec, OffGrid> {
        let step = if self.market_step.is_positive() { self.market_step } else { self.step };
        let fitted = to_step(quantity, step, Round::Down);
        let least = if self.market_min_qty.is_positive() { self.market_min_qty } else { self.min_qty };
        let most = if self.market_max_qty.is_positive() { self.market_max_qty } else { self.max_qty };
        bounded("quantity", fitted, least, most)
    }

    /// Whether `price` times `quantity` reaches the least notional.
    pub fn check_notional(&self, price: Dec, quantity: Dec) -> Result<(), OffGrid> {
        let notional = price.checked_mul(quantity).unwrap_or(Dec::ZERO);
        match notional < self.min_notional {
            true => Err(OffGrid::BelowNotional { notional, least: self.min_notional }),
            false => Ok(()),
        }
    }

    /// The places a price is written with: the tick's.
    #[must_use]
    pub fn price_places(&self) -> u32 {
        self.tick.places()
    }

    /// The places a quantity is written with: the step's.
    #[must_use]
    pub fn quantity_places(&self) -> u32 {
        self.step.places()
    }
}

/// `value` as a whole number of `step`s, rounded as asked; unchanged when there is no step.
#[must_use]
pub fn to_step(value: Dec, step: Dec, round: Round) -> Dec {
    if !step.is_positive() {
        return value;
    }
    let places = step.places().max(value.places());
    match value.checked_div(step, 0, round).and_then(|steps| steps.checked_mul(step)) {
        Some(fitted) => fitted.round(places, Round::Nearest),
        None => value,
    }
}

fn bounded(what: &'static str, value: Dec, least: Dec, most: Dec) -> Result<Dec, OffGrid> {
    if !value.is_positive() {
        return Err(OffGrid::NotPositive { what, value });
    }
    if least.is_positive() && value < least {
        return Err(OffGrid::BelowMin { what, value, least });
    }
    if most.is_positive() && value > most {
        return Err(OffGrid::AboveMax { what, value, most });
    }
    Ok(value)
}

#[cfg(test)]
#[path = "tests/grid.rs"]
mod tests;
