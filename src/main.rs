use std::str::FromStr;

use exchange::{BookEntry, Exchange, Symbol, Wallet};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

mod exchange;

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

    let mut aevo = exchange::Aevo::new(persistent_trades, aevo_fee);
    let mut dydx = exchange::DyDx::new(persistent_trades, dydx_fee);

    aevo.order_book_subscribe(&aevo_symbol);
    dydx.order_book_subscribe(&dydx_symbol);

    let mut wallets_initialized = false;
    let mut aevo_wallet = Wallet::new(starting_value);
    let mut dydx_wallet = Wallet::new(starting_value);

    let mut aevo_prices = (None, None);
    let mut dydx_prices = (None, None);

    tracing::info!("initialized, starting...");

    loop {
        tokio::select! {
            Some(bests) = dydx.next() => {aevo_prices = bests;}
            Some(bests) = aevo.next() => {dydx_prices = bests;}
        };

        let aevo_prices = match aevo_prices {
            (Some(ref bid), Some(ref ask)) => (bid, ask),
            _ => continue,
        };

        let dydx_prices = match dydx_prices {
            (Some(ref bid), Some(ref ask)) => (bid, ask),
            _ => continue,
        };

        //Let assume the current price of base token is always the bid price from Aevo
        let curr_base_price = aevo_prices.0.price;

        if !wallets_initialized {
            aevo_wallet.rebalance(curr_base_price);
            dydx_wallet.rebalance(curr_base_price);
            wallets_initialized = true;
            tracing::debug!("wallets rebalanced {:?} {:?}", aevo_wallet, dydx_wallet);
        }

        let spread1 = calculate_spread(aevo_prices.1.price, dydx_prices.0.price);
        let spread2 = calculate_spread(dydx_prices.1.price, aevo_prices.0.price);

        if spread1 > spread2 && spread1 > dec!(0) {
            (aevo_wallet, dydx_wallet) = run_strategy(
                &aevo,
                &dydx,
                aevo_prices,
                dydx_prices,
                aevo_wallet,
                dydx_wallet,
                starting_value,
                curr_base_price,
            )
            .await?;
        } else {
            (dydx_wallet, aevo_wallet) = run_strategy(
                &dydx,
                &aevo,
                dydx_prices,
                aevo_prices,
                dydx_wallet,
                aevo_wallet,
                starting_value,
                curr_base_price,
            )
            .await?;
        };
    }
}

fn calculate_spread(ask: Decimal, bid: Decimal) -> Decimal {
    let num = bid - ask;
    num / ((bid + ask) / dec!(2))
}

fn calculate_pl(
    starting_value: Decimal,
    curr_price: Decimal,
    wallet1: &Wallet,
    wallet2: &Wallet,
) -> (Decimal, Decimal) {
    // This is not the standard formula for calculating P&L
    let starting_value = starting_value * dec!(2);
    let total =
        wallet1.quote + wallet2.quote + wallet1.base * curr_price + wallet2.base * curr_price;
    let pl = (total / starting_value) - dec!(1);

    (pl, total)
}

fn is_profitable(
    amount: Decimal,
    sell_price: Decimal,
    buy_price: Decimal,
    sell_fee: Decimal,
    buy_fee: Decimal,
) -> bool {
    let sell = amount * sell_price;
    let buy = amount * buy_price;

    sell - buy - sell * sell_fee - buy * buy_fee > dec!(0)
}

async fn run_strategy(
    exc1: &impl Exchange,
    exc2: &impl Exchange,
    exc1_prices: (&BookEntry, &BookEntry),
    exc2_prices: (&BookEntry, &BookEntry),
    exc1_wallet: Wallet,
    exc2_wallet: Wallet,
    starting_value: Decimal,
    ticker_base_price: Decimal,
) -> anyhow::Result<(Wallet, Wallet)> {
    // We are going to buy on exc1 and sell on exc2
    // Find the maximum amount we can trade. The amount is calculated as the
    // minimum between exc1 best ask, exc2 best bid and the amount of the base
    // token in the exc1 wallet.
    let mut amount = exc1_prices
        .1
        .amount
        .min(exc2_prices.0.amount.min(exc2_wallet.base));

    // We need to find out if we have money to trade. Otherwise, use the whole
    // budget.
    let max_quote_amount = exc1_wallet.quote.min(amount * exc1_prices.1.price);
    amount = max_quote_amount / exc1_prices.1.price;

    if amount.is_zero()
        || !is_profitable(
            amount,
            exc2_prices.0.price,
            exc1_prices.1.price,
            exc2.fee(),
            exc1.fee(),
        )
    {
        return Ok((exc1_wallet, exc2_wallet));
    }

    let exc1_wallet = exc1.buy(amount, exc1_prices.1.price, exc1_wallet).await?;
    let exc2_wallet = exc2.sell(amount, exc2_prices.0.price, exc2_wallet).await?;

    tracing::info!(
        "================================================================================"
    );
    tracing::info!(
        "BUY on {} amount: {:.4} price: {:.4}",
        exc1,
        amount,
        exc1_prices.1.price
    );
    tracing::info!(
        "SELL on {} amount {:.4} price {:.4}",
        exc2,
        amount,
        exc2_prices.0.price
    );
    tracing::info!("{} wallet {}", exc1, exc1_wallet);
    tracing::info!("{} wallet {}", exc2, exc2_wallet);

    let (pl, total) = calculate_pl(
        starting_value,
        ticker_base_price,
        &exc1_wallet,
        &exc2_wallet,
    );
    tracing::info!("total balance {}. New P&L {:.4}%", total, pl);
    tracing::info!(
        "================================================================================"
    );
    tracing::info!("");

    return Ok((exc1_wallet, exc2_wallet));
}
