//! Reads BTCUSDT's rules and book from Aster and prints the unsigned market order a $100 buy would
//! send: `cargo run -p df-aster-core --example market_order`. Name another symbol
//! (`-- ETHUSDT`) to read that one instead.

use std::error::Error;

use df_aster_core::Client;
use df_aster_core::client::native::{Native, clock};
use df_aster_core::markets::book::{DepthLimit, depth};
use df_aster_core::markets::info::markets;
use df_aster_core::trade::orders::NewOrder;
use df_aster_core::utils::{Dec, Round, Side};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let symbol = std::env::args().nth(1).unwrap_or_else(|| "BTCUSDT".to_owned());
    let client = Client::reader(Native::new(), clock);

    let listed = markets(&client).await?;
    let market = listed.get(&symbol).ok_or("not listed")?;
    let grid = market.grid();
    let book = depth(&client, &symbol, DepthLimit::at_least(5)).await?;
    let ask = book.best_ask().ok_or("no one is selling")?;

    println!("{symbol}  tick {}  step {}  least notional {}", grid.tick, grid.step, grid.min_notional);
    println!("     bid {}  ask {}", show(book.best_bid().map(|level| level.price)), ask.price);

    let budget = Dec::from_int(100);
    let quantity = budget.checked_div(ask.price, grid.quantity_places(), Round::Down).ok_or("no quantity")?;
    let order = NewOrder::market(&symbol, Side::Buy, quantity).fit(&grid, Round::Down)?;
    let fields: Vec<String> = order.fields()?.into_iter().map(|(key, value)| format!("{key}={value}")).collect();
    println!("\nwould sign: {}", fields.join("&"));

    let spent = client.spent();
    println!("\nspent {} of the IP's {} a minute", spent.minute, spent.budget);
    Ok(())
}

fn show(value: Option<Dec>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}
