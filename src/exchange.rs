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

#[derive(Debug)]
pub struct Wallet {
    pub base: f64,
    pub quote: f64,
}

impl Wallet {
    pub fn new(initial_amount: f64) -> Wallet {
        Self {
            base: 0.0,
            quote: initial_amount,
        }
    }

    pub fn rebalance(&mut self, price: f64) {
        self.quote = self.quote / 2.0;
        self.base = self.quote / price;
    }
}
