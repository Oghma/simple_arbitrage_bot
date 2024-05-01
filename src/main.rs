use std::str::FromStr;

use exchange::{Exchange, Symbol, Wallet};
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
    let aevo_fee = std::env::var("AEVO_FEE")?.parse()?;
    let dydx_symbol = Symbol::from_str(&std::env::var("DYDX_SYMBOL")?)?;
    let dydx_fee = std::env::var("DYDX_FEE")?.parse()?;
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
            // We are going to buy on Aevo and sell on DyDx
            // Find the maximum amount we can trade
            let mut amount = aevo_prices.1.amount.min(dydx_prices.1.amount);
            // We need to find out if we have money to trade. Otherwise, use
            // the whole budget.
            let max_amount_quote = aevo_wallet.quote.min(amount * curr_base_price);
            let amount_quote = max_amount_quote.min(dydx_wallet.base * curr_base_price);

            amount = amount_quote / curr_base_price;

            if amount.is_zero()
                || !is_profitable(
                    amount,
                    dydx_prices.0.price,
                    aevo_prices.1.price,
                    dydx_fee,
                    aevo_fee,
                )
            {
                continue;
            }

            // Now that we have an amount to trade, place the orders
            aevo_wallet = aevo.buy(amount, aevo_prices.1.price, aevo_wallet).await?;
            dydx_wallet = dydx.sell(amount, dydx_prices.0.price, dydx_wallet).await?;

            tracing::info!(
                "================================================================================"
            );
            tracing::info!(
                "BUY on Aevo amount: {:.4} price: {:.4}",
                amount,
                aevo_prices.1.price
            );
            tracing::info!(
                "SELL on DyDx amount {:.4} price {:.4}",
                amount,
                dydx_prices.0.price
            );
            tracing::info!("Aevo wallet {}", aevo_wallet);
            tracing::info!("Dydx wallet {}", dydx_wallet);
            let (pl, total) =
                calculate_pl(starting_value, curr_base_price, &aevo_wallet, &dydx_wallet);
            tracing::info!("total balance {}. New P&L {:.4}%", total, pl);
            tracing::info!(
                "================================================================================"
            );
            tracing::info!("");
        } else if spread2 > dec!(0) {
            //We are going to buy on DyDx and sell on Aevo
            // Find the maximum amount we can trade
            let mut amount = aevo_prices.0.amount.min(dydx_prices.0.amount);
            // We need to find out if we have enough money to trade. Otherwise,
            // use the whole budget.
            let max_amount_quote = dydx_wallet.quote.min(amount * curr_base_price);
            let amount_quote = max_amount_quote.min(aevo_wallet.base * curr_base_price);

            amount = amount_quote / curr_base_price;

            if amount.is_zero()
                || !is_profitable(
                    amount,
                    aevo_prices.0.price,
                    dydx_prices.1.price,
                    aevo_fee,
                    dydx_fee,
                )
            {
                continue;
            }

            // Now that we have an amount to trade, place the orders
            aevo_wallet = aevo.sell(amount, aevo_prices.0.price, aevo_wallet).await?;
            dydx_wallet = dydx.buy(amount, dydx_prices.1.price, dydx_wallet).await?;

            tracing::info!(
                "================================================================================"
            );
            tracing::info!(
                "BUY on DyDx amount: {:.4} price: {:.4}",
                amount,
                dydx_prices.1.price
            );
            tracing::info!(
                "SELL on Aevo amount {:.4} price {:.4}",
                amount,
                aevo_prices.0.price
            );
            tracing::info!("Aevo wallet {}", aevo_wallet);
            tracing::info!("Dydx wallet {}", dydx_wallet);
            let (pl, total) =
                calculate_pl(starting_value, curr_base_price, &aevo_wallet, &dydx_wallet);
            tracing::info!("total balance {}. New P&L {:.4}%", total, pl);
            tracing::info!(
                "================================================================================"
            );
            tracing::info!("");
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
