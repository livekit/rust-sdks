use std::fmt::{Debug, Formatter};

use futures::future::poll_fn;
use futures_util::{SinkExt, StreamExt};
use futures_util::stream::{SplitSink, SplitStream};
use prost::Message as ProstMessage;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tracing::{event, Level};

use crate::proto::{signal_request, signal_response, SignalRequest, SignalResponse};
use crate::signal_client::SendMessage::Pong;

pub const PROTOCOL_VERSION: u32 = 8;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("websocket failure")]
    WsError(#[from] WsError),
    #[error("failed to parse the url")]
    UrlParse(#[from] url::ParseError),
    #[error("failed to decode messages from server")]
    ProtoParse(#[from] prost::DecodeError),
}

type SignalResult<T> = Result<T, SignalError>;
type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
enum RecvMessage {
    Signal {
        response_chn: oneshot::Sender<Option<signal_response::Message>>,
    },
}

#[derive(Debug)]
enum SendMessage {
    Signal {
        signal: signal_request::Message,
        response_chn: oneshot::Sender<SignalResult<()>>,
    },
    Pong {
        ping_data: Vec<u8>,
    },
}

pub struct SignalClient {
    read_tx: mpsc::Sender<RecvMessage>,
    write_tx: mpsc::Sender<SendMessage>,
    read_handle: JoinHandle<()>,
    write_handle: JoinHandle<()>,
}

impl Debug for SignalClient {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "SignalClient")
    }
}

#[tracing::instrument(skip(url, token))]
pub async fn connect(url: &str, token: &str) -> SignalResult<SignalClient> {
    let mut lk_url = url::Url::parse(url)?;
    lk_url.set_path("/rtc");
    lk_url
        .query_pairs_mut()
        .append_pair("access_token", token)
        .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str());

    event!(Level::DEBUG, "connecting to websocket: {}", lk_url);
    let (ws_stream, _) = connect_async(lk_url.clone()).await?;
    event!(Level::DEBUG, "connected to SignalClient");

    let (ws_writer, ws_reader) = ws_stream.split();

    let (read_tx, read_rx) = mpsc::channel::<RecvMessage>(8);
    let (write_tx, write_rx) = mpsc::channel::<SendMessage>(8);

    let read_handle = tokio::spawn(SignalClient::ws_read(read_rx, ws_reader, write_tx.clone()));
    let write_handle = tokio::spawn(SignalClient::ws_write(write_rx, ws_writer));

    Ok(SignalClient {
        read_tx,
        write_tx,
        read_handle,
        write_handle,
    })
}

impl SignalClient {
    pub async fn close(self) {
        drop(self.read_tx);
        drop(self.write_tx);

        let _ = self.read_handle.await;
        let _ = self.write_handle.await;
    }

    pub async fn recv(&self) -> Option<signal_response::Message> {
        let (send, recv) = oneshot::channel();
        let msg = RecvMessage::Signal { response_chn: send };
        let _ = self.read_tx.send(msg).await;
        recv.await.expect("channel closed")
    }

    pub async fn send(&self, signal: signal_request::Message) -> SignalResult<()> {
        let (send, recv) = oneshot::channel();
        let msg = SendMessage::Signal {
            signal,
            response_chn: send,
        };
        let _ = self.write_tx.send(msg).await;
        recv.await.expect("channel closed")
    }

    async fn ws_write(
        mut write_receiver: mpsc::Receiver<SendMessage>,
        mut ws_writer: SplitSink<WebSocket, Message>,
    ) {
        while let Some(msg) = write_receiver.recv().await {
            match msg {
                SendMessage::Signal {
                    signal,
                    response_chn,
                } => {
                    event!(Level::TRACE, "sending: {:?}", signal);

                    let req = SignalRequest {
                        message: Some(signal),
                    };

                    let write_res = ws_writer.send(Message::Binary(req.encode_to_vec())).await;
                    if let Err(err) = write_res {
                        event!(Level::ERROR, "failed to send signal: {:?}", err);
                        let _ = response_chn.send(Err(err.into()));
                        break;
                    }

                    let _ = response_chn.send(Ok(()));
                }
                Pong { ping_data } => {
                    if let Err(err) = ws_writer.send(Message::Pong(ping_data)).await {
                        event!(Level::ERROR, "failed to send pong message: {:?}", err);
                    }
                }
            }
        }

        let _ = ws_writer
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "disconnected by client".into(),
            })))
            .await;
        let _ = ws_writer.flush().await;
    }

    async fn ws_read(
        mut read_receiver: mpsc::Receiver<RecvMessage>,
        mut ws_reader: SplitStream<WebSocket>,
        write_tx: mpsc::Sender<SendMessage>,
    ) {
        while let Some(RecvMessage::Signal { mut response_chn }) = read_receiver.recv().await {
            tokio::select! {
                read = Self::handle_msg(&mut ws_reader, &write_tx) => {
                    let _ = response_chn.send(read);
                }
                _ = poll_fn(|cx| response_chn.poll_closed(cx)) => {
                    continue; // Cancelled
                },
            }
        }
    }

    async fn handle_msg(
        ws_reader: &mut SplitStream<WebSocket>,
        write_tx: &mpsc::Sender<SendMessage>,
    ) -> Option<signal_response::Message> {
        loop {
            let read = ws_reader.next().await?;
            match read {
                Ok(Message::Binary(data)) => {
                    let res = SignalResponse::decode(data.as_slice())
                        .expect("failed to decode SignalResponse");
                    event!(Level::TRACE, "received: {:?}", res);
                    return Some(res.message.unwrap());
                }
                Ok(Message::Ping(data)) => {
                    let _ = write_tx.send(Pong { ping_data: data });
                    continue;
                }
                Ok(Message::Close(close)) => {
                    event!(Level::DEBUG, "server closed the connection: {:?}", close);
                    return None;
                }
                _ => {
                    event!(Level::ERROR, "unhandled websocket message {:?}", read);
                    return None;
                }
            }
        }
    }
}
