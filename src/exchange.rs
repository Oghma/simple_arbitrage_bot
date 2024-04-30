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
