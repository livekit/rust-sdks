use futures::future::poll_fn;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use std::fmt::{Debug, Formatter};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{
    protocol::frame::{coding::CloseCode, CloseFrame},
    Error as WsError, Message,
};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{event, span, Level};

use crate::proto::{signal_request, signal_response, SignalRequest, SignalResponse};

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
struct RecvMessage {
    response_chn: oneshot::Sender<Option<signal_response::Message>>,
}

#[derive(Debug)]
struct SendMessage {
    signal: signal_request::Message,
    response_chn: oneshot::Sender<SignalResult<()>>,
}

pub struct SignalClient {
    read_sender: mpsc::Sender<RecvMessage>,
    write_sender: mpsc::Sender<SendMessage>,
    write_shutdown_sender: oneshot::Sender<()>,
    read_shutdown_sender: oneshot::Sender<()>,
    read_handle: JoinHandle<()>,
    write_handle: JoinHandle<()>,
}

impl Debug for SignalClient {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "SignalClient")
    }
}

#[tracing::instrument]
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
    let (read_shutdown_tx, read_shutdown_rx) = oneshot::channel();
    let (write_shutdown_tx, write_shutdown_rx) = oneshot::channel();

    let read_handle = tokio::spawn(SignalClient::ws_read(read_rx, ws_reader, read_shutdown_rx));
    let write_handle = tokio::spawn(SignalClient::ws_write(
        write_rx,
        ws_writer,
        write_shutdown_rx,
    ));

    Ok(SignalClient {
        read_sender: read_tx,
        write_sender: write_tx,
        write_shutdown_sender: write_shutdown_tx,
        read_shutdown_sender: read_shutdown_tx,
        read_handle,
        write_handle,
    })
}

impl SignalClient {
    pub async fn close(self) {
        let _ = self.write_shutdown_sender.send(());
        let _ = self.write_handle.await;
        let _ = self.read_shutdown_sender.send(());
        let _ = self.read_handle.await;
    }

    pub async fn recv(&self) -> Option<signal_response::Message> {
        let (send, recv) = oneshot::channel();
        let msg = RecvMessage { response_chn: send };
        let _ = self.read_sender.send(msg).await;
        recv.await.expect("channel closed")
    }

    pub async fn send(&self, signal: signal_request::Message) -> SignalResult<()> {
        let (send, recv) = oneshot::channel();
        let msg = SendMessage {
            signal,
            response_chn: send,
        };
        let _ = self.write_sender.send(msg).await;
        recv.await.expect("channel closed")
    }

    #[tracing::instrument]
    async fn ws_write(
        mut write_receiver: mpsc::Receiver<SendMessage>,
        mut ws_writer: SplitSink<WebSocket, Message>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(msg) = write_receiver.recv() => {
                    event!(Level::TRACE, "sending: {:?}", msg.signal);

                    let req = SignalRequest {
                        message: Some(msg.signal),
                    };

                    let write_res = ws_writer.send(Message::Binary(req.encode_to_vec())).await;
                    if let Err(err) = write_res {
                        event!(Level::ERROR, "failed to send message to ws: {:?}", err);
                        let _ = msg.response_chn.send(Err(err.into()));
                        break;
                    }

                    let _ = msg.response_chn.send(Ok(()));
                },
                _ = (&mut shutdown_receiver) => {
                    let _ = ws_writer.send(Message::Close(Some(CloseFrame {
                        code: CloseCode::Normal,
                        reason: "disconnected by client".into()
                    }))).await;
                    let _ = ws_writer.flush().await;
                    break;
                }
            }
        }
    }

    #[tracing::instrument]
    async fn ws_read(
        mut read_receiver: mpsc::Receiver<RecvMessage>,
        mut ws_reader: SplitStream<WebSocket>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(mut msg) = read_receiver.recv() => {
                    tokio::select! {
                        Some(read) = ws_reader.next() => {
                           match read {
                                Ok(Message::Binary(data)) => {
                                    let res = SignalResponse::decode(data.as_slice()).expect("failed to decode SignalResponse");
                                    let signal = res.message.unwrap();
                                    event!(Level::TRACE, "received: {:?}", signal);
                                    let _ = msg.response_chn.send(Some(signal));
                                }
                                _ => {
                                    event!(Level::ERROR, "unhandled websocket message {:?}", read);
                                    let _ = msg.response_chn.send(None);
                                }
                            }
                        },
                        _ = poll_fn(|cx| msg.response_chn.poll_closed(cx)) => {
                            continue; // Cancelled
                        },
                        else => {
                            break; // Connection closed
                        }
                    }
                },
                _ = (&mut shutdown_receiver) => break
            }
        }
    }
}
