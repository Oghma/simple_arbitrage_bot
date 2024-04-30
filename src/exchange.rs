//! Exchange implementations

mod aevo;

use futures_util::Stream;
use serde::Deserialize;

trait Exchange: Stream {
    fn order_book_subscribe(&self, symbol: &'static Symbol);
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

pub struct Symbol(String);
