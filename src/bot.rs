//! Arbitrage bot

use std::pin::Pin;

use crate::{
    exchange::{Aevo, BookEntry, DyDx, Exchange, Wallet},
    Config,
};
use futures_util::StreamExt;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio_stream::StreamMap;

pub async fn run_bot(config: &Config) -> anyhow::Result<()> {
    let aevo = Aevo::new(config.persistent_trades, config.aevo_fee);
    let dydx = DyDx::new(config.persistent_trades, config.dydx_fee);

    aevo.order_book_subscribe(&config.aevo_symbol);
    dydx.order_book_subscribe(&config.dydx_symbol);

    let mut wallets_initialized = false;
    let mut aevo_wallet = Wallet::new(config.starting_value);
    let mut dydx_wallet = Wallet::new(config.starting_value);

    let mut exchanges = StreamMap::<
        usize,
        Pin<Box<dyn Exchange<Item = (Option<BookEntry>, Option<BookEntry>)>>>,
    >::new();

    exchanges.insert(0, Box::pin(aevo));
    exchanges.insert(1, Box::pin(dydx));

    let mut best_prices = vec![(None, None), (None, None)];
    tracing::info!("bot initialized, starting...");

    while let Some((key, update)) = exchanges.next().await {
        match update {
            (Some(bid), Some(ask)) => best_prices[key] = (Some(bid), Some(ask)),
            _ => continue,
        };

        // The firsts iterations have empty order books. Wait until are filled
        if best_prices
            .iter()
            .all(|price| price.0.is_none() && price.1.is_none())
        {
            continue;
        }

        //Let assume the current price of base token is always the bid price from Aevo
        let curr_base_price = best_prices[0].0.as_ref().map(|entry| entry.price).unwrap();

        if !wallets_initialized {
            aevo_wallet.rebalance(curr_base_price);
            dydx_wallet.rebalance(curr_base_price);
            wallets_initialized = true;
            tracing::debug!("wallets rebalanced {:?} {:?}", aevo_wallet, dydx_wallet);
        }

        // We want the smallest best ask because we buy from it
        let best_ask = best_prices
            .iter()
            .enumerate()
            .min_by(|(_, num1), (_, num2)| {
                num1.1
                    .clone()
                    .unwrap()
                    .price
                    .cmp(&num2.1.clone().unwrap().price)
            })
            .map(|(key, ask)| (key, ask.1.clone().unwrap()))
            .unwrap();

        // We want the biggest best bid because we sell to it
        let best_bid = best_prices
            .iter()
            .enumerate()
            .max_by(|(_, num1), (_, num2)| {
                num1.0
                    .clone()
                    .unwrap()
                    .price
                    .cmp(&num2.0.clone().unwrap().price)
            })
            .map(|(key, ask)| (key, ask.0.clone().unwrap()))
            .unwrap();

        // Check best_bid and best_ask are in different exchanges
        if best_bid.0 == best_ask.0 {
            continue;
        }

        if calculate_spread(best_ask.1.price, best_bid.1.price) > dec!(0) {
            if best_ask.0 == 0 {
                // Buy on Aevo and sell on DyDx
                //FIXME: Not working
                (aevo_wallet, dydx_wallet) = run_strategy(
                    &aevo,
                    &dydx,
                    &best_ask.1,
                    &best_bid.1,
                    aevo_wallet,
                    dydx_wallet,
                    config.starting_value,
                    curr_base_price,
                )
                .await?;
            } else {
                // Buy on DyDx and sell on Aevo
                //FIXME: Not working
                (dydx_wallet, aevo_wallet) = run_strategy(
                    &dydx,
                    &aevo,
                    &best_ask.1,
                    &best_bid.1,
                    dydx_wallet,
                    aevo_wallet,
                    config.starting_value,
                    curr_base_price,
                )
                .await?;
            }
        }
    }

    Ok(())
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
    exc1_prices: &BookEntry,
    exc2_prices: &BookEntry,
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
        .amount
        .min(exc2_prices.amount.min(exc2_wallet.base));

    // We need to find out if we have money to trade. Otherwise, use the whole
    // budget.
    let max_quote_amount = exc1_wallet.quote.min(amount * exc1_prices.price);
    amount = max_quote_amount / exc1_prices.price;

    if amount.is_zero()
        || !is_profitable(
            amount,
            exc2_prices.price,
            exc1_prices.price,
            exc2.fee(),
            exc1.fee(),
        )
    {
        return Ok((exc1_wallet, exc2_wallet));
    }

    let exc1_wallet = exc1.buy(amount, exc1_prices.price, exc1_wallet).await?;
    let exc2_wallet = exc2.sell(amount, exc2_prices.price, exc2_wallet).await?;

    tracing::info!(
        "================================================================================"
    );
    tracing::info!(
        "BUY on {} amount: {:.4} price: {:.4}",
        exc1,
        amount,
        exc1_prices.price
    );
    tracing::info!(
        "SELL on {} amount {:.4} price {:.4}",
        exc2,
        amount,
        exc2_prices.price
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

    Ok((exc1_wallet, exc2_wallet))
}
