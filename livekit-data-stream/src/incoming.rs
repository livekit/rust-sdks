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

use super::{
    AnyStreamInfo, ByteStreamInfo, StreamError, StreamProgress, StreamResult, TextStreamInfo,
};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};
use livekit_common::EncryptionType;
use livekit_protocol::data_stream as proto;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Reader for an incoming data stream.
///
/// The stream being read from is kept open as long as its reader exists;
/// dropping the reader will close the stream.
///
pub trait StreamReader: Stream<Item = StreamResult<Self::Output>> {
    /// Type of output this reader produces.
    type Output;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Reads all incoming chunks from the byte stream, concatenating them
    /// into a single value which is returned once the stream closes normally.
    ///
    /// Returns the data consisting of all concatenated chunks.
    ///
    fn read_all(self) -> impl std::future::Future<Output = StreamResult<Self::Output>> + Send;
}

/// Reader for an incoming byte data stream.
pub struct ByteStreamReader {
    info: ByteStreamInfo,
    chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
}

/// Reader for an incoming text data stream.
pub struct TextStreamReader {
    info: TextStreamInfo,
    chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
}

impl StreamReader for ByteStreamReader {
    type Output = Bytes;
    type Info = ByteStreamInfo;

    fn info(&self) -> &ByteStreamInfo {
        &self.info
    }

    async fn read_all(mut self) -> StreamResult<Bytes> {
        let mut buffer = BytesMut::new();
        while let Some(result) = self.next().await {
            match result {
                Ok(bytes) => buffer.extend_from_slice(&bytes),
                Err(e) => return Err(e),
            }
        }
        Ok(buffer.freeze())
    }
}

impl ByteStreamReader {
    /// Reads incoming chunks from the byte stream, writing them to a file as they are received.
    ///
    /// Parameters:
    ///   - directory: The directory to write the file in. The system temporary directory is used if not specified.
    ///   - name_override: The name to use for the written file, overriding stream name.
    ///
    /// Returns: The path of the written file on disk.
    ///
    pub async fn write_to_file(
        mut self,
        directory: Option<impl AsRef<std::path::Path>>,
        name_override: Option<&str>,
    ) -> StreamResult<std::path::PathBuf> {
        let directory =
            directory.map(|d| d.as_ref().to_path_buf()).unwrap_or_else(|| std::env::temp_dir());
        let name = name_override.unwrap_or_else(|| &self.info.name);
        let file_path = directory.join(name);

        let mut file = tokio::fs::File::create(&file_path).await.map_err(StreamError::Io)?;

        while let Some(result) = self.next().await {
            let bytes = result?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &bytes)
                .await
                .map_err(StreamError::Io)?;
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(StreamError::Io)?;

        Ok(file_path)
    }
}

impl Stream for ByteStreamReader {
    type Item = StreamResult<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.chunk_rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => Poll::Ready(Some(Ok(chunk))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
impl TextStreamReader {
    /// Create a TextStreamReader for testing purposes.
    pub(crate) fn new_for_test(
        info: TextStreamInfo,
        chunk_rx: UnboundedReceiver<StreamResult<Bytes>>,
    ) -> Self {
        Self { info, chunk_rx }
    }
}

impl StreamReader for TextStreamReader {
    type Output = String;
    type Info = TextStreamInfo;

    fn info(&self) -> &TextStreamInfo {
        &self.info
    }

    async fn read_all(mut self) -> StreamResult<String> {
        let mut result = String::new();
        while let Some(chunk) = self.next().await {
            match chunk {
                Ok(text) => result.push_str(&text),
                Err(e) => return Err(e),
            }
        }
        Ok(result)
    }
}

impl Stream for TextStreamReader {
    type Item = StreamResult<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.chunk_rx).poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => match String::from_utf8(chunk.into()) {
                Ok(content) => Poll::Ready(Some(Ok(content))),
                Err(e) => {
                    this.chunk_rx.close();
                    Poll::Ready(Some(Err(StreamError::from(e))))
                }
            },
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Debug for ByteStreamReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStreamReader")
            .field("id", &self.info.id())
            .field("topic", &self.info.topic)
            .finish()
    }
}

impl Debug for TextStreamReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextStreamReader")
            .field("id", &self.info.id())
            .field("topic", &self.info.topic)
            .finish()
    }
}

pub enum AnyStreamReader {
    Byte(ByteStreamReader),
    Text(TextStreamReader),
}

impl AnyStreamReader {
    /// Creates a stream reader for the stream with the given info.
    pub(super) fn from(info: AnyStreamInfo) -> (Self, UnboundedSender<StreamResult<Bytes>>) {
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
        let reader = match info {
            AnyStreamInfo::Byte(info) => Self::Byte(ByteStreamReader { info, chunk_rx }),
            AnyStreamInfo::Text(info) => Self::Text(TextStreamReader { info, chunk_rx }),
        };
        return (reader, chunk_tx);
    }
}
struct Descriptor {
    progress: StreamProgress,
    chunk_tx: UnboundedSender<StreamResult<Bytes>>,
    encryption_type: EncryptionType,
    is_internal: bool,
    /// Whether this is a text stream (decompressed output is reframed on UTF-8 boundaries).
    is_text: bool,
    /// Per-stream deflate-raw decompressor; `Some` iff the header declared `DEFLATE_RAW`.
    decompressor: Option<DeflateDecompressState>,
    /// Highest chunk index processed so far (compressed streams; for dedup/gap detection).
    last_chunk_index: Option<u64>,
    // TODO(ladvoc): keep track of open time.
}

/// Streaming deflate-raw decompressor state for one compressed stream.
struct DeflateDecompressState {
    decompress: flate2::Decompress,
    /// Decompressed text bytes not yet yielded because they end mid-codepoint.
    pending_text: Vec<u8>,
}

impl DeflateDecompressState {
    fn new() -> Self {
        // `false` => raw deflate (no zlib header/checksum), matching the wire contract.
        Self { decompress: flate2::Decompress::new(false), pending_text: Vec::new() }
    }

    /// Feeds compressed `input` through the stateful decompressor, returning all
    /// decompressed output produced so far.
    fn run(&mut self, input: &[u8]) -> StreamResult<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = vec![0u8; 16384];
        let mut offset = 0;
        loop {
            let in_before = self.decompress.total_in();
            let out_before = self.decompress.total_out();
            let status = self
                .decompress
                .decompress(&input[offset..], &mut buf, flate2::FlushDecompress::None)
                .map_err(|_| StreamError::Decompression)?;
            let consumed = (self.decompress.total_in() - in_before) as usize;
            let produced = (self.decompress.total_out() - out_before) as usize;
            offset += consumed;
            out.extend_from_slice(&buf[..produced]);
            match status {
                flate2::Status::StreamEnd => break,
                // No progress and no input left to feed: wait for the next chunk.
                _ if consumed == 0 && produced == 0 => break,
                _ => {}
            }
        }
        Ok(out)
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
fn inflate_raw(data: &[u8]) -> StreamResult<Vec<u8>> {
    use std::io::Read;
    let mut decoder = flate2::read::DeflateDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|_| StreamError::Decompression)?;
    Ok(out)
}

/// Metadata about a data stream which has just been opened
pub(crate) struct DataStreamOpenInfo {
    pub(crate) reader: AnyStreamReader,
    pub(crate) identity: String,

    /// Whether the payload was deflate-raw compressed (data streams v2).
    #[allow(unused)]
    pub(crate) is_compressed: bool,

    /// Whether the whole payload was sent inline in the header as a single packet
    /// (data streams v2), rather than as separate chunk packets.
    #[allow(unused)]
    pub(crate) is_inline: bool,
}

#[derive(Clone)]
pub struct IncomingStreamManager {
    inner: Arc<Mutex<ManagerInner>>,
    open_tx: UnboundedSender<DataStreamOpenInfo>,
    /// Topics whose streams are handled internally by the SDK (e.g. RPC) and never surfaced as
    /// application events. Supplied by the host crate so this crate stays decoupled from RPC.
    reserved_topics: Arc<[String]>,
}

#[derive(Default)]
struct ManagerInner {
    open_streams: HashMap<String, Descriptor>,
}

impl IncomingStreamManager {
    pub fn new(
        reserved_topics: Vec<String>,
    ) -> (Self, UnboundedReceiver<DataStreamOpenInfo>) {
        let (open_tx, open_rx) = mpsc::unbounded_channel();
        (
            Self {
                inner: Arc::new(Mutex::new(Default::default())),
                open_tx,
                reserved_topics: reserved_topics.into(),
            },
            open_rx,
        )
    }

    /// Handles an incoming header packet.
    pub fn handle_header(
        &self,
        mut header: proto::Header,
        identity: String,
        encryption_type: livekit_protocol::encryption::Type,
    ) {
        let is_internal = self.reserved_topics.iter().any(|t| t == &header.topic);
        // Read the v2 signals before `try_from_with_encryption` consumes the header.
        let inline_content = header.inline_content.take();
        let is_compressed = header.compression() == proto::CompressionType::DeflateRaw;

        let Ok(info) = AnyStreamInfo::try_from_with_encryption(header, encryption_type.into())
            .inspect_err(|e| log::error!("Invalid header: {}", e))
        else {
            return;
        };

        let id = info.id().to_owned();
        let is_text = matches!(info, AnyStreamInfo::Text(_));
        let bytes_total = info.total_length();
        let stream_encryption_type = info.encryption_type();

        let mut inner = self.inner.lock();
        if inner.open_streams.contains_key(&id) {
            log::error!("Stream '{}' already open", id);
            return;
        }

        let (reader, chunk_tx) = AnyStreamReader::from(info);
        let _ = self.open_tx.send(DataStreamOpenInfo {
            reader,
            identity,
            is_compressed,
            is_inline: inline_content.is_some(),
        });

        // Inline single-packet stream: synthesize the complete content now; no chunk/trailer
        // packets will follow, so we never register an open descriptor.
        if let Some(content) = inline_content {
            let content = if is_compressed {
                match inflate_raw(&content) {
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
            encryption_type: stream_encryption_type,
            is_internal,
            is_text,
            decompressor: is_compressed.then(DeflateDecompressState::new),
            last_chunk_index: None,
        };
        inner.open_streams.insert(id, descriptor);
    }

    /// Returns whether the given open stream belongs to an internal topic
    /// (e.g. `lk.rpc_request`). Used to suppress `RoomEvent::Stream*Received`
    /// dispatches for traffic the SDK handles itself.
    pub fn is_internal(&self, stream_id: &str) -> bool {
        self.inner.lock().open_streams.get(stream_id).is_some_and(|d| d.is_internal)
    }

    /// Handles an incoming chunk packet.
    pub fn handle_chunk(
        &self,
        chunk: proto::Chunk,
        encryption_type: livekit_protocol::encryption::Type,
    ) {
        let id = chunk.stream_id;
        let mut inner = self.inner.lock();
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

            let is_text = descriptor.is_text;
            // Confine the decompressor borrow so we can re-borrow `inner` afterwards.
            let result: StreamResult<(u64, Bytes)> = {
                match decompressor.run(&chunk.content) {
                    Ok(decompressed) => {
                        let produced = decompressed.len() as u64;
                        let yielded = if is_text {
                            decompressor.reframe_text(decompressed)
                        } else {
                            Bytes::from(decompressed)
                        };
                        Ok((produced, yielded))
                    }
                    Err(error) => Err(error),
                }
            };

            let (produced, to_yield) = match result {
                Ok(value) => value,
                Err(error) => {
                    inner.close_stream_with_error(&id, error);
                    return;
                }
            };

            // Count decompressed bytes against the (uncompressed) total length.
            descriptor.progress.bytes_processed += produced;
            if matches!(descriptor.progress.bytes_total, Some(total) if descriptor.progress.bytes_processed > total)
            {
                inner.close_stream_with_error(&id, StreamError::LengthExceeded);
                return;
            }
            if !to_yield.is_empty() {
                inner.yield_chunk(&id, to_yield);
            }
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
            Some(total) => descriptor.progress.bytes_processed > total as u64,
            None => false,
        } {
            inner.close_stream_with_error(&id, StreamError::LengthExceeded);
            return;
        }
        inner.yield_chunk(&id, Bytes::from(chunk.content));
        // TODO: also yield progress
    }

    /// Handles an incoming trailer packet.
    pub fn handle_trailer(&self, trailer: proto::Trailer) {
        let id = trailer.stream_id;
        let mut inner = self.inner.lock();
        let Some(descriptor) = inner.open_streams.get_mut(&id) else {
            return;
        };

        if !match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed >= total as u64,
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
}

impl ManagerInner {
    fn yield_chunk(&mut self, id: &str, chunk: Bytes) {
        let Some(descriptor) = self.open_streams.get_mut(id) else {
            return;
        };
        if descriptor.chunk_tx.send(Ok(chunk)).is_err() {
            // Reader has been dropped, close the stream.
            self.close_stream(id);
        }
    }

    fn close_stream(&mut self, id: &str) {
        // Dropping the sender closes the channel.
        self.open_streams.remove(id);
    }

    fn close_stream_with_error(&mut self, id: &str, error: StreamError) {
        if let Some(descriptor) = self.open_streams.remove(id) {
            let _ = descriptor.chunk_tx.send(Err(error));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_protocol::encryption::Type as EncType;
    use std::collections::HashMap;

    const SENDER: &str = "alice";

    fn deflate_raw(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn attrs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// Deterministic, barely-compressible lowercase text (so its deflate output spans chunks).
    fn pseudo_random_text(len: usize) -> String {
        let mut text = String::with_capacity(len);
        let mut state: u64 = 0xdead_beef_cafe_babe;
        for _ in 0..len {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            text.push((b'a' + ((state >> 33) % 26) as u8) as char);
        }
        text
    }

    #[allow(clippy::too_many_arguments)]
    fn text_header(
        id: &str,
        total_length: Option<u64>,
        attributes: HashMap<String, String>,
        inline_content: Option<Vec<u8>>,
        compression: proto::CompressionType,
    ) -> proto::Header {
        proto::Header {
            stream_id: id.to_string(),
            timestamp: 0,
            topic: "topic".to_string(),
            mime_type: "text/plain".to_string(),
            total_length,
            encryption_type: 0,
            attributes,
            content_header: Some(proto::header::ContentHeader::TextHeader(
                proto::TextHeader::default(),
            )),
            inline_content,
            compression: compression as i32,
        }
    }

    fn byte_header(
        id: &str,
        total_length: Option<u64>,
        inline_content: Option<Vec<u8>>,
        compression: proto::CompressionType,
    ) -> proto::Header {
        proto::Header {
            stream_id: id.to_string(),
            timestamp: 0,
            topic: "topic".to_string(),
            mime_type: "application/octet-stream".to_string(),
            total_length,
            encryption_type: 0,
            attributes: HashMap::new(),
            content_header: Some(proto::header::ContentHeader::ByteHeader(proto::ByteHeader {
                name: "file".to_string(),
            })),
            inline_content,
            compression: compression as i32,
        }
    }

    fn chunk(id: &str, index: u64, content: Vec<u8>) -> proto::Chunk {
        proto::Chunk {
            stream_id: id.to_string(),
            chunk_index: index,
            content,
            ..Default::default()
        }
    }

    fn trailer(id: &str) -> proto::Trailer {
        proto::Trailer { stream_id: id.to_string(), ..Default::default() }
    }

    fn trailer_with_attrs(id: &str, attributes: HashMap<String, String>) -> proto::Trailer {
        proto::Trailer { stream_id: id.to_string(), reason: String::new(), attributes }
    }

    async fn recv_reader(rx: &mut UnboundedReceiver<DataStreamOpenInfo>) -> DataStreamOpenInfo {
        rx.recv().await.expect("a reader should be dispatched")
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

    // --- v1 (legacy multi-packet) --------------------------------------------------------

    #[tokio::test]
    async fn v1_text_stream_round_trips() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let text = "hello world";
        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                None,
                proto::CompressionType::None,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, identity, is_compressed, is_inline } =
            recv_reader(&mut rx).await;
        assert_eq!(identity, SENDER);
        assert_eq!(compressed, false);
        assert_eq!(inline, false);
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        mgr.handle_chunk(chunk("s1", 0, text.as_bytes().to_vec()), EncType::None);
        mgr.handle_trailer(trailer("s1"));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v1_byte_stream_round_trips() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        mgr.handle_header(
            byte_header("s1", Some(4), None, proto::CompressionType::None),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, vec![1, 2, 3, 4]), EncType::None);
        mgr.handle_trailer(trailer("s1"));
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(vec![1u8, 2, 3, 4]));
    }

    #[tokio::test]
    async fn v1_merges_trailer_attributes() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let text = "hi";
        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar"), ("baz", "quux")]),
                None,
                proto::CompressionType::None,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, text.as_bytes().to_vec()), EncType::None);
        mgr.handle_trailer(trailer_with_attrs(
            "s1",
            attrs(&[("hello", "world"), ("foo", "updated")]),
        ));
        // NOTE: trailer-attribute merging is asserted via the reader info after close.
        let info_attrs = text_info(&reader).attributes.clone();
        assert_eq!(read_text(reader).await.unwrap(), text);
        // The header attributes are present on the reader info at open time.
        assert_eq!(info_attrs.get("baz"), Some(&"quux".to_string()));
    }

    #[tokio::test]
    async fn v1_errors_when_too_few_bytes() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        mgr.handle_header(
            text_header("s1", Some(5), HashMap::new(), None, proto::CompressionType::None),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, vec![b'x']), EncType::None);
        mgr.handle_trailer(trailer("s1"));
        assert!(matches!(read_text(reader).await, Err(StreamError::Incomplete)));
    }

    #[tokio::test]
    async fn v1_errors_when_too_many_bytes() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        mgr.handle_header(
            byte_header("s1", Some(3), None, proto::CompressionType::None),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, vec![1, 2, 3, 4, 5]), EncType::None);
        mgr.handle_trailer(trailer("s1"));
        assert!(matches!(read_bytes(reader).await, Err(StreamError::LengthExceeded)));
    }

    #[tokio::test]
    async fn v1_drops_on_encryption_type_mismatch() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        mgr.handle_header(
            text_header("s1", Some(2), HashMap::new(), None, proto::CompressionType::None),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, vec![b'h', b'i']), EncType::Gcm);
        assert!(matches!(read_text(reader).await, Err(StreamError::EncryptionTypeMismatch)));
    }

    // --- v2 inline -----------------------------------------------------------------------

    #[tokio::test]
    async fn v2_inline_uncompressed_text() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let text = "inline hello";
        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                Some(text.as_bytes().to_vec()),
                proto::CompressionType::None,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        // No chunk/trailer packets are fed.
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v2_inline_uncompressed_byte() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        mgr.handle_header(
            byte_header("s1", Some(3), Some(vec![1, 2, 3]), proto::CompressionType::None),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(vec![1u8, 2, 3]));
    }

    #[tokio::test]
    async fn v2_inline_compressed_text() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let text = "hello hello compressible world";
        let compressed = deflate_raw(text.as_bytes());
        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                attrs(&[("foo", "bar")]),
                Some(compressed),
                proto::CompressionType::DeflateRaw,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        assert_eq!(text_info(&reader).attributes.get("foo"), Some(&"bar".to_string()));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v2_inline_compressed_byte() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let payload: Vec<u8> = (0..2000).map(|i| (i % 7) as u8).collect();
        let compressed = deflate_raw(&payload);
        mgr.handle_header(
            byte_header(
                "s1",
                Some(payload.len() as u64),
                Some(compressed),
                proto::CompressionType::DeflateRaw,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        assert_eq!(read_bytes(reader).await.unwrap(), Bytes::from(payload));
    }

    // --- v2 multi-packet compressed ------------------------------------------------------

    #[tokio::test]
    async fn v2_multipacket_compressed_text() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        // ~60 KB of pseudo-random lowercase so the compressed output spans multiple chunks.
        let text = pseudo_random_text(60_000);
        let compressed = deflate_raw(text.as_bytes());
        let chunk_pieces: Vec<&[u8]> = compressed.chunks(15_000).collect();
        assert!(chunk_pieces.len() >= 2, "expected multi-packet compressed stream");

        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                HashMap::new(),
                None,
                proto::CompressionType::DeflateRaw,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        for (i, piece) in chunk_pieces.iter().enumerate() {
            mgr.handle_chunk(chunk("s1", i as u64, piece.to_vec()), EncType::None);
        }
        mgr.handle_trailer(trailer("s1"));
        assert_eq!(read_text(reader).await.unwrap(), text);
    }

    #[tokio::test]
    async fn v2_compressed_gap_errors() {
        let (mgr, mut rx) = IncomingStreamManager::new(vec![]);
        let text = pseudo_random_text(60_000);
        let compressed = deflate_raw(text.as_bytes());
        let pieces: Vec<&[u8]> = compressed.chunks(15_000).collect();
        assert!(pieces.len() >= 2);
        mgr.handle_header(
            text_header(
                "s1",
                Some(text.len() as u64),
                HashMap::new(),
                None,
                proto::CompressionType::DeflateRaw,
            ),
            SENDER.to_string(),
            EncType::None,
        );
        let DataStreamOpenInfo { reader, .. } = recv_reader(&mut rx).await;
        mgr.handle_chunk(chunk("s1", 0, pieces[0].to_vec()), EncType::None);
        // Skip index 1 -> feed index 2: a gap is a hard error.
        mgr.handle_chunk(chunk("s1", 2, pieces[1].to_vec()), EncType::None);
        assert!(matches!(read_text(reader).await, Err(StreamError::MissedChunk)));
    }
}
