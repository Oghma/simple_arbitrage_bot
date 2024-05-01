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

#[derive(Debug)]
pub struct OrderBook {
    pub bids: Vec<TmpBookEntry>,
    pub asks: Vec<TmpBookEntry>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }

    pub fn best_ask(&self) -> Option<&TmpBookEntry> {
        self.asks.get(0)
    }

    pub fn best_bid(&self) -> Option<&TmpBookEntry> {
        self.bids.get(0)
    }

    pub fn update(&mut self, update: OrderBookMessage) {
        match update {
            OrderBookMessage::Snapshot { bids, asks } => {
                self.bids = bids
                    .iter()
                    .map(|bid| TmpBookEntry {
                        amount: bid.amount.parse().unwrap(),
                        price: bid.price.parse().unwrap(),
                    })
                    .collect();
                self.asks = asks
                    .iter()
                    .map(|ask| TmpBookEntry {
                        amount: ask.amount.parse().unwrap(),
                        price: ask.price.parse().unwrap(),
                    })
                    .collect();
            }
            OrderBookMessage::BidUpdate(entry) => {
                let amount: f64 = entry.amount.parse().unwrap();
                let price: f64 = entry.price.parse().unwrap();

                // Remove the entry
                if amount == 0.0 {
                    if let Some(index) = self.bids.iter().position(|bid| bid.price == price) {
                        self.bids.remove(index);
                    }
                } else {
                    // Update the entry
                    if let Some(index) = self.bids.iter().position(|bid| bid.price == price) {
                        self.bids[index].amount = amount;
                    } else {
                        // New entry
                        self.bids.push(TmpBookEntry { amount, price });

                        self.bids
                            .sort_by(|val1, val2| val1.price.partial_cmp(&val2.price).unwrap());
                    }
                }
            }

            OrderBookMessage::AskUpdate(entry) => {
                let amount: f64 = entry.amount.parse().unwrap();
                let price: f64 = entry.price.parse().unwrap();

                // Remove the entry
                if amount == 0.0 {
                    if let Some(index) = self.asks.iter().position(|ask| ask.price == price) {
                        self.asks.remove(index);
                    }
                } else {
                    // Update the entry
                    if let Some(index) = self.asks.iter().position(|ask| ask.price == price) {
                        self.asks[index].amount = amount;
                    } else {
                        // New entry
                        self.asks.push(TmpBookEntry { amount, price });

                        self.asks
                            .sort_by(|val1, val2| val1.price.partial_cmp(&val2.price).unwrap());
                    }
                }
            }
        }
    }
}

#[derive(Clone, Deserialize, Debug)]
pub struct TmpBookEntry {
    pub amount: f64,
    pub price: f64,
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
    pub price: String,
    #[serde(alias = "size")]
    pub amount: String,
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
