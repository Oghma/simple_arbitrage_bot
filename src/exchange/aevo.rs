//! Aevo exchange implementation

use std::task::Poll;

use futures_util::{SinkExt, Stream, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{BookEntry, Exchange, OrderBookMessage, Symbol};

const WSS_URL: &str = "wss://ws.aevo.xyz";

pub struct Aevo {
    receiver: mpsc::Receiver<BookRawMessage>,
    sender: mpsc::Sender<BookRawMessage>,
}

impl Aevo {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(10000);

        Self { receiver, sender }
    }
}

impl Exchange for Aevo {
    fn order_book_subscribe(&self, symbol: &'static Symbol) {
        // This is very ugly and should not be done. The connection with the
        // exchange should be created when the object is created. The method
        // should only send the subscription to the order book channel
        tokio::spawn(handle_wss(symbol, self.sender.clone()));
    }
}

impl Stream for Aevo {
    type Item = OrderBookMessage;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(msg)) => match msg.msg_type.as_ref() {
                "snapshot" => Poll::Ready(Some(OrderBookMessage::Snapshot {
                    bids: msg.bids,
                    asks: msg.asks,
                })),
                "update" => {
                    if msg.asks.is_empty() {
                        Poll::Ready(Some(OrderBookMessage::BidUpdate(msg.bids[0].clone())))
                    } else {
                        Poll::Ready(Some(OrderBookMessage::AskUpdate(msg.asks[0].clone())))
                    }
                }
                _ => panic!("received unknown orderbook message"),
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

async fn handle_wss(symbol: &Symbol, channel: mpsc::Sender<BookRawMessage>) {
    //FIXME: Remove all these unwrap
    loop {
        // Connect to Aevo
        let (mut wss_stream, _) = connect_async(WSS_URL).await.expect("Failed to connect");
        // Send the order book subscription request
        wss_stream
            .send(Message::Text(
                json!({"op":"subscribe", "data":[format!("orderbook:{}", symbol.0)]}).to_string(),
            ))
            .await
            .unwrap();

        wss_stream
            .for_each(|message| async {
                let message = message.unwrap().into_text().unwrap();
                match serde_json::from_str::<AevoRawMessage>(&message) {
                    Ok(msg) => channel.send(msg.data).await.unwrap(),
                    Err(_) => tracing::debug!("received unknown message {:?}", message),
                }
            })
            .await;
    }
}

// Ignore unused variables for these two structs

#[derive(Deserialize, Debug)]
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
