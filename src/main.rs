use std::str::FromStr;

use exchange::Symbol;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

mod bot;
mod exchange;

struct Config {
    aevo_symbol: Symbol,
    aevo_fee: Decimal,
    dydx_symbol: Symbol,
    dydx_fee: Decimal,
    starting_value: Decimal,
    persistent_trades: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    tracing::info!("starting bot");

    tracing::info!("initializing...");
    // Configuration
    let aevo_symbol = Symbol::from_str(&std::env::var("AEVO_SYMBOL")?)?;
    let aevo_fee = std::env::var("AEVO_FEE")?.parse::<Decimal>()? / dec!(100);
    let dydx_symbol = Symbol::from_str(&std::env::var("DYDX_SYMBOL")?)?;
    let dydx_fee = std::env::var("DYDX_FEE")?.parse::<Decimal>()? / dec!(100);
    let starting_value = std::env::var("STARTING_VALUE")?.parse()?;
    let persistent_trades = std::env::var("PERSISTENT_TRADES")?.parse()?;

    let config = Config {
        aevo_symbol,
        aevo_fee,
        dydx_symbol,
        dydx_fee,
        starting_value,
        persistent_trades,
    };

    bot::run_bot(&config).await?;

    Ok(())
}
