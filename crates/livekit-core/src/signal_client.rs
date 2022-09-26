use futures_util::{SinkExt, StreamExt};
use futures_util::stream::{SplitSink, SplitStream};
use log::{error, info};
use prost::Message as ProstMessage;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::{
    Error as WsError,
    Message, protocol::frame::{CloseFrame, coding::CloseCode},
};

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
    response_chn: oneshot::Sender<SignalResult<signal_response::Message>>,
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

pub async fn connect(url: &str, token: &str) -> SignalResult<SignalClient> {
    let mut lk_url = url::Url::parse(url)?;
    lk_url.set_path("/rtc");
    lk_url
        .query_pairs_mut()
        .append_pair("access_token", token)
        .append_pair("protocol", PROTOCOL_VERSION.to_string().as_str());

    let (ws_stream, _) = connect_async(lk_url).await?;
    let (ws_writer, ws_reader) = ws_stream.split();

    let (read_tx, read_rx) = mpsc::channel::<RecvMessage>(1);
    let (write_tx, write_rx) = mpsc::channel::<SendMessage>(1);
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

    pub async fn recv(&self) -> SignalResult<signal_response::Message> {
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

    async fn ws_write(
        mut write_receiver: mpsc::Receiver<SendMessage>,
        mut ws_writer: SplitSink<WebSocket, Message>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(msg) = write_receiver.recv() => {
                    let req = SignalRequest {
                        message: Some(msg.signal),
                    };

                    let write_res = ws_writer.send(Message::Binary(req.encode_to_vec())).await;
                    if let Err(err) = write_res {
                        error!("failed to send message to ws: {:?}", err);
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

    async fn ws_read(
        mut write_receiver: mpsc::Receiver<RecvMessage>,
        mut ws_reader: SplitStream<WebSocket>,
        mut shutdown_receiver: oneshot::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                Some(msg) = write_receiver.recv() => {
                    let read = ws_reader.next().await;
                    if read.is_none() {
                        let _ = msg.response_chn.send(Err(SignalError::WsError(WsError::ConnectionClosed)));
                        break;
                    }
                    let read = read.unwrap();
                    match read {
                        Ok(Message::Binary(data)) => {
                            let res = SignalResponse::decode(data.as_slice()).expect("failed to decode incoming SignalResponse");

                            // TODO(theomonnon) Handle Message::Pong
                            let res = res.message.unwrap();
                            let _ = msg.response_chn.send(Ok(res));
                        }
                        _ => {
                            error!("unhandled websocket message: {:?}", read);
                            let _ = msg.response_chn.send(Err(SignalError::WsError(WsError::ConnectionClosed)));
                            break;
                        }
                    }
                },
                _ = (&mut shutdown_receiver) => break
            }
        }
    }
}

#[tokio::test]
async fn test_test() {
    env_logger::init();
    let client = connect("ws://localhost:7880", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE2NzEyMzk4NjAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ0ZXN0IiwibmJmIjoxNjY0MDM5ODYwLCJzdWIiOiJ0ZXN0IiwidmlkZW8iOnsicm9vbUFkbWluIjp0cnVlLCJyb29tQ3JlYXRlIjp0cnVlLCJyb29tSm9pbiI6dHJ1ZX19.0Bee2jI2cSZveAbZ8MLc-ADoMYQ4l8IRxcAxpXAS6a8").await.unwrap();
    let msg = client.recv().await.unwrap();

    client.close().await;
    info!("Received message {:?}", msg);
}
