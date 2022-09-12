use futures_util::SinkExt;
use futures_util::StreamExt;
use log::{error, info};
use prost::Message as ProtoMessage;
use std::borrow::Borrow;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::{proto, proto::signal_response};

#[derive(Error, Debug)]
pub enum SignalClientError {
    #[error("websocket failure")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("failed to parse the url")]
    UrlParse(#[from] url::ParseError),
    #[error("failed to parse messages from server")]
    ProtoParse(#[from] prost::DecodeError),
}

pub struct SignalClient {
    ws_handle: Option<JoinHandle<Result<(), SignalClientError>>>,
    response_tx: broadcast::Sender<signal_response::Message>,
    pub response_rx: broadcast::Receiver<signal_response::Message>,
}

impl SignalClient {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(16);

        Self {
            response_tx: tx,
            response_rx: rx,
            ws_handle: None,
        }
    }

    pub async fn connect(&mut self, url: &str, token: &str) -> Result<(), SignalClientError> {
        let mut lk_url = url::Url::parse(url)?;
        lk_url.set_path("/rtc");
        lk_url
            .query_pairs_mut()
            .append_pair("access_token", token)
            .append_pair("protocol", "8");

        info!("Connecting to {}", lk_url);
        let (ws, _) = connect_async(&lk_url).await?;

        self.ws_handle = Some(tokio::spawn(Self::handle_ws(ws, self.response_tx.clone())));
        Ok(())
    }

    pub async fn disconnect() {
        unimplemented!()
    }

    async fn handle_ws(
        mut ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
        response_tx: broadcast::Sender<signal_response::Message>,
    ) -> Result<(), SignalClientError> {
        loop {
            tokio::select! {
                next_msg = ws.next() => {
                    let ws_msg = match next_msg {
                        Some(msg) => msg?,
                        None => break,
                    };

                    let data = match ws_msg {
                        Message::Binary(data) => data,
                        Message::Ping(data) => {
                            ws.send(Message::Pong(data)).await?;
                            continue
                        },
                        Message::Close(_frame) => break,
                        _ => continue,
                    };

                    let proto_msg = proto::SignalResponse::decode(data.borrow())?;
                    let signal_response = proto_msg.message.unwrap();

                    match signal_response {
                        signal_response::Message::Pong(ts) => {

                        },
                        _ => {
                            response_tx.send(signal_response).unwrap();
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
