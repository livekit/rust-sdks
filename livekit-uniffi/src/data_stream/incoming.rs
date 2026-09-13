// Copyright 2026 LiveKit, Inc.
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

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use livekit_data_stream::{api as ds_api, backend as ds};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;
use tokio_util::sync::{CancellationToken, DropGuard};

use super::common::{
    decode_data_packet, ByteStreamInfo, DataStreamError, EncryptionType, TextStreamInfo,
};
use ds_api::StreamReader as _;

/// Receives inbound data-stream packets and processes them on the incoming manager's actor loop,
/// surfacing opened readers through a foreign delegate.
///
/// Mirrors [`crate::data_track::remote::RemoteDataTrackManager`]: `handle_packet_received` is a
/// cheap synchronous enqueue (safe to call from a native data-channel callback), while
/// decompression and reassembly happen on the spawned `run` task in packet order.
#[derive(uniffi::Object)]
pub struct IncomingDataStreamManager {
    input: ds::incoming::ManagerInput,
    _guard: DropGuard,
}

/// Delegate for receiving output events from [`IncomingDataStreamManager`].
///
/// Only stream lifecycle events (opened/closed) are surfaced. The manager's deprecated v1 raw
/// chunk/trailer notifications are intentionally not forwarded over the FFI boundary.
#[uniffi::export(with_foreign)]
pub trait IncomingDataStreamManagerDelegate: Send + Sync {
    /// A byte stream was opened by `identity` and is ready to be read.
    fn on_byte_stream_opened(&self, reader: Arc<ByteStreamReader>, identity: String);

    /// A text stream was opened by `identity` and is ready to be read.
    fn on_text_stream_opened(&self, reader: Arc<TextStreamReader>, identity: String);

    /// A previously opened stream terminated on the wire and will produce no further data: its
    /// trailer arrived, its (single-packet) inline payload completed, it failed, or it was
    /// aborted. Emitted exactly once per opened stream, after the corresponding open event.
    ///
    /// Hosts delivering streams on ordered topics use this to know when a stream's handler chain
    /// can advance — a stream that is still open must not block streams opened after it forever.
    fn on_stream_closed(&self, stream_id: String, identity: String);
}

#[uniffi::export(async_runtime = "tokio")]
impl IncomingDataStreamManager {
    /// Creates a manager that surfaces opened streams to `delegate`.
    ///
    /// `max_payload_byte_length` caps the decompressed size of a single incoming stream
    /// (`None` = default, 5 GB) and is **fixed for the lifetime of the manager**. If the host
    /// sources it from per-connection options that aren't final until connect — and can differ
    /// between sessions of the same host object — construct a fresh manager for each session
    /// rather than lazily memoizing one, or the first session's value is silently pinned. (Same
    /// class of rough edge as a single room instance being `connect()`ed multiple times.)
    /// Rebuilding is cheap and safe: dropping the manager cancels its tasks, and handler wiring
    /// lives on the foreign side.
    #[uniffi::constructor]
    pub fn new(
        delegate: Arc<dyn IncomingDataStreamManagerDelegate>,
        max_payload_byte_length: Option<u64>,
    ) -> Arc<Self> {
        let token = CancellationToken::new();
        let (manager, input, output) =
            ds::incoming::Manager::new(max_payload_byte_length.map(|n| n as usize));

        let rt = crate::runtime::runtime();
        rt.spawn(shutdown_forward_task(input.clone(), token.clone()));
        let delegate_forward = DelegateForwardTask { output, delegate, token: token.clone() };
        rt.spawn(delegate_forward.run());
        rt.spawn(manager.run());

        Self { input, _guard: token.drop_guard() }.into()
    }

    /// Handles an encoded [`livekit_protocol::DataPacket`] received over the data channel.
    ///
    /// Fire-and-forget: the packet is decoded and enqueued in order; processing happens on the
    /// manager's run loop. Non-data-stream or undecodable packets are ignored.
    ///
    /// `encryption_type` is how this packet arrived on the wire, and must be passed by the host
    /// because it cannot be recovered from the bytes: `encrypted_packet` is a member of the
    /// `DataPacket.value` oneof, so decrypting replaces it with the decrypted stream packet.
    /// Hosts without end-to-end encryption pass [`EncryptionType::None`]; hosts with E2EE pass
    /// the type they decrypted with, letting the manager reject chunks whose encryption doesn't
    /// match their stream's header ([`DataStreamError::EncryptionTypeMismatch`]).
    pub fn handle_packet_received(&self, packet: Bytes, encryption_type: EncryptionType) {
        if let Some(event) = decode_data_packet(&packet, encryption_type.into()) {
            let _ = self.input.send(event.into());
        }
    }

    /// Aborts all open incoming streams so their readers error instead of hanging (e.g. on
    /// disconnect). Handler wiring on the foreign side survives, so streams that arrive later
    /// (e.g. after a reconnect) are still processed.
    pub fn abort_all_streams(&self) {
        let _ = self.input.send(ds::incoming::InputEvent::AbortAllStreams);
    }

    /// Aborts open incoming streams sent by `identity` (e.g. when that participant disconnects
    /// mid-send), so their readers error instead of hanging.
    pub fn abort_streams_from(&self, identity: String) {
        let _ = self.input.send(ds::incoming::InputEvent::AbortStreamsFrom(identity.into()));
    }

    /// Number of currently open incoming streams: streams announced by a header that are still
    /// awaiting more packets. Inline (single-packet) streams complete immediately and are never
    /// counted.
    ///
    /// The query runs on the manager's loop in order with previously submitted events, so a
    /// packet or abort passed beforehand is reflected in the answer — useful in tests to wait for
    /// a header to register (or an abort to land) without racing the run loop.
    pub async fn open_stream_count(&self) -> u64 {
        let (respond_to, response) = tokio::sync::oneshot::channel();
        if self.input.send(ds::incoming::InputEvent::QueryOpenStreamCount(respond_to)).is_err() {
            return 0;
        }
        response.await.unwrap_or(0) as u64
    }
}

/// Reader for an incoming byte data stream.
#[derive(uniffi::Object)]
pub struct ByteStreamReader {
    info: ByteStreamInfo,
    inner: Mutex<ds_api::ByteStreamReader>,
}

impl ByteStreamReader {
    fn new(reader: ds_api::ByteStreamReader) -> Self {
        Self { info: reader.info().clone().into(), inner: Mutex::new(reader) }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ByteStreamReader {
    /// Information about the underlying stream.
    pub fn info(&self) -> ByteStreamInfo {
        self.info.clone()
    }

    /// Returns the next chunk, or `None` once the stream has closed.
    pub async fn next(&self) -> Result<Option<Bytes>, DataStreamError> {
        Ok(self.inner.lock().await.next().await.transpose()?)
    }

    /// Reads every chunk, concatenating them into a single buffer returned once the stream closes.
    pub async fn read_all(&self) -> Result<Bytes, DataStreamError> {
        let mut reader = self.inner.lock().await;
        let mut buffer = BytesMut::new();
        while let Some(chunk) = reader.next().await {
            buffer.extend_from_slice(&chunk?);
        }
        Ok(buffer.freeze())
    }
}

/// Reader for an incoming text data stream.
#[derive(uniffi::Object)]
pub struct TextStreamReader {
    info: TextStreamInfo,
    inner: Mutex<ds_api::TextStreamReader>,
}

impl TextStreamReader {
    fn new(reader: ds_api::TextStreamReader) -> Self {
        Self { info: reader.info().clone().into(), inner: Mutex::new(reader) }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl TextStreamReader {
    /// Information about the underlying stream.
    pub fn info(&self) -> TextStreamInfo {
        self.info.clone()
    }

    /// Returns the next chunk, or `None` once the stream has closed.
    pub async fn next(&self) -> Result<Option<String>, DataStreamError> {
        Ok(self.inner.lock().await.next().await.transpose()?)
    }

    /// Reads every chunk, concatenating them into a single string returned once the stream closes.
    pub async fn read_all(&self) -> Result<String, DataStreamError> {
        let mut reader = self.inner.lock().await;
        let mut result = String::new();
        while let Some(chunk) = reader.next().await {
            result.push_str(&chunk?);
        }
        Ok(result)
    }
}

/// Forwards manager output events to the foreign [`IncomingDataStreamManagerDelegate`].
struct DelegateForwardTask {
    output: UnboundedReceiver<ds::incoming::OutputEvent>,
    delegate: Arc<dyn IncomingDataStreamManagerDelegate>,
    token: CancellationToken,
}

impl DelegateForwardTask {
    async fn run(mut self) {
        loop {
            tokio::select! {
                _ = self.token.cancelled() => break,
                event = self.output.recv() => match event {
                    Some(event) => self.forward_event(event),
                    None => break,
                }
            }
        }
    }

    fn forward_event(&self, event: ds::incoming::OutputEvent) {
        match event {
            ds::incoming::OutputEvent::StreamOpened(ds::incoming::StreamOpened {
                stream_reader,
                participant_identity,
            }) => {
                let identity = participant_identity.to_string();
                match stream_reader {
                    ds_api::AnyStreamReader::Byte(reader) => {
                        let reader = Arc::new(ByteStreamReader::new(reader));
                        self.delegate.on_byte_stream_opened(reader, identity);
                    }
                    ds_api::AnyStreamReader::Text(reader) => {
                        let reader = Arc::new(TextStreamReader::new(reader));
                        self.delegate.on_text_stream_opened(reader, identity);
                    }
                }
            }
            ds::incoming::OutputEvent::StreamClosed(ds::incoming::StreamClosed {
                stream_id,
                participant_identity,
                topic: _,
            }) => {
                self.delegate
                    .on_stream_closed(stream_id.to_string(), participant_identity.to_string());
            }
            // Deprecated v1 raw chunk/trailer notifications are not surfaced over the FFI boundary.
            ds::incoming::OutputEvent::ChunkReceived(_)
            | ds::incoming::OutputEvent::TrailerReceived(_) => {}
        }
    }
}

async fn shutdown_forward_task(input: ds::incoming::ManagerInput, token: CancellationToken) {
    token.cancelled().await;
    let _ = input.send(ds::incoming::InputEvent::Shutdown);
}
