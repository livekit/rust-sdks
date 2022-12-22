use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prost::Message as ProstMessage;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::error::ProtocolError;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{event, Level};

use crate::proto::{signal_request, SignalRequest, SignalResponse};
use crate::signal_client::{SignalEmitter, SignalEvent, SignalOptions, SignalResult};

pub const PROTOCOL_VERSION: u32 = 8;

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
enum InternalMessage {
    Signal {
        signal: signal_request::Message,
        response_chn: oneshot::Sender<SignalResult<()>>,
    },
    Pong {
        ping_data: Vec<u8>,
    },
    Close {
        close_frame: Option<CloseFrame<'static>>,
    },
}

/// SignalStream hold the WebSocket connection
///
/// It is replaced by [SignalClient] at each reconnection.
#[derive(Debug)]
pub(crate) struct SignalStream {
    internal_tx: mpsc::Sender<InternalMessage>,
    read_handle: JoinHandle<()>,
    write_handle: JoinHandle<()>,
}

impl SignalStream {
    /// Connect to livekit websocket.
    /// Return SignalError if the connections failed
    ///
    /// SignalStream will never try to reconnect if the connection has been
    /// closed.
    pub(super) async fn connect(
        url: &str,
        token: &str,
        options: SignalOptions,
        emitter: SignalEmitter,
    ) -> SignalResult<Self> {
        let mut lk_url = url::Url::parse(url)?;
        lk_url.set_path("/rtc");
        lk_url
            .query_pairs_mut()
            .append_pair("access_token", token)
            .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str())
            .append_pair("reconnect", if options.reconnect { "1" } else { "0" })
            .append_pair("sid", &options.sid)
            .append_pair(
                "auto_subscribe",
                if options.auto_subscribe { "1" } else { "0" },
            )
            .append_pair(
                "adaptive_stream",
                if options.adaptive_stream { "1" } else { "0" },
            );

        event!(Level::INFO, "connecting to SignalClient: {}", lk_url);
        let (ws_stream, _) = connect_async(lk_url).await?;
        let _ = emitter.send(SignalEvent::Open).await;

        let (ws_writer, ws_reader) = ws_stream.split();
        let (internal_tx, internal_rx) = mpsc::channel::<InternalMessage>(8);

        let write_handle = tokio::spawn(Self::write_task(internal_rx, ws_writer, emitter.clone()));
        let read_handle = tokio::spawn(Self::read_task(internal_tx.clone(), ws_reader, emitter));

        Ok(Self {
            internal_tx,
            read_handle,
            write_handle,
        })
    }

    /// Close the websocket
    /// It sends a CloseFrame to the server before closing
    pub async fn close(self) {
        let _ = self
            .internal_tx
            .send(InternalMessage::Close {
                close_frame: Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "disconnected by client".into(),
                }),
            })
            .await;

        let _ = self.write_handle.await;
        let _ = self.read_handle.await;
    }

    /// Send a SignalRequest to the websocket
    /// It also waits for the message to be sent
    pub async fn send(&self, signal: signal_request::Message) -> SignalResult<()> {
        let (send, recv) = oneshot::channel();
        let msg = InternalMessage::Signal {
            signal,
            response_chn: send,
        };
        let _ = self.internal_tx.send(msg).await;
        recv.await.expect("channel closed")
    }

    /// This task is used to send messages to the websocket
    /// It is also responsible for closing the connection
    async fn write_task(
        mut internal_rx: mpsc::Receiver<InternalMessage>,
        mut ws_writer: SplitSink<WebSocket, Message>,
        emitter: SignalEmitter,
    ) {
        while let Some(msg) = internal_rx.recv().await {
            match msg {
                InternalMessage::Signal {
                    signal,
                    response_chn,
                } => {
                    event!(Level::TRACE, "sending SignalRequest: {:?}", signal);

                    let data = Message::Binary(
                        SignalRequest {
                            message: Some(signal),
                        }
                        .encode_to_vec(),
                    );

                    if let Err(err) = ws_writer.send(data).await {
                        event!(Level::ERROR, "failed to send signal: {:?}", err);
                        let _ = response_chn.send(Err(err.into()));
                        break;
                    }

                    let _ = response_chn.send(Ok(()));
                }
                InternalMessage::Pong { ping_data } => {
                    if let Err(err) = ws_writer.send(Message::Pong(ping_data)).await {
                        event!(Level::ERROR, "failed to send pong message: {:?}", err);
                    }
                }
                InternalMessage::Close { close_frame } => {
                    if let Some(close_frame) = close_frame {
                        let _ = ws_writer.send(Message::Close(Some(close_frame))).await;
                        let _ = ws_writer.flush().await;
                    }
                    break;
                }
            }
        }

        let _ = ws_writer.close().await;
        let _ = emitter.send(SignalEvent::Close).await;
    }

    /// This task is used to read incoming messages from the websocket
    /// and dispatch them through the EventEmitter.
    ///
    /// It can also send messages to [handle_write] task ( Used e.g. answer to pings )
    async fn read_task(
        internal_tx: mpsc::Sender<InternalMessage>,
        mut ws_reader: SplitStream<WebSocket>,
        emitter: SignalEmitter,
    ) {
        while let Some(msg) = ws_reader.next().await {
            match msg {
                Ok(Message::Binary(data)) => {
                    let res = SignalResponse::decode(data.as_slice())
                        .expect("failed to decode SignalResponse");

                    let msg = res.message.unwrap();
                    event!(Level::TRACE, "received SignalResponse: {:?}", msg);
                    let _ = emitter.send(SignalEvent::Signal(msg)).await;
                }
                Ok(Message::Ping(data)) => {
                    let _ = internal_tx
                        .send(InternalMessage::Pong { ping_data: data })
                        .await;
                    continue;
                }
                Ok(Message::Close(close)) => {
                    event!(Level::DEBUG, "server closed the connection: {:?}", close);
                    break;
                }
                _ => {
                    event!(Level::ERROR, "unhandled websocket message {:?}", msg);
                    break;
                }
            }
        }

        let _ = internal_tx
            .send(InternalMessage::Close { close_frame: None })
            .await;
    }
}
