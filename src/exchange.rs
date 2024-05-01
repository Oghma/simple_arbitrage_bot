//! Exchange implementations

mod aevo;
mod dydx;

use std::{convert, str::FromStr};

pub use aevo::Aevo;
pub use dydx::DyDx;

use futures_util::Stream;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;

pub trait Exchange: Stream {
    fn order_book_subscribe(&self, symbol: &Symbol);

    fn buy(&self, amount: Decimal, price: Decimal, wallet: Wallet) -> anyhow::Result<Wallet> {
        // We are buying base token for quote token
        let new_base_amount = wallet.base + amount;
        let new_quote_amount = wallet.quote - (price * amount);
        Ok(Wallet {
            base: new_base_amount,
            quote: new_quote_amount,
        })
    }

    fn sell(&self, amount: Decimal, price: Decimal, wallet: Wallet) -> anyhow::Result<Wallet> {
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
                        amount: bid.amount,
                        price: bid.price,
                    })
                    .collect();
                self.asks = asks
                    .iter()
                    .map(|ask| TmpBookEntry {
                        amount: ask.amount,
                        price: ask.price,
                    })
                    .collect();
            }
            OrderBookMessage::BidUpdate(entry) => {
                let amount: Decimal = entry.amount;
                let price: Decimal = entry.price;

                // Remove the entry
                if amount.is_zero() {
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
                let amount: Decimal = entry.amount;
                let price: Decimal = entry.price;

                // Remove the entry
                if amount.is_zero() {
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
    pub base: Decimal,
    pub quote: Decimal,
}

impl Wallet {
    pub fn new(initial_amount: Decimal) -> Wallet {
        Self {
            base: dec!(0),
            quote: initial_amount,
        }
    }

    pub fn rebalance(&mut self, price: Decimal) {
        self.quote = self.quote / dec!(2);
        self.base = self.quote / price;
    }
}
