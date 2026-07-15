// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{PlatformConnection, TransportError};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use livekit_runtime::TcpStream;
use tokio::sync::Mutex;

#[cfg(feature = "__native-tokio")]
use tokio_tungstenite::{
    tungstenite::{error::ProtocolError, Error as WsError, Message},
    MaybeTlsStream, WebSocketStream,
};
#[cfg(feature = "__native-async")]
use async_tungstenite::{
    async_std::ClientStream as MaybeTlsStream,
    tungstenite::{error::ProtocolError, Error as WsError, Message},
    WebSocketStream,
};

type WebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct NativeConnection {
    writer: Mutex<SplitSink<WebSocket, Message>>,
    reader: Mutex<SplitStream<WebSocket>>,
}

impl NativeConnection {
    pub(super) fn new(ws: WebSocket) -> Self {
        let (writer, reader) = ws.split();
        Self { writer: Mutex::new(writer), reader: Mutex::new(reader) }
    }
}

#[async_trait::async_trait]
impl PlatformConnection for NativeConnection {
    async fn send(&self, frame: Vec<u8>) -> Result<(), TransportError> {
        self.writer
            .lock()
            .await
            .send(Message::Binary(frame.into()))
            .await
            .map_err(|e| TransportError::Connection(e.to_string()))
    }

    async fn recv(&self) -> Result<Option<Vec<u8>>, TransportError> {
        let mut reader = self.reader.lock().await;
        loop {
            match reader.next().await {
                Some(Ok(Message::Binary(data))) => return Ok(Some(data.to_vec())),
                // Respond to WS ping internally; keep reading for the next app frame.
                Some(Ok(Message::Ping(payload))) => {
                    let _ = self.writer.lock().await.send(Message::Pong(payload)).await;
                }
                Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => continue,
                Some(Ok(Message::Text(_))) => continue, // signalling never sends text
                Some(Ok(Message::Close(_))) | None => return Ok(None),
                Some(Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake))) => {
                    return Ok(None)
                }
                Some(Err(e)) => return Err(TransportError::Connection(e.to_string())),
            }
        }
    }

    async fn close(&self) {
        let _ = self.writer.lock().await.close().await;
    }
}
