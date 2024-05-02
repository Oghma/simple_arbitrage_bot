//! Aevo exchange implementation

use std::task::Poll;

use futures_util::{SinkExt, Stream, StreamExt};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{BookEntry, Exchange, OrderBook, OrderBookMessage, Symbol};

const WSS_URL: &str = "wss://ws.aevo.xyz";

pub struct Aevo {
    receiver: mpsc::Receiver<BookRawMessage>,
    sender: mpsc::Sender<BookRawMessage>,
    order_book: OrderBook,
    persistent_trades: bool,
    fee: Decimal,
}

impl Aevo {
    pub fn new(persistent_trades: bool, fee: Decimal) -> Self {
        let (sender, receiver) = mpsc::channel(10000);

        Self {
            receiver,
            sender,
            order_book: OrderBook::new(),
            persistent_trades,
            fee,
        }
    }
}

impl Exchange for Aevo {
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
        let mut update = BookRawMessage {
            msg_type: "update".to_string(),
            ..Default::default()
        };
        update.asks.push(entry);
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
        let mut update = BookRawMessage {
            msg_type: "update".to_string(),
            ..Default::default()
        };
        update.bids.push(entry);
        self.sender.send(update).await?;
        Ok(())
    }

    fn fee(&self) -> Decimal {
        self.fee
    }
}

impl Stream for Aevo {
    type Item = (Option<BookEntry>, Option<BookEntry>);

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // We process order book messages internally. Return only best ask/bid
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(msg)) => {
                let update = match msg.msg_type.as_ref() {
                    "snapshot" => OrderBookMessage::Snapshot {
                        bids: msg.bids,
                        asks: msg.asks,
                    },
                    "update" => {
                        if msg.asks.is_empty() {
                            OrderBookMessage::BidUpdate(msg.bids[0].clone())
                        } else {
                            OrderBookMessage::AskUpdate(msg.asks[0].clone())
                        }
                    }
                    _ => panic!("received unknown orderbook message"),
                };
                self.order_book.update(update);
                Poll::Ready(Some((
                    self.order_book.best_bid().cloned(),
                    self.order_book.best_ask().cloned(),
                )))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

async fn handle_wss(symbol: Symbol, channel: mpsc::Sender<BookRawMessage>) {
    loop {
        // Connect to Aevo
        let (mut wss_stream, _) = connect_async(WSS_URL).await.expect("Failed to connect");
        // Send the order book subscription request
        wss_stream
            .send(Message::Text(
                json!({"op":"subscribe", "data":[format!("orderbook:{}", symbol.0)]}).to_string(),
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

                match serde_json::from_str::<AevoRawMessage>(&message) {
                    Ok(msg) => channel.send(msg.data).await.unwrap(),
                    Err(_) => tracing::debug!("received unknown message {:?}", message),
                }
            })
            .await;
    }
}

// Ignore unused variables for these two structs

#[derive(Deserialize, Debug, Default)]
#[allow(dead_code)]
struct BookRawMessage {
    #[serde(alias = "type")]
    msg_type: String,
    instrument_id: String,
    instrument_name: String,
    instrument_type: String,
    bids: Vec<BookEntry>,
    asks: Vec<BookEntry>,
    last_updated: String,
    checksum: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct AevoRawMessage {
    channel: Option<String>,
    data: BookRawMessage,
    write_ts: String,
}
