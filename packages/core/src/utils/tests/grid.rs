//! Prices and quantities fitted to a symbol's steps and bounds.

use super::*;

fn d(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

fn btc() -> Grid {
    Grid {
        tick: d("0.1"),
        min_price: d("1"),
        max_price: d("1000000"),
        step: d("0.001"),
        min_qty: d("0.001"),
        max_qty: d("1000"),
        market_step: d("0.001"),
        market_min_qty: d("0.001"),
        market_max_qty: d("120"),
        min_notional: d("5"),
    }
}

#[test]
fn prices_and_quantities_land_on_the_grid() {
    let grid = btc();
    assert_eq!(grid.price(d("83000.17"), Round::Down).unwrap(), d("83000.1"));
    assert_eq!(grid.price(d("83000.11"), Round::Up).unwrap(), d("83000.2"));
    assert_eq!(grid.price(d("83000.15"), Round::Nearest).unwrap(), d("83000.2"));
    assert_eq!(grid.quantity(d("0.0029")).unwrap(), d("0.002"));
    assert_eq!(
        grid.market_quantity(d("200")).unwrap_err(),
        OffGrid::AboveMax { what: "quantity", value: d("200"), most: d("120") }
    );
    assert!(matches!(grid.quantity(d("0.0009")), Err(OffGrid::NotPositive { .. })));
    assert!(grid.check_notional(d("83000"), d("0.001")).is_ok());
    assert!(grid.check_notional(d("1000"), d("0.001")).is_err());
    assert_eq!((grid.price_places(), grid.quantity_places()), (1, 3));
    assert_eq!(to_step(d("0.012345"), d("0.00001"), Round::Down), d("0.01234"));
    assert_eq!(to_step(d("7"), Dec::ZERO, Round::Down), d("7"));
}
