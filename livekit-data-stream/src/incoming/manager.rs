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

use bytes::Bytes;
use livekit_common::{EncryptionType, ParticipantIdentity};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender},
    watch,
};

use crate::{
    info::AnyStreamInfo,
    types::{Chunk, CompressionType, Header, Packet, StreamId, Trailer},
    utils::{StreamError, StreamProgress, StreamResult},
};

use super::{
    events::{
        ChunkReceived, InputEvent, OutputEvent, PacketReceived, StreamOpened, TrailerReceived,
    },
    stream_reader::AnyStreamReader,
};

struct Descriptor {
    progress: StreamProgress,
    chunk_tx: UnboundedSender<StreamResult<Bytes>>,
    /// Publishes `progress` updates to the reader's `progress()` stream.
    progress_tx: watch::Sender<StreamProgress>,
    encryption_type: EncryptionType,
    /// Identity of the participant sending this stream; used to abort the stream
    /// if that participant disconnects mid-send.
    sender_identity: ParticipantIdentity,
    is_internal: bool,
    /// Whether this is a text stream (decompressed output is reframed on UTF-8 boundaries).
    is_text: bool,
    /// Per-stream deflate-raw decompressor; `Some` if the header declared `DEFLATE_RAW`.
    decompressor: Option<DeflateDecompressState>,
    /// Highest chunk index processed so far (compressed streams; for dedup/gap detection).
    last_chunk_index: Option<u64>,
}

/// Streaming deflate-raw decompressor state for one compressed stream.
///
/// Backed by `async-compression`'s push-style (`AsyncWrite`) decoder: ordered compressed chunks
/// are written into it and the decompressed output lands in the inner `Vec`, which is drained per
/// chunk. Because the manager runs as an actor (see [`Manager::run`]), the decode is
/// awaited directly on the run-loop task — no lock is held across the `.await`, and it behaves
/// identically across every async backend the SDK supports.
struct DeflateDecompressState {
    decoder: async_compression::futures::write::DeflateDecoder<Vec<u8>>,
    /// Decompressed text bytes not yet yielded because they end mid-codepoint.
    pending_text: Vec<u8>,
}

impl DeflateDecompressState {
    fn new() -> Self {
        // The `deflate` algorithm is raw DEFLATE (no zlib header/checksum), matching the wire
        // contract.
        Self {
            decoder: async_compression::futures::write::DeflateDecoder::new(Vec::new()),
            pending_text: Vec::new(),
        }
    }

    /// Feeds compressed `input` through the stateful decompressor, returning all
    /// decompressed output produced so far.
    async fn push(&mut self, input: &[u8]) -> StreamResult<Vec<u8>> {
        use futures_util::io::AsyncWriteExt;
        self.decoder.write_all(input).await.map_err(|_| StreamError::Decompression)?;
        // Flush so all currently-decodable output lands in the inner `Vec`.
        self.decoder.flush().await.map_err(|_| StreamError::Decompression)?;
        Ok(std::mem::take(self.decoder.get_mut()))
    }

    /// Appends `decompressed` text bytes and returns the longest valid-UTF-8 prefix,
    /// retaining any trailing incomplete codepoint for the next chunk.
    fn reframe_text(&mut self, decompressed: Vec<u8>) -> Bytes {
        self.pending_text.extend_from_slice(&decompressed);
        let valid = match std::str::from_utf8(&self.pending_text) {
            Ok(_) => self.pending_text.len(),
            Err(e) => e.valid_up_to(),
        };
        Bytes::from(self.pending_text.drain(..valid).collect::<Vec<u8>>())
    }
}

/// One-shot deflate-raw decompression of a complete (inline) payload.
async fn inflate_raw(data: &[u8]) -> StreamResult<Vec<u8>> {
    use futures_util::io::AsyncReadExt;
    let mut decoder = async_compression::futures::bufread::DeflateDecoder::new(
        futures_util::io::Cursor::new(data),
    );
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).await.map_err(|_| StreamError::Decompression)?;
    Ok(out)
}

/// Cheap, cloneable, `Send + Sync` handle used to feed [`InputEvent`]s into the manager's run
/// loop.
///
/// Dropping the last handle stops the loop (via [`InputEvent::Shutdown`]).
#[derive(Clone)]
pub struct ManagerInput {
    input_tx: UnboundedSender<InputEvent>,
    _drop_guard: Arc<DropGuard>,
}

/// Sends [`InputEvent::Shutdown`] when the last [`ManagerInput`] is dropped.
struct DropGuard {
    input_tx: UnboundedSender<InputEvent>,
}

impl Drop for DropGuard {
    fn drop(&mut self) {
        let _ = self.input_tx.send(InputEvent::Shutdown);
    }
}

impl ManagerInput {
    fn new(input_tx: UnboundedSender<InputEvent>) -> Self {
        Self { input_tx: input_tx.clone(), _drop_guard: Arc::new(DropGuard { input_tx }) }
    }

    /// Feeds an event to the manager's run loop. Fails only if the loop has already stopped.
    pub fn send(&self, event: InputEvent) -> StreamResult<()> {
        self.input_tx.send(event).map_err(|_| StreamError::Internal)
    }
}

/// Actor that owns all incoming-stream state and processes [`InputEvent`]s on a single task
/// (see [`Self::run`]). Because it owns its state directly (no shared `Mutex`), its handlers can
/// `.await` decompression on the run-loop task.
pub struct Manager {
    inner: ManagerInner,
    input_rx: UnboundedReceiver<InputEvent>,
    output_tx: UnboundedSender<OutputEvent>,

    /// Topics whose streams are handled internally by the SDK (e.g. RPC) and never surfaced as
    /// application events. Supplied by the host crate so this crate stays decoupled from RPC.
    reserved_topics: Vec<&'static str>,
}

#[derive(Default)]
struct ManagerInner {
    open_streams: HashMap<StreamId, Descriptor>,
}

impl Manager {
    pub fn new(
        reserved_topics: Vec<&'static str>,
    ) -> (Self, ManagerInput, UnboundedReceiver<OutputEvent>) {
        // Unbounded: inbound wire packets must never be dropped (a dropped chunk is an
        // unrecoverable `MissedChunk`) and must not head-of-line-block the engine event loop.
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        let manager = Self { inner: ManagerInner::default(), reserved_topics, input_rx, output_tx };
        (manager, ManagerInput::new(input_tx), output_rx)
    }

    /// Runs the manager's event loop until the input channel closes (all
    /// [`ManagerInput`]s dropped) or [`InputEvent::Shutdown`] is received. On exit,
    /// dropping `self` closes every open reader.
    pub async fn run(mut self) {
        while let Some(event) = self.input_rx.recv().await {
            match event {
                InputEvent::PacketReceived(PacketReceived { packet, participant_identity }) => {
                    match packet {
                        Packet::Header { header, encryption_type } => {
                            self.on_header(header, participant_identity, encryption_type).await
                        }
                        Packet::Chunk { chunk, encryption_type } => {
                            self.on_chunk(chunk, participant_identity, encryption_type).await
                        }
                        Packet::Trailer(trailer) => self.on_trailer(trailer, participant_identity),
                    }
                }
                InputEvent::AbortStreamsFrom(identity) => self.on_abort(identity),
                InputEvent::Shutdown => break,
            }
        }
    }

    /// Handles an incoming header packet.
    async fn on_header(
        &mut self,
        mut header: Header,
        participant_identity: ParticipantIdentity,
        encryption_type: EncryptionType,
    ) {
        let is_internal = self.is_internal_topic(&header.topic);
        // Read the v2 signals before `try_from_with_encryption` consumes the header.
        let inline_content = header.inline_content.take();
        let is_compressed = header.compression == CompressionType::DeflateRaw;

        let Ok(info) = AnyStreamInfo::try_from_with_encryption(header, encryption_type)
            .inspect_err(|e| log::error!("Invalid header: {}", e))
        else {
            return;
        };

        let id: StreamId = info.id().into();
        let is_text = matches!(info, AnyStreamInfo::Text(_));
        let bytes_total = info.total_length();
        let stream_encryption_type = info.encryption_type();

        if self.inner.open_streams.contains_key(&id) {
            log::error!("Stream '{}' already open", id);
            return;
        }

        let (stream_reader, chunk_tx, progress_tx) = AnyStreamReader::from(info);
        let _ = self.output_tx.send(
            StreamOpened { stream_reader, participant_identity: participant_identity.clone() }
                .into(),
        );

        // Inline single-packet stream: synthesize the complete content now; no chunk/trailer
        // packets will follow, so we never register an open descriptor.
        if let Some(content) = inline_content {
            let content = if is_compressed {
                match inflate_raw(&content).await {
                    Ok(decompressed) => decompressed,
                    Err(error) => {
                        // Defensive: a conforming sender never sends a compressed stream we
                        // can't read, but drop gracefully if it happens.
                        let _ = chunk_tx.send(Err(error));
                        return;
                    }
                }
            } else {
                content
            };
            // The whole payload arrives at once, so publish a single completed progress update.
            let _ = progress_tx.send(StreamProgress {
                chunk_index: 0,
                bytes_processed: content.len() as u64,
                bytes_total,
            });
            // The full payload is complete and (for text) valid UTF-8, so deliver it as one chunk.
            if !content.is_empty() {
                let _ = chunk_tx.send(Ok(Bytes::from(content)));
            }
            // Dropping `chunk_tx` closes the reader.
            return;
        }

        let descriptor = Descriptor {
            progress: StreamProgress { bytes_total, ..Default::default() },
            chunk_tx,
            progress_tx,
            encryption_type: stream_encryption_type,
            sender_identity: participant_identity,
            is_internal,
            is_text,
            decompressor: is_compressed.then(DeflateDecompressState::new),
            last_chunk_index: None,
        };
        self.inner.open_streams.insert(id, descriptor);
    }

    /// Returns whether a given streams is handled internally by the SDK
    /// (e.g. `lk.rpc_request`) and associated events should not be surfaced to the application.
    fn is_internal(&self, id: &StreamId) -> bool {
        self.inner.open_streams.get(id).is_some_and(|d| d.is_internal)
    }

    /// Returns whether streams created on the given topic are handled internally by the SDK
    /// (e.g. `lk.rpc_request`) and should not be surfaced to the application.
    ///
    /// When possible, prefer [`Self::is_internal`] instead.
    fn is_internal_topic(&self, topic: &str) -> bool {
        self.reserved_topics.iter().any(|t| t == &topic)
    }

    /// Handles an incoming chunk packet.
    async fn on_chunk(
        &mut self,
        chunk: Chunk,
        participant_identity: ParticipantIdentity,
        encryption_type: EncryptionType,
    ) {
        let id = chunk.stream_id.clone();
        if !self.is_internal(&id) {
            let _ = self.output_tx.send(OutputEvent::ChunkReceived(ChunkReceived {
                chunk: chunk.clone(),
                participant_identity,
            }));
        }

        let inner = &mut self.inner;
        let Some(descriptor) = inner.open_streams.get_mut(&id) else {
            return;
        };

        if descriptor.encryption_type != encryption_type.into() {
            inner.close_stream_with_error(&id, StreamError::EncryptionTypeMismatch);
            return;
        }

        if let Some(decompressor) = &mut descriptor.decompressor {
            // --- Compressed stream: feed chunks through one stateful decompressor. ---
            // Duplicate index (reconnect replay): drop with a warning.
            if let Some(last) = descriptor.last_chunk_index {
                if chunk.chunk_index <= last {
                    log::warn!(
                        "Dropping duplicate chunk {} for compressed stream '{}'",
                        chunk.chunk_index,
                        id
                    );
                    return;
                }
            }
            // A gap is unrecoverable for a stateful decompressor.
            let expected = descriptor.last_chunk_index.map(|i| i + 1).unwrap_or(0);
            if chunk.chunk_index != expected {
                inner.close_stream_with_error(&id, StreamError::MissedChunk);
                return;
            }
            descriptor.last_chunk_index = Some(chunk.chunk_index);

            // Confine the decompressor borrow so we can re-borrow `inner` afterwards.
            let result: StreamResult<(u64, Bytes)> = {
                match decompressor.push(&chunk.content).await {
                    Ok(decompressed) => {
                        let uncompressed_byte_count = decompressed.len() as u64;
                        let yielded = if descriptor.is_text {
                            decompressor.reframe_text(decompressed)
                        } else {
                            Bytes::from(decompressed)
                        };
                        Ok((uncompressed_byte_count, yielded))
                    }
                    Err(error) => Err(error),
                }
            };

            let (uncompressed_byte_count, to_yield) = match result {
                Ok(value) => value,
                Err(error) => {
                    inner.close_stream_with_error(&id, error);
                    return;
                }
            };

            // Count decompressed bytes against the (uncompressed) total length.
            descriptor.progress.bytes_processed += uncompressed_byte_count;
            if let Some(total) = descriptor.progress.bytes_total {
                if descriptor.progress.bytes_processed > total {
                    inner.close_stream_with_error(&id, StreamError::LengthExceeded);
                    return;
                }
            }
            if !to_yield.is_empty() {
                inner.yield_chunk(&id, to_yield);
            }
            inner.publish_progress(&id);
            return;
        }

        // --- Uncompressed (v1) stream: contiguous chunks, content delivered as-is. ---
        if descriptor.progress.chunk_index != chunk.chunk_index {
            inner.close_stream_with_error(&id, StreamError::MissedChunk);
            return;
        }

        descriptor.progress.chunk_index += 1;
        descriptor.progress.bytes_processed += chunk.content.len() as u64;

        if match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed > total,
            None => false,
        } {
            inner.close_stream_with_error(&id, StreamError::LengthExceeded);
            return;
        }
        inner.yield_chunk(&id, Bytes::from(chunk.content));
        inner.publish_progress(&id);
    }

    /// Handles an incoming trailer packet.
    fn on_trailer(&mut self, trailer: Trailer, participant_identity: ParticipantIdentity) {
        let id = trailer.stream_id.clone();
        if !self.is_internal(&id) {
            let _ = self
                .output_tx
                .send(TrailerReceived { trailer: trailer.clone(), participant_identity }.into());
        }

        let inner = &mut self.inner;
        let Some(descriptor) = inner.open_streams.get_mut(&id) else {
            return;
        };

        if !match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed >= total,
            None => true,
        } {
            inner.close_stream_with_error(&id, StreamError::Incomplete);
            return;
        }
        if !trailer.reason.is_empty() {
            inner.close_stream_with_error(&id, StreamError::AbnormalEnd(trailer.reason));
            return;
        }
        inner.close_stream(&id);
    }

    /// Aborts every open stream being sent by the given participant, erroring each
    /// reader with [`StreamError::AbnormalEnd`].
    ///
    /// Called when a remote participant disconnects: any streams it had in flight to
    /// this receiver are terminated so their readers observe an error rather than
    /// hanging forever waiting for chunks that will never arrive.
    fn on_abort(&mut self, identity: ParticipantIdentity) {
        let inner = &mut self.inner;
        let ids: Vec<StreamId> = inner
            .open_streams
            .iter()
            .filter(|(_, descriptor)| descriptor.sender_identity == identity)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let reason = format!(
                "Participant {} unexpectedly disconnected in the middle of sending data",
                identity
            );
            inner.close_stream_with_error(&id, StreamError::AbnormalEnd(reason));
        }
    }
}

impl ManagerInner {
    fn yield_chunk(&mut self, id: &StreamId, chunk: Bytes) {
        let Some(descriptor) = self.open_streams.get_mut(id) else {
            return;
        };
        if descriptor.chunk_tx.send(Ok(chunk)).is_err() {
            // Reader has been dropped, close the stream.
            self.close_stream(id);
        }
    }

    /// Publishes the descriptor's current progress to the reader's `progress()` stream.
    fn publish_progress(&self, id: &StreamId) {
        if let Some(descriptor) = self.open_streams.get(id) {
            // `StreamProgress` is `Copy`; a send error just means the reader was dropped, which the
            // chunk channel already handles, so ignore it.
            let _ = descriptor.progress_tx.send(descriptor.progress);
        }
    }

    fn close_stream(&mut self, id: &StreamId) {
        // Dropping the sender closes the channel.
        self.open_streams.remove(id);
    }

    fn close_stream_with_error(&mut self, id: &StreamId, error: StreamError) {
        if let Some(descriptor) = self.open_streams.remove(id) {
            let _ = descriptor.chunk_tx.send(Err(error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        incoming::StreamReader,
        info::TextStreamInfo,
        test_utils::pseudo_random_text,
        types::{ByteHeader, StreamId, TextHeader},
    };
    use futures_util::{io::AsyncReadExt, Stream};
    use std::collections::HashMap;

    const SENDER: &str = "alice";

    async fn deflate_raw(data: &[u8]) -> Vec<u8> {
        let mut encoder = async_compression::futures::bufread::DeflateEncoder::new(
            futures_util::io::Cursor::new(data),
        );
        let mut out = Vec::new();
        encoder.read_to_end(&mut out).await.expect("DeflateEncoder::read_to_end failed");
        out
    }

    fn attrs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn text_header(
        id: &str,
        total_length: Option<u64>,
        attributes: HashMap<String, String>,
        inline_content: Option<Vec<u8>>,
        compression: CompressionType,
    ) -> Header {
        Header {
            stream_id: StreamId::from(id),
            timestamp: 0,
            topic: "topic".to_string(),
            mime_type: "text/plain".to_string(),
            total_length,
            attributes,
            content_header: Some(TextHeader::default().into()),
            inline_content,
            compression,
        }
    }

    fn byte_header(
        id: &str,
        total_length: Option<u64>,
        inline_content: Option<Vec<u8>>,
        compression: CompressionType,
    ) -> Header {
        Header {
            stream_id: StreamId::from(id),
            timestamp: 0,
            topic: "topic".to_string(),
            mime_type: "application/octet-stream".to_string(),
            total_length,
            attributes: HashMap::new(),
            content_header: Some(ByteHeader { name: "file".to_string() }.into()),
            inline_content,
            compression,
        }
    }

    fn chunk(id: &str, index: u64, content: Vec<u8>) -> Chunk {
        Chunk { stream_id: StreamId::from(id), chunk_index: index, content, ..Default::default() }
    }

    fn trailer(id: &str) -> Trailer {
        Trailer { stream_id: StreamId::from(id), ..Default::default() }
    }

    fn trailer_with_attrs(id: &str, attributes: HashMap<String, String>) -> Trailer {
        Trailer { stream_id: StreamId::from(id), reason: String::new(), attributes }
    }

    async fn read_text(reader: AnyStreamReader) -> StreamResult<String> {
        match reader {
            AnyStreamReader::Text(r) => r.read_all().await,
            _ => panic!("expected a text reader"),
        }
    }

    async fn read_bytes(reader: AnyStreamReader) -> StreamResult<Bytes> {
        match reader {
            AnyStreamReader::Byte(r) => r.read_all().await,
            _ => panic!("expected a byte reader"),
        }
    }

    fn text_info(reader: &AnyStreamReader) -> &TextStreamInfo {
        match reader {
            AnyStreamReader::Text(r) => r.info(),
            _ => panic!("expected a text reader"),
        }
    }

    /// Drives an [`Manager`] actor for tests: spawns its `run` loop, exposes
    /// `send_*` helpers to feed events, and `next_opened` to await the reader for a new stream.
    struct Harness {
        input: ManagerInput,
        output_rx: UnboundedReceiver<OutputEvent>,
    }

    impl Harness {
        fn new(reserved_topics: Vec<&'static str>) -> Self {
            let (manager, input, output_rx) = Manager::new(reserved_topics);
            tokio::spawn(manager.run());
            Self { input, output_rx }
        }

        fn send_packet(&self, packet: Packet) {
            self.send_packet_from(packet, SENDER);
        }

        fn send_packet_from(&self, packet: Packet, identity: &str) {
            let event = InputEvent::PacketReceived(PacketReceived {
                packet,
                participant_identity: ParticipantIdentity::from(identity),
            });
            self.input.send(event).expect("Harness::send_packet failed");
        }

        fn abort(&self, identity: ParticipantIdentity) {
            self.input.send(InputEvent::AbortStreamsFrom(identity)).unwrap();
        }

        /// Awaits the next opened stream's reader (skipping back-compat chunk/trailer outputs).
        async fn next_opened(&mut self) -> (AnyStreamReader, ParticipantIdentity) {
            loop {
                match self.output_rx.recv().await.expect("a stream should be opened") {
                    OutputEvent::StreamOpened(StreamOpened {
                        stream_reader,
                        participant_identity,
                    }) => {
                        return (stream_reader, participant_identity);
                    }
                    _ => continue,
                }
            }
        }
    }

    // --- v1 (legacy multi-packet) --------------------------------------------------------

    #[tokio::test]
    async fn v1_text_stream_round_trips() {
        let mut h = Harness::new(vec![]);
        let text = "hello world";
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                None,
                CompressionType::None,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, identity) = h.next_opened().await;
        assert_eq!(identity.as_str(), SENDER);
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, text.as_bytes().to_vec()),
            encryption_type: EncryptionType::None,
        });
        h.send_packet(Packet::Trailer(trailer("s1")));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v1_byte_stream_round_trips() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: byte_header("s1", Some(4), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, vec![1, 2, 3, 4]),
            encryption_type: EncryptionType::None,
        });
        h.send_packet(Packet::Trailer(trailer("s1")));
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(vec![1u8, 2, 3, 4]));
    }

    #[tokio::test]
    async fn v1_merges_trailer_attributes() {
        let mut h = Harness::new(vec![]);
        let text = "hi";
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar"), ("baz", "quux")]),
                None,
                CompressionType::None,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, text.as_bytes().to_vec()),
            encryption_type: EncryptionType::None,
        });
        h.send_packet(Packet::Trailer(trailer_with_attrs(
            "s1",
            attrs(&[("hello", "world"), ("foo", "updated")]),
        )));
        // NOTE: trailer-attribute merging is asserted via the reader info after close.
        let info_attrs = text_info(&reader).attributes.clone();
        assert_eq!(read_text(reader).await.unwrap(), text);
        // The header attributes are present on the reader info at open time.
        assert_eq!(info_attrs.get("baz"), Some(&"quux".to_string()));
    }

    #[tokio::test]
    async fn v1_errors_when_too_few_bytes() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: text_header("s1", Some(5), HashMap::new(), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, vec![b'x']),
            encryption_type: EncryptionType::None,
        });
        h.send_packet(Packet::Trailer(trailer("s1")));
        assert!(matches!(read_text(reader).await, Err(StreamError::Incomplete)));
    }

    #[tokio::test]
    async fn v1_errors_when_too_many_bytes() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: byte_header("s1", Some(3), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, vec![1, 2, 3, 4, 5]),
            encryption_type: EncryptionType::None,
        });
        h.send_packet(Packet::Trailer(trailer("s1")));
        assert!(matches!(read_bytes(reader).await, Err(StreamError::LengthExceeded)));
    }

    #[tokio::test]
    async fn v1_drops_on_encryption_type_mismatch() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: text_header("s1", Some(2), HashMap::new(), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, vec![b'h', b'i']),
            encryption_type: EncryptionType::Gcm,
        });
        assert!(matches!(read_text(reader).await, Err(StreamError::EncryptionTypeMismatch)));
    }

    // --- v2 inline -----------------------------------------------------------------------

    #[tokio::test]
    async fn v2_inline_uncompressed_text() {
        let mut h = Harness::new(vec![]);
        let text = "inline hello";
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                Some(text.as_bytes().to_vec()),
                CompressionType::None,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        // No chunk/trailer packets are fed.
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v2_inline_uncompressed_byte() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: byte_header("s1", Some(3), Some(vec![1, 2, 3]), CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(vec![1u8, 2, 3]));
    }

    #[tokio::test]
    async fn v2_inline_compressed_text() {
        let mut h = Harness::new(vec![]);
        let text = "hello hello compressible world";
        let compressed = deflate_raw(text.as_bytes()).await;
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                Some(compressed),
                CompressionType::DeflateRaw,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v2_inline_compressed_byte() {
        let mut h = Harness::new(vec![]);
        let payload: Vec<u8> = (0..2000).map(|i| (i % 7) as u8).collect();
        let compressed = deflate_raw(&payload).await;
        h.send_packet(Packet::Header {
            header: byte_header(
                "s1",
                Some(payload.len() as u64),
                Some(compressed),
                CompressionType::DeflateRaw,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(payload));
    }

    // --- v2 multi-packet compressed ------------------------------------------------------

    #[tokio::test]
    async fn v2_multipacket_compressed_text() {
        let mut h = Harness::new(vec![]);
        // ~60 KB of pseudo-random lowercase so the compressed output spans multiple chunks.
        let text = pseudo_random_text(60_000);
        let compressed = deflate_raw(text.as_bytes()).await;
        let chunk_pieces: Vec<&[u8]> = compressed.chunks(15_000).collect();
        assert!(chunk_pieces.len() >= 2, "expected multi-packet compressed stream");

        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                HashMap::new(),
                None,
                CompressionType::DeflateRaw,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        for (i, piece) in chunk_pieces.iter().enumerate() {
            h.send_packet(Packet::Chunk {
                chunk: chunk("s1", i as u64, piece.to_vec()),
                encryption_type: EncryptionType::None,
            });
        }
        h.send_packet(Packet::Trailer(trailer("s1")));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn errors_open_streams_on_sender_disconnect() {
        let mut h = Harness::new(vec![]);
        h.send_packet(Packet::Header {
            header: text_header("s1", Some(10), HashMap::new(), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        // Partial content, no trailer: the sender then drops.
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, vec![b'h', b'e', b'l', b'l', b'o']),
            encryption_type: EncryptionType::None,
        });
        h.abort(ParticipantIdentity::from(SENDER));
        assert!(matches!(read_text(reader).await, Err(StreamError::AbnormalEnd(_))));
    }

    #[tokio::test]
    async fn abort_only_affects_matching_sender() {
        let mut h = Harness::new(vec![]);
        h.send_packet_from(
            Packet::Header {
                header: text_header("s1", Some(5), HashMap::new(), None, CompressionType::None),
                encryption_type: EncryptionType::None,
            },
            "bob",
        );
        let (reader, _) = h.next_opened().await;
        h.send_packet_from(
            Packet::Chunk {
                chunk: chunk("s1", 0, vec![b'h', b'e', b'l', b'l', b'o']),
                encryption_type: EncryptionType::None,
            },
            "bob",
        );
        // A different participant disconnecting must not disturb bob's stream.
        h.abort(ParticipantIdentity::from(SENDER));
        h.send_packet_from(Packet::Trailer(trailer("s1")), "bob");
        assert_eq!(read_text(reader).await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn v2_compressed_gap_errors() {
        let mut h = Harness::new(vec![]);
        let text = pseudo_random_text(60_000);
        let compressed = deflate_raw(text.as_bytes()).await;
        let pieces: Vec<&[u8]> = compressed.chunks(15_000).collect();
        assert!(pieces.len() >= 2);
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(text.len() as u64),
                HashMap::new(),
                None,
                CompressionType::DeflateRaw,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 0, pieces[0].to_vec()),
            encryption_type: EncryptionType::None,
        });
        // Skip index 1 -> feed index 2: a gap is a hard error.
        h.send_packet(Packet::Chunk {
            chunk: chunk("s1", 2, pieces[1].to_vec()),
            encryption_type: EncryptionType::None,
        });
        assert!(matches!(read_text(reader).await, Err(StreamError::MissedChunk)));
    }

    // --- progress() ----------------------------------------------------------------------

    /// Returns the reader's progress stream regardless of its concrete kind. Boxed because the two
    /// `progress()` impls are distinct opaque types that don't unify across match arms.
    fn progress_of(
        reader: &AnyStreamReader,
    ) -> std::pin::Pin<Box<dyn Stream<Item = StreamProgress> + Send + '_>> {
        match reader {
            AnyStreamReader::Byte(r) => Box::pin(r.progress()),
            AnyStreamReader::Text(r) => Box::pin(r.progress()),
        }
    }

    /// Drains a progress stream to completion (the stream ends when the sender closes).
    async fn collect_progress(stream: impl Stream<Item = StreamProgress>) -> Vec<StreamProgress> {
        use futures_util::StreamExt;
        let mut stream = std::pin::pin!(stream);
        let mut out = Vec::new();
        while let Some(progress) = stream.next().await {
            out.push(progress);
        }
        out
    }

    /// The last value reaches the total, values never decrease, and the stream terminates.
    fn assert_progress_completes(values: &[StreamProgress], total: u64) {
        let last = values.last().expect("progress stream yielded at least one value");
        assert_eq!(last.bytes_processed(), total);
        assert_eq!(last.bytes_total(), Some(total));
        assert_eq!(last.percentage(), Some(1.0));
        assert!(
            values.windows(2).all(|w| w[0].bytes_processed() <= w[1].bytes_processed()),
            "progress must be monotonically non-decreasing: {values:?}"
        );
    }

    #[tokio::test]
    async fn progress_reports_completion_uncompressed_bytes() {
        let mut h = Harness::new(vec![]);
        let total = 12u64;
        h.send_packet(Packet::Header {
            header: byte_header("s1", Some(total), None, CompressionType::None),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        let progress = progress_of(&reader);
        // Feed the payload across several contiguous chunks; keep `reader` alive so the chunk
        // channel stays open while progress is observed.
        for (i, piece) in
            [vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]].into_iter().enumerate()
        {
            h.send_packet(Packet::Chunk {
                chunk: chunk("s1", i as u64, piece),
                encryption_type: EncryptionType::None,
            });
        }
        h.send_packet(Packet::Trailer(trailer("s1")));

        let values = collect_progress(progress).await;
        assert_progress_completes(&values, total);
        drop(reader);
    }

    #[tokio::test]
    async fn progress_reports_completion_compressed_text() {
        let mut h = Harness::new(vec![]);
        let text = pseudo_random_text(60_000);
        let total = text.len() as u64;
        let compressed = deflate_raw(text.as_bytes()).await;
        let pieces: Vec<&[u8]> = compressed.chunks(15_000).collect();
        assert!(pieces.len() >= 2, "expected multi-packet compressed stream");

        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(total),
                HashMap::new(),
                None,
                CompressionType::DeflateRaw,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        let progress = progress_of(&reader);
        for (i, piece) in pieces.iter().enumerate() {
            h.send_packet(Packet::Chunk {
                chunk: chunk("s1", i as u64, piece.to_vec()),
                encryption_type: EncryptionType::None,
            });
        }
        h.send_packet(Packet::Trailer(trailer("s1")));

        let values = collect_progress(progress).await;
        assert_progress_completes(&values, total);
        drop(reader);
    }

    #[tokio::test]
    async fn progress_reports_completion_inline() {
        let mut h = Harness::new(vec![]);
        let text = "inline hello";
        let total = text.len() as u64;
        h.send_packet(Packet::Header {
            header: text_header(
                "s1",
                Some(total),
                HashMap::new(),
                Some(text.as_bytes().to_vec()),
                CompressionType::None,
            ),
            encryption_type: EncryptionType::None,
        });
        let (reader, _) = h.next_opened().await;
        // The whole payload arrives in the header, so progress jumps straight to complete.
        let values = collect_progress(progress_of(&reader)).await;
        assert_progress_completes(&values, total);
        drop(reader);
    }
}
