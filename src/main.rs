use std::str::FromStr;

use exchange::{Exchange, OrderBook, Symbol, Wallet};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

mod exchange;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Configuration
    let aevo_symbol = Symbol::from_str(&std::env::var("AEVO_SYMBOL")?)?;
    let dydx_symbol = Symbol::from_str(&std::env::var("DYDX_SYMBOL")?)?;
    let starting_value = std::env::var("STARTING_VALUE")?.parse()?;

    let mut aevo = exchange::Aevo::new();
    let mut dydx = exchange::DyDx::new();

    aevo.order_book_subscribe(&aevo_symbol);
    dydx.order_book_subscribe(&dydx_symbol);

    let mut aevo_book = OrderBook::new();
    let mut dydx_book = OrderBook::new();

    let mut wallets_initialized = false;
    let mut aevo_wallet = Wallet::new(starting_value);
    let mut dydx_wallet = Wallet::new(starting_value);

    loop {
        tokio::select! {
            Some(book_message) = dydx.next() => {dydx_book.update(book_message);}
            Some(book_message) = aevo.next() => {aevo_book.update(book_message);}
        };

        let aevo_prices = match (aevo_book.best_ask(), aevo_book.best_bid()) {
            (Some(bid), Some(ask)) => (bid, ask),
            _ => continue,
        };

        let dydx_prices = match (dydx_book.best_ask(), dydx_book.best_bid()) {
            (Some(bid), Some(ask)) => (bid, ask),
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
            // We are going to buy on Aevo and sell on DyDx
            // Find the maximum amount we can trade
            let mut amount = aevo_prices.1.amount.min(dydx_prices.1.amount);
            // We need to find out if we have money to trade. Otherwise, use
            // the whole budget.
            let max_amount_quote = aevo_wallet.quote.min(amount * curr_base_price);
            let amount_quote = max_amount_quote.min(dydx_wallet.base * curr_base_price);

            amount = amount_quote / curr_base_price;

            if amount.is_zero() {
                continue;
            }

            // Now that we have an amount to trade, place the orders
            aevo_wallet = aevo.buy(amount, aevo_prices.1.price, aevo_wallet)?;
            dydx_wallet = dydx.sell(amount, dydx_prices.0.price, dydx_wallet)?;

            tracing::info!(
                "new trade! Bought on Aevo and sold on DydX. New P&L {}",
                calculate_pl(starting_value, curr_base_price, &aevo_wallet, &dydx_wallet)
            );
            tracing::debug!("{:?} {:?}", aevo_wallet, dydx_wallet);
        } else if spread2 > dec!(0) {
            //We are going to buy on DyDx and sell on Aevo
            // Find the maximum amount we can trade
            let mut amount = aevo_prices.0.amount.min(dydx_prices.0.amount);
            // We need to find out if we have enough money to trade. Otherwise,
            // use the whole budget.
            let max_amount_quote = dydx_wallet.quote.min(amount * curr_base_price);
            let amount_quote = max_amount_quote.min(aevo_wallet.base * curr_base_price);

            amount = amount_quote / curr_base_price;

            if amount.is_zero() {
                continue;
            }

            // Now that we have an amount to trade, place the orders
            aevo_wallet = aevo.sell(amount, aevo_prices.0.price, aevo_wallet)?;
            dydx_wallet = dydx.buy(amount, dydx_prices.1.price, dydx_wallet)?;

            tracing::info!(
                "new trade! Bought on DyDx and sold on Aevo. New P&L {}",
                calculate_pl(starting_value, curr_base_price, &aevo_wallet, &dydx_wallet)
            );
            tracing::debug!("{:?} {:?}", aevo_wallet, dydx_wallet);
        }
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
) -> Decimal {
    // This is not the standard formula for calculating P&L
    let starting_value = starting_value * dec!(2);
    let total =
        wallet1.quote + wallet2.quote + wallet1.base * curr_price + wallet2.base * curr_price;

    (total / starting_value) - dec!(1)
}
