//! DyDx exchange implementation

use std::{collections::HashMap, task::Poll};

use futures_util::{SinkExt, Stream, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{BookEntry, Exchange, OrderBook, OrderBookMessage, Symbol};

const WSS_URL: &str = "wss://indexer.dydx.trade/v4/ws";

pub struct DyDx {
    receiver: mpsc::Receiver<BookRawMessage>,
    sender: mpsc::Sender<BookRawMessage>,
    order_book: OrderBook,
    persistent_trades: bool,
}

impl DyDx {
    pub fn new(persistent_trades: bool) -> Self {
        let (sender, receiver) = mpsc::channel(10000);
        Self {
            receiver,
            sender,
            order_book: OrderBook::new(),
            persistent_trades,
        }
    }
}

impl Exchange for DyDx {
    fn order_book_subscribe(&self, symbol: &Symbol) {
        // This is very ugly and should not be done. The connection with the
        // exchange should be created when the object is created. The method
        // should only send the subscription to the order book channel
        let symbol = symbol.clone();
        tokio::spawn(handle_wss(symbol, self.sender.clone()));
    }

    async fn handle_persistent_buy(&self, amount: Decimal, price: Decimal) -> anyhow::Result<()> {
        if !self.persistent_trades {
            return Ok(());
        }

        let Some(ask) = self.order_book.best_ask() else {
            return Ok(());
        };

        // Update the entry
        let entry = BookEntry {
            price,
            amount: ask.amount - amount,
        };
        let mut update = BookRawMessage::default();
        update.contents.insert("asks".to_string(), vec![entry]);
        self.sender.send(update).await?;
        Ok(())
    }

    async fn handle_persistent_sell(&self, amount: Decimal, price: Decimal) -> anyhow::Result<()> {
        if !self.persistent_trades {
            return Ok(());
        }

        let Some(bid) = self.order_book.best_bid() else {
            return Ok(());
        };

        // Update the entry
        let entry = BookEntry {
            price,
            amount: bid.amount - amount,
        };
        let mut update = BookRawMessage::default();
        update.contents.insert("bids".to_string(), vec![entry]);
        self.sender.send(update).await?;
        Ok(())
    }
}

impl Stream for DyDx {
    type Item = (Option<BookEntry>, Option<BookEntry>);

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(msg)) => {
                let update = match (
                    msg.contents.contains_key("asks"),
                    msg.contents.contains_key("bids"),
                ) {
                    // Snapshot
                    (true, true) => Some(OrderBookMessage::Snapshot {
                        bids: msg.contents["bids"].clone(),
                        asks: msg.contents["asks"].clone(),
                    }),
                    // Bid update
                    (false, true) => {
                        Some(OrderBookMessage::BidUpdate(msg.contents["bids"][0].clone()))
                    }
                    // Ask update
                    (true, false) => {
                        Some(OrderBookMessage::AskUpdate(msg.contents["asks"][0].clone()))
                    }
                    // Both are empty, ignore
                    _ => None,
                };
                if let Some(upd) = update {
                    self.order_book.update(upd);
                }
                Poll::Ready(Some((
                    self.order_book.best_ask().cloned(),
                    self.order_book.best_bid().cloned(),
                )))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

async fn handle_wss(symbol: Symbol, channel: mpsc::Sender<BookRawMessage>) {
    loop {
        //Connect to DyDx
        let (mut wss_stream, _) = connect_async(WSS_URL).await.expect("Failed to connect");
        // Send the order book subscription request
        wss_stream
            .send(Message::Text(
                json!({"type":"subscribe", "channel":"v4_orderbook", "id":symbol.0}).to_string(),
            ))
            .await
            .expect("Failed to send orderbook subscription");

        wss_stream
            .for_each(|message| async {
                // If we receive an error close the connection and try to reconnect
                let Ok(message) = message else {
                    return;
                };
                // Again, at the moment if we fail to convert in string close
                // the connection and try to reconnect
                let Ok(message) = message.into_text() else {
                    return;
                };

                match serde_json::from_str::<BookRawMessage>(&message) {
                    Ok(msg) => channel.send(msg).await.unwrap(),
                    Err(_) => tracing::debug!("received unknown message {:?}", message),
                }
            })
            .await;
    }
}

#[derive(Deserialize, Debug, Default)]
#[allow(dead_code)]
pub struct BookRawMessage {
    #[serde(alias = "type")]
    msg_type: String,
    connection_id: String,
    message_id: usize,
    channel: String,
    id: String,
    contents: HashMap<String, Vec<BookEntry>>,
}
