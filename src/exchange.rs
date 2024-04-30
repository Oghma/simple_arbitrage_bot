//! Exchange implementations

#[derive(Clone, Debug)]
pub enum OrderBookMessage {
    Snapshot {
        bids: Vec<BookEntry>,
        asks: Vec<BookEntry>,
    },
    AskUpdate(BookEntry),
    BidUpdate(BookEntry),
}

#[derive(Clone, Debug)]
pub struct BookEntry {
    pub amount: String,
    pub price: String,
}
