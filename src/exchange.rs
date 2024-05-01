//! Exchange implementations

mod aevo;
mod dydx;

use std::{convert, str::FromStr};

pub use aevo::Aevo;
pub use dydx::DyDx;

use futures_util::Stream;
use serde::Deserialize;

pub trait Exchange: Stream {
    fn order_book_subscribe(&self, symbol: &Symbol);

    fn buy(&self, amount: f64, price: f64, wallet: Wallet) -> anyhow::Result<Wallet> {
        // We are buying base token for quote token
        let new_base_amount = wallet.base + amount;
        let new_quote_amount = wallet.quote - (price * amount);
        Ok(Wallet {
            base: new_base_amount,
            quote: new_quote_amount,
        })
    }

    fn sell(&self, amount: f64, price: f64, wallet: Wallet) -> anyhow::Result<Wallet> {
        // We are selling base token for quote token
        let new_base_amount = wallet.base - amount;
        let new_quote_amount = wallet.quote + (price * amount);
        Ok(Wallet {
            base: new_base_amount,
            quote: new_quote_amount,
        })
    }
}

}

#[derive(Clone, Debug)]
pub enum OrderBookMessage {
    Snapshot {
        bids: Vec<BookEntry>,
        asks: Vec<BookEntry>,
    },
    AskUpdate(BookEntry),
    BidUpdate(BookEntry),
}

#[derive(Clone, Deserialize, Debug)]
pub struct BookEntry {
    #[serde(alias = "size")]
    pub amount: String,
    pub price: String,
}

#[derive(Clone)]
pub struct Symbol(String);

impl FromStr for Symbol {
    type Err = convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Symbol(s.to_string()))
    }
}

pub struct Wallet {
    pub base: f64,
    pub quote: f64,
}

impl Wallet {
    pub fn init(initial_amount: f64, price: f64) -> Wallet {
        let quote = initial_amount / 2.0;
        Self {
            base: quote / price,
            quote,
        }
    }
}
