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
    create_random_uuid, ByteStreamInfo, OperationType, SendError, StreamError, StreamProgress,
    StreamResult, TextStreamInfo,
};
use crate::utf8_chunk::Utf8AwareChunkExt;
use bmrng::unbounded::{UnboundedRequestReceiver, UnboundedRequestSender};
use chrono::Utc;
use livekit_common::{
    ClientCapability, ParticipantIdentity, RemoteParticipantRegistry,
    CLIENT_PROTOCOL_DATA_STREAM_V2,
};
use livekit_protocol as proto;
use proto::data_stream::CompressionType;
use std::{collections::HashMap, io::Write, path::Path, sync::Arc};
use tokio::{io::AsyncReadExt, sync::Mutex};

/// Writer for an open data stream.
pub trait StreamWriter<'a> {
    /// Type of input this writer accepts.
    type Input: 'a;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Writes to the stream.
    fn write(
        &self,
        input: Self::Input,
    ) -> impl std::future::Future<Output = StreamResult<()>> + Send;

    /// Closes the stream normally.
    fn close(self) -> impl std::future::Future<Output = StreamResult<()>> + Send;

    /// Closes the stream abnormally, specifying the reason for closure.
    fn close_with_reason(
        self,
        reason: &str,
    ) -> impl std::future::Future<Output = StreamResult<()>> + Send;
}

#[derive(Clone)]
/// Writer for an open byte data stream.
pub struct ByteStreamWriter {
    info: Arc<ByteStreamInfo>,
    stream: Arc<Mutex<RawStream>>,
}

#[derive(Clone)]
/// Writer for an open text data stream.
pub struct TextStreamWriter {
    info: Arc<TextStreamInfo>,
    stream: Arc<Mutex<RawStream>>,
}

impl<'a> StreamWriter<'a> for ByteStreamWriter {
    type Input = &'a [u8];
    type Info = ByteStreamInfo;

    fn info(&self) -> &Self::Info {
        &self.info
    }

    async fn write(&self, bytes: &'a [u8]) -> StreamResult<()> {
        let mut stream = self.stream.lock().await;
        for chunk in bytes.chunks(STREAM_CHUNK_SIZE_BYTES) {
            stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.lock().await.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.lock().await.close(Some(reason)).await
    }
}

impl<'a> StreamWriter<'a> for TextStreamWriter {
    type Input = &'a str;
    type Info = TextStreamInfo;

    fn info(&self) -> &Self::Info {
        &self.info
    }

    async fn write(&self, text: &'a str) -> StreamResult<()> {
        let mut stream = self.stream.lock().await;
        for chunk in text.as_bytes().utf8_aware_chunks(STREAM_CHUNK_SIZE_BYTES) {
            stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.lock().await.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.lock().await.close(Some(reason)).await
    }
}

struct RawStreamOpenOptions {
    header: proto::data_stream::Header,
    destination_identities: Vec<ParticipantIdentity>,
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

struct RawStream {
    id: String,
    progress: StreamProgress,
    is_closed: bool,
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

impl RawStream {
    async fn open(options: RawStreamOpenOptions) -> StreamResult<Self> {
        let id = options.header.stream_id.to_string();
        let bytes_total = options.header.total_length;

        let packet = Self::create_header_packet(options.header, options.destination_identities);
        Self::send_packet(&options.packet_tx, packet).await?;

        Ok(Self {
            id,
            progress: StreamProgress { bytes_total, ..Default::default() },
            is_closed: false,
            packet_tx: options.packet_tx,
        })
    }

    async fn write_chunk(&mut self, bytes: &[u8]) -> StreamResult<()> {
        let packet = Self::create_chunk_packet(&self.id, self.progress.chunk_index, bytes);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.progress.bytes_processed += bytes.len() as u64;
        self.progress.chunk_index += 1;
        Ok(())
    }

    /// Writes opaque bytes split into MTU-sized chunks on raw byte boundaries.
    ///
    /// Used for byte payloads and for compressed (deflate-raw) content, where the bytes
    /// are opaque and must not be split on UTF-8 boundaries.
    async fn write_raw_chunks(&mut self, bytes: &[u8]) -> StreamResult<()> {
        for chunk in bytes.chunks(STREAM_CHUNK_SIZE_BYTES) {
            self.write_chunk(chunk).await?;
        }
        Ok(())
    }

    /// Streams a file's contents into MTU-sized chunks, optionally deflate-raw compressing
    /// on the fly. The whole file is never buffered in memory at once.
    async fn write_file(&mut self, path: impl AsRef<Path>, compress: bool) -> StreamResult<()> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut read_buf = vec![0u8; 8192];

        if compress {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            loop {
                let n = file.read(&mut read_buf).await?;
                if n == 0 {
                    break;
                }
                // Writing into a `Vec` is infallible.
                encoder.write_all(&read_buf[..n]).expect("deflate write to Vec is infallible");
                // Drain whole MTU-sized chunks of compressed output as they accumulate so
                // we never hold the full compressed file in memory.
                while encoder.get_ref().len() >= STREAM_CHUNK_SIZE_BYTES {
                    let rest = encoder.get_mut().split_off(STREAM_CHUNK_SIZE_BYTES);
                    let chunk = std::mem::replace(encoder.get_mut(), rest);
                    self.write_chunk(&chunk).await?;
                }
            }
            // Flush the final deflate block and send whatever compressed bytes remain.
            let remaining = encoder.finish().expect("deflate finish into Vec is infallible");
            self.write_raw_chunks(&remaining).await?;
        } else {
            let mut pending: Vec<u8> = Vec::new();
            loop {
                let n = file.read(&mut read_buf).await?;
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&read_buf[..n]);
                while pending.len() >= STREAM_CHUNK_SIZE_BYTES {
                    let rest = pending.split_off(STREAM_CHUNK_SIZE_BYTES);
                    let chunk = std::mem::replace(&mut pending, rest);
                    self.write_chunk(&chunk).await?;
                }
            }
            if !pending.is_empty() {
                self.write_chunk(&pending).await?;
            }
        }
        Ok(())
    }

    async fn close(&mut self, reason: Option<&str>) -> StreamResult<()> {
        if self.is_closed {
            Err(StreamError::AlreadyClosed)?
        }
        let packet = Self::create_trailer_packet(&self.id, reason);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.is_closed = true;
        Ok(())
    }

    async fn send_packet(
        tx: &UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
        packet: proto::DataPacket,
    ) -> StreamResult<()> {
        tx.send_receive(packet)
            .await
            .map_err(|_| StreamError::Internal)? // request channel closed
            .map_err(|_| StreamError::SendFailed) // data channel error
    }

    fn create_header_packet(
        header: proto::data_stream::Header,
        destination_identities: Vec<ParticipantIdentity>,
    ) -> proto::DataPacket {
        proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            destination_identities: destination_identities.into_iter().map(|id| id.0).collect(),
            value: Some(livekit_protocol::data_packet::Value::StreamHeader(header)),
            // TODO: placeholder for reliable data transport
            ..Default::default()
        }
    }

    fn create_chunk_packet(id: &str, chunk_index: u64, content: &[u8]) -> proto::DataPacket {
        let chunk = proto::data_stream::Chunk {
            stream_id: id.to_string(),
            chunk_index,
            content: content.to_vec(),
            ..Default::default()
        };
        proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            value: Some(livekit_protocol::data_packet::Value::StreamChunk(chunk)),
            ..Default::default()
        }
    }

    fn create_trailer_packet(id: &str, reason: Option<&str>) -> proto::DataPacket {
        let trailer = proto::data_stream::Trailer {
            stream_id: id.to_string(),
            reason: reason.unwrap_or_default().to_owned(),
            ..Default::default()
        };
        proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            value: Some(livekit_protocol::data_packet::Value::StreamTrailer(trailer)),
            ..Default::default()
        }
    }
}

impl Drop for RawStream {
    /// Close stream normally if not already closed.
    fn drop(&mut self) {
        if self.is_closed {
            return;
        }
        let packet = Self::create_trailer_packet(&self.id, None);
        let packet_tx = self.packet_tx.clone();
        // Use try_current() instead of assuming a Tokio runtime exists.
        // The drop can run on a non-Tokio thread (e.g. a GC finalizer in
        // Unity/.NET) or after the runtime has shut down, in which case
        // we silently skip the trailer — the connection is going away anyway.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { Self::send_packet(&packet_tx, packet).await });
        }
    }
}

/// Options used when opening an outgoing byte data stream.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct StreamByteOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    pub destination_identities: Vec<ParticipantIdentity>,
    pub id: Option<String>,
    pub mime_type: Option<String>,
    pub name: Option<String>,
    pub total_length: Option<u64>,
    /// Whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out). Ignored by the incremental `stream_bytes`.
    pub compress: Option<bool>,
}

/// Options used when opening an outgoing text data stream.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct StreamTextOptions {
    pub topic: String,
    pub attributes: HashMap<String, String>,
    pub destination_identities: Vec<ParticipantIdentity>,
    pub id: Option<String>,
    pub operation_type: Option<OperationType>,
    pub version: Option<i32>,
    pub reply_to_stream_id: Option<String>,
    pub attached_stream_ids: Vec<String>,
    pub generated: Option<bool>,
    /// Whether to deflate-raw compress the payload when all recipients support it.
    /// Defaults to `true` (compression opt-out). Ignored by the incremental `stream_text`.
    pub compress: Option<bool>,
}

#[derive(Clone)]
pub struct OutgoingStreamManager {
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

impl OutgoingStreamManager {
    pub fn new() -> (Self, UnboundedRequestReceiver<proto::DataPacket, Result<(), SendError>>) {
        let (packet_tx, packet_rx) = bmrng::unbounded_channel();
        let manager = Self { packet_tx };
        (manager, packet_rx)
    }

    pub async fn stream_text(&self, options: StreamTextOptions) -> StreamResult<TextStreamWriter> {
        // Incremental streams are never inlined or compressed (the content is unknown up front).
        let stream_id = options.id.clone().unwrap_or_else(create_random_uuid);
        let dests = options.destination_identities.clone();
        let (header, text_header) =
            build_text_header(&options, stream_id, None, None, CompressionType::None);
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let writer = TextStreamWriter {
            info: Arc::new(TextStreamInfo::from_headers(header, text_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };
        Ok(writer)
    }

    pub async fn stream_bytes(&self, options: StreamByteOptions) -> StreamResult<ByteStreamWriter> {
        let stream_id = options.id.clone().unwrap_or_else(create_random_uuid);
        let name = options.name.clone().unwrap_or_default();
        let dests = options.destination_identities.clone();
        let (header, byte_header) = build_byte_header(
            &options,
            stream_id,
            name,
            options.total_length,
            None,
            CompressionType::None,
        );
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let writer = ByteStreamWriter {
            info: Arc::new(ByteStreamInfo::from_headers(header, byte_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };
        Ok(writer)
    }

    pub async fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
        registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<TextStreamInfo> {
        let stream_id = options.id.clone().unwrap_or_else(create_random_uuid);
        let total_length = text.len() as u64;
        let payload = text.as_bytes();
        let dests = options.destination_identities.clone();

        let eligibility = evaluate_eligibility(registry, &dests);
        let compress_ok = options.compress.unwrap_or(true) && eligibility.compression;

        // 1. Inline single-packet attempt (no attachments; all recipients are v2).
        if eligibility.inline && options.attached_stream_ids.is_empty() {
            let (content, compression) = maybe_compress_inline(payload, compress_ok);
            let (header, text_header) = build_text_header(
                &options,
                stream_id.clone(),
                Some(total_length),
                Some(content),
                compression,
            );
            if header_packet_fits(&header, &dests) {
                let packet = RawStream::create_header_packet(header.clone(), dests);
                RawStream::send_packet(&self.packet_tx, packet).await?;
                return Ok(TextStreamInfo::from_headers(header, text_header));
            }
            // Otherwise (large payload), fall through to the chunked path.
        }

        // 2/3. Chunked, compressed when eligible else uncompressed.
        let compression =
            if compress_ok { CompressionType::DeflateRaw } else { CompressionType::None };
        let (header, text_header) =
            build_text_header(&options, stream_id, Some(total_length), None, compression);
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let info = TextStreamInfo::from_headers(header, text_header);
        let mut stream = RawStream::open(open_options).await?;
        if compress_ok {
            stream.write_raw_chunks(&deflate_raw(payload)).await?;
        } else {
            for chunk in payload.utf8_aware_chunks(STREAM_CHUNK_SIZE_BYTES) {
                stream.write_chunk(chunk).await?;
            }
        }
        stream.close(None).await?;
        Ok(info)
    }

    /// Send bytes to participants in the room.
    ///
    /// This method sends an in-memory blob of bytes to participants in the room
    /// as a byte stream. It opens a stream using the provided options, writes the
    /// entire buffer, and closes the stream before returning.
    ///
    /// The `total_length` in the header is set from the provided data and is not
    /// overridable by `options.total_length`. The header defaults `name` to `"unknown"`
    /// and `mime_type` to `"application/octet-stream"`.
    pub async fn send_bytes(
        &self,
        data: impl AsRef<[u8]>,
        options: StreamByteOptions,
        registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<ByteStreamInfo> {
        if options.total_length.is_some() {
            log::warn!("Ignoring total_length option specified for send_bytes");
        }
        let bytes = data.as_ref();
        let stream_id = options.id.clone().unwrap_or_else(create_random_uuid);
        let name = options.name.clone().unwrap_or_else(|| BYTE_DEFAULT_NAME.to_owned());
        let total_length = bytes.len() as u64;
        let dests = options.destination_identities.clone();

        let eligibility = evaluate_eligibility(registry, &dests);
        let compress_ok = options.compress.unwrap_or(true) && eligibility.compression;

        // 1. Inline single-packet attempt (all recipients are v2).
        if eligibility.inline {
            let (content, compression) = maybe_compress_inline(bytes, compress_ok);
            let (header, byte_header) = build_byte_header(
                &options,
                stream_id.clone(),
                name.clone(),
                Some(total_length),
                Some(content),
                compression,
            );
            if header_packet_fits(&header, &dests) {
                let packet = RawStream::create_header_packet(header.clone(), dests);
                RawStream::send_packet(&self.packet_tx, packet).await?;
                return Ok(ByteStreamInfo::from_headers(header, byte_header));
            }
        }

        // 2/3. Chunked, compressed when eligible else uncompressed.
        let compression =
            if compress_ok { CompressionType::DeflateRaw } else { CompressionType::None };
        let (header, byte_header) =
            build_byte_header(&options, stream_id, name, Some(total_length), None, compression);
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let info = ByteStreamInfo::from_headers(header, byte_header);
        let mut stream = RawStream::open(open_options).await?;
        if compress_ok {
            stream.write_raw_chunks(&deflate_raw(bytes)).await?;
        } else {
            stream.write_raw_chunks(bytes).await?;
        }
        stream.close(None).await?;
        Ok(info)
    }

    /// Streams a file from disk to participants as a byte stream.
    ///
    /// Never uses the inline single-packet path (deciding inline-eligibility would require
    /// buffering and compressing the whole file up front). Compresses when every recipient
    /// supports it. The whole file is never buffered in memory at once.
    pub async fn send_file(
        &self,
        path: impl AsRef<Path>,
        options: StreamByteOptions,
        registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<ByteStreamInfo> {
        let path = path.as_ref();
        let file_size = tokio::fs::metadata(path)
            .await
            .map(|metadata| metadata.len())
            .map_err(StreamError::from)?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
        let stream_id = options.id.clone().unwrap_or_else(create_random_uuid);
        let dests = options.destination_identities.clone();

        let eligibility = evaluate_eligibility(registry, &dests);
        let compress_ok = options.compress.unwrap_or(true) && eligibility.compression;
        let compression =
            if compress_ok { CompressionType::DeflateRaw } else { CompressionType::None };

        let (header, byte_header) =
            build_byte_header(&options, stream_id, name, Some(file_size), None, compression);
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let info = ByteStreamInfo::from_headers(header, byte_header);
        let mut stream = RawStream::open(open_options).await?;
        stream.write_file(path, compress_ok).await?;
        stream.close(None).await?;
        Ok(info)
    }
}

/// Inline / compression eligibility evaluated over a send's recipients.
struct SendEligibility {
    /// Every recipient advertises `clientProtocol >= 2`.
    inline: bool,
    /// Inline-eligible AND every recipient advertises `CAP_COMPRESSION_DEFLATE_RAW`.
    compression: bool,
}

/// Evaluates inline/compression eligibility over a send's recipients.
///
/// Recipients are the named `destinations`, or every remote participant for a broadcast
/// (empty `destinations`). An empty recipient set (empty room) is eligible for everything.
fn evaluate_eligibility(
    registry: &dyn RemoteParticipantRegistry,
    destinations: &[ParticipantIdentity],
) -> SendEligibility {
    let recipients: Vec<ParticipantIdentity> =
        if destinations.is_empty() { registry.remote_identities() } else { destinations.to_vec() };
    let inline = recipients
        .iter()
        .all(|id| registry.remote_client_protocol(id) >= CLIENT_PROTOCOL_DATA_STREAM_V2);
    let compression = inline
        && recipients.iter().all(|id| {
            registry.remote_capabilities(id).contains(&ClientCapability::CompressionDeflateRaw)
        });
    SendEligibility { inline, compression }
}

/// Returns the inline payload and its compression flag: deflate-raw compressed when
/// `compress` is set AND the compressed form is actually smaller, else the raw bytes.
fn maybe_compress_inline(payload: &[u8], compress: bool) -> (Vec<u8>, CompressionType) {
    if compress {
        let compressed = deflate_raw(payload);
        if compressed.len() < payload.len() {
            return (compressed, CompressionType::DeflateRaw);
        }
    }
    (payload.to_vec(), CompressionType::None)
}

/// One-shot deflate-raw (raw DEFLATE, no zlib/gzip wrapper) of the full payload.
fn deflate_raw(data: &[u8]) -> Vec<u8> {
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).expect("deflate write to Vec is infallible");
    encoder.finish().expect("deflate finish into Vec is infallible")
}

/// Whether the serialized header `DataPacket` fits within the MTU budget.
fn header_packet_fits(
    header: &proto::data_stream::Header,
    destinations: &[ParticipantIdentity],
) -> bool {
    use prost::Message;
    let packet = RawStream::create_header_packet(header.clone(), destinations.to_vec());
    packet.encoded_len() <= STREAM_CHUNK_SIZE_BYTES
}

/// Enforces the header-packet MTU budget on the chunked path (the inline path falls back
/// gracefully instead of erroring).
fn enforce_header_size(
    header: &proto::data_stream::Header,
    destinations: &[ParticipantIdentity],
) -> StreamResult<()> {
    if header_packet_fits(header, destinations) {
        Ok(())
    } else {
        Err(StreamError::HeaderTooLarge)
    }
}

fn build_text_header(
    options: &StreamTextOptions,
    stream_id: String,
    total_length: Option<u64>,
    inline_content: Option<Vec<u8>>,
    compression: CompressionType,
) -> (proto::data_stream::Header, proto::data_stream::TextHeader) {
    let text_header = proto::data_stream::TextHeader {
        operation_type: options.operation_type.unwrap_or_default() as i32,
        version: options.version.unwrap_or_default(),
        reply_to_stream_id: options.reply_to_stream_id.clone().unwrap_or_default(),
        attached_stream_ids: options.attached_stream_ids.clone(),
        generated: options.generated.unwrap_or_default(),
    };
    let header = proto::data_stream::Header {
        stream_id,
        timestamp: Utc::now().timestamp_millis(),
        topic: options.topic.clone(),
        mime_type: TEXT_MIME_TYPE.to_owned(),
        total_length,
        encryption_type: proto::encryption::Type::None.into(),
        attributes: options.attributes.clone(),
        content_header: Some(proto::data_stream::header::ContentHeader::TextHeader(
            text_header.clone(),
        )),
        inline_content,
        compression: compression as i32,
    };
    (header, text_header)
}

fn build_byte_header(
    options: &StreamByteOptions,
    stream_id: String,
    name: String,
    total_length: Option<u64>,
    inline_content: Option<Vec<u8>>,
    compression: CompressionType,
) -> (proto::data_stream::Header, proto::data_stream::ByteHeader) {
    let byte_header = proto::data_stream::ByteHeader { name };
    let header = proto::data_stream::Header {
        stream_id,
        timestamp: Utc::now().timestamp_millis(),
        topic: options.topic.clone(),
        mime_type: options.mime_type.clone().unwrap_or_else(|| BYTE_MIME_TYPE.to_owned()),
        total_length,
        encryption_type: proto::encryption::Type::None.into(),
        attributes: options.attributes.clone(),
        content_header: Some(proto::data_stream::header::ContentHeader::ByteHeader(
            byte_header.clone(),
        )),
        inline_content,
        compression: compression as i32,
    };
    (header, byte_header)
}

/// Max chunk content size AND the header-packet MTU budget. Kept below the ~16 KB
/// data-channel MTU for protocol/E2EE framing headroom.
const STREAM_CHUNK_SIZE_BYTES: usize = 15000;

// Default MIME type to use for byte streams.
static BYTE_MIME_TYPE: &str = "application/octet-stream";

/// Default MIME type to use for text streams.
static TEXT_MIME_TYPE: &str = "text/plain";

/// Default name for `send_bytes` byte-stream headers.
static BYTE_DEFAULT_NAME: &str = "unknown";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    const V2: i32 = CLIENT_PROTOCOL_DATA_STREAM_V2;
    const DEFLATE: ClientCapability = ClientCapability::CompressionDeflateRaw;

    // --- Fake recipient registry ---------------------------------------------------------

    struct FakeRegistry {
        remotes: HashMap<String, (i32, Vec<ClientCapability>)>,
    }

    impl FakeRegistry {
        fn new() -> Self {
            Self { remotes: HashMap::new() }
        }

        fn add(mut self, id: &str, client_protocol: i32, caps: &[ClientCapability]) -> Self {
            self.remotes.insert(id.to_string(), (client_protocol, caps.to_vec()));
            self
        }
    }

    impl RemoteParticipantRegistry for FakeRegistry {
        fn remote_client_protocol(&self, identity: &ParticipantIdentity) -> i32 {
            self.remotes.get(&identity.0).map(|(p, _)| *p).unwrap_or(0)
        }
        fn remote_capabilities(&self, identity: &ParticipantIdentity) -> Vec<ClientCapability> {
            self.remotes.get(&identity.0).map(|(_, c)| c.clone()).unwrap_or_default()
        }
        fn remote_identities(&self) -> Vec<ParticipantIdentity> {
            self.remotes.keys().map(|k| ParticipantIdentity(k.clone())).collect()
        }
    }

    fn pre_v2_room() -> FakeRegistry {
        FakeRegistry::new().add("alice", 0, &[]).add("bob", 0, &[]).add("jim", 1, &[])
    }

    fn all_v2_room() -> FakeRegistry {
        FakeRegistry::new().add("alice", V2, &[DEFLATE]).add("bob", V2, &[DEFLATE]).add(
            "noCompression",
            V2,
            &[],
        )
    }

    fn mixed_room() -> FakeRegistry {
        FakeRegistry::new()
            .add("alice", 0, &[])
            .add("bob", V2, &[DEFLATE])
            .add("jim", V2, &[DEFLATE])
            .add("mallory", 1, &[])
            .add("noCompression", V2, &[])
    }

    // --- Capture harness -----------------------------------------------------------------

    type Sent = Arc<StdMutex<Vec<proto::DataPacket>>>;

    fn setup() -> (OutgoingStreamManager, Sent) {
        let (manager, mut packet_rx) = OutgoingStreamManager::new();
        let sent: Sent = Arc::new(StdMutex::new(Vec::new()));
        let sink = sent.clone();
        tokio::spawn(async move {
            while let Ok((packet, responder)) = packet_rx.recv().await {
                sink.lock().unwrap().push(packet);
                let _ = responder.respond(Ok(()));
            }
        });
        (manager, sent)
    }

    fn ids(list: &[&str]) -> Vec<ParticipantIdentity> {
        list.iter().map(|s| ParticipantIdentity(s.to_string())).collect()
    }

    fn text_opts(topic: &str, dests: &[&str]) -> StreamTextOptions {
        StreamTextOptions {
            topic: topic.to_string(),
            destination_identities: ids(dests),
            ..Default::default()
        }
    }

    fn byte_opts(topic: &str, dests: &[&str]) -> StreamByteOptions {
        StreamByteOptions {
            topic: topic.to_string(),
            destination_identities: ids(dests),
            ..Default::default()
        }
    }

    fn header(p: &proto::DataPacket) -> &proto::data_stream::Header {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamHeader(h) => h,
            _ => panic!("expected stream header"),
        }
    }

    fn chunk(p: &proto::DataPacket) -> &proto::data_stream::Chunk {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamChunk(c) => c,
            _ => panic!("expected stream chunk"),
        }
    }

    fn is_text_header(h: &proto::data_stream::Header) -> bool {
        matches!(h.content_header, Some(proto::data_stream::header::ContentHeader::TextHeader(_)))
    }

    fn is_byte_header(h: &proto::data_stream::Header) -> bool {
        matches!(h.content_header, Some(proto::data_stream::header::ContentHeader::ByteHeader(_)))
    }

    fn assert_trailer(p: &proto::DataPacket) {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamTrailer(t) => assert_eq!(t.reason, ""),
            _ => panic!("expected stream trailer"),
        }
    }

    fn deflate_raw_i32() -> i32 {
        CompressionType::DeflateRaw as i32
    }
    fn none_i32() -> i32 {
        CompressionType::None as i32
    }

    /// ~50 KB of deterministic, somewhat-compressible text (repeated marker + pseudo-random
    /// lowercase). Compresses to >15 KB (so it can't inline) but well under its raw size.
    fn somewhat_compressible(blocks: usize) -> String {
        let mut s = String::new();
        let mut state: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..blocks {
            s.push_str("hello world");
            for _ in 0..1000 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                s.push((b'a' + ((state >> 33) % 26) as u8) as char);
            }
        }
        s
    }

    // --- Pre-v2 room: legacy, uncompressed, multi-packet ---------------------------------

    #[tokio::test]
    async fn pre_v2_short_text_is_legacy_multipacket() {
        let (m, sent) = setup();
        m.send_text("hello world", text_opts("chat", &[]), &pre_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        let h = header(&p[0]);
        assert!(is_text_header(h));
        assert_eq!(h.topic, "chat");
        assert_eq!(h.compression, none_i32());
        assert!(h.inline_content.is_none());
        let c = chunk(&p[1]);
        assert_eq!(c.chunk_index, 0);
        assert_eq!(c.content, b"hello world");
        assert_trailer(&p[2]);
    }

    #[tokio::test]
    async fn pre_v2_long_text_splits_at_mtu() {
        let (m, sent) = setup();
        let text = "A".repeat(40_000);
        m.send_text(&text, text_opts("chat", &[]), &pre_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 5); // header + 3 chunks + trailer
        assert_eq!(header(&p[0]).compression, none_i32());
        assert_eq!(chunk(&p[1]).content.len(), 15_000);
        assert_eq!(chunk(&p[2]).content.len(), 15_000);
        assert_eq!(chunk(&p[3]).content.len(), 10_000);
        assert_eq!(chunk(&p[1]).chunk_index, 0);
        assert_eq!(chunk(&p[3]).chunk_index, 2);
        assert_trailer(&p[4]);
    }

    #[tokio::test]
    async fn pre_v2_bytes_is_legacy_multipacket() {
        let (m, sent) = setup();
        m.send_bytes([0u8, 1, 2, 3], byte_opts("blob", &[]), &pre_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        let h = header(&p[0]);
        assert!(is_byte_header(h));
        assert_eq!(h.compression, none_i32());
        assert!(h.inline_content.is_none());
        assert_eq!(chunk(&p[1]).content, vec![0, 1, 2, 3]);
        assert_trailer(&p[2]);
    }

    // --- All-v2 room: inline + compression -----------------------------------------------

    #[tokio::test]
    async fn v2_short_compressible_text_inlines_compressed() {
        let (m, sent) = setup();
        let text = "hello hello compressible world";
        m.send_text(text, text_opts("chat", &["alice", "bob"]), &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert!(is_text_header(h));
        assert_eq!(h.compression, deflate_raw_i32());
        let inline = h.inline_content.as_ref().unwrap();
        assert_ne!(inline.as_slice(), text.as_bytes()); // compressed, not raw
    }

    #[tokio::test]
    async fn v2_short_incompressible_text_inlines_raw() {
        let (m, sent) = setup();
        m.send_text("short", text_opts("chat", &["alice", "bob"]), &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert_eq!(h.compression, none_i32());
        assert_eq!(h.inline_content.as_ref().unwrap().as_slice(), b"short");
    }

    #[tokio::test]
    async fn v2_no_compression_cap_inlines_raw() {
        let (m, sent) = setup();
        let text = "hello hello compressible world";
        m.send_text(text, text_opts("chat", &["noCompression"]), &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1); // inline (gated on protocol) still happens
        let h = header(&p[0]);
        assert_eq!(h.compression, none_i32()); // compression gated off by missing cap
        assert_eq!(h.inline_content.as_ref().unwrap().as_slice(), text.as_bytes());
    }

    #[tokio::test]
    async fn v2_large_highly_compressible_text_still_inlines() {
        let (m, sent) = setup();
        let text = "hello world".repeat(20_000);
        m.send_text(&text, text_opts("chat", &["alice", "bob"]), &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert_eq!(h.compression, deflate_raw_i32());
        assert!(h.inline_content.as_ref().unwrap().len() < text.len());
    }

    #[tokio::test]
    async fn v2_somewhat_compressible_text_is_compressed_multipacket() {
        let (m, sent) = setup();
        let text = somewhat_compressible(50);
        m.send_text(&text, text_opts("chat", &["alice", "bob"]), &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        let h = header(&p[0]);
        assert_eq!(h.compression, deflate_raw_i32());
        assert!(h.inline_content.is_none());
        let chunks: Vec<_> = p[1..p.len() - 1].iter().map(chunk).collect();
        // Multi-packet, but fewer chunks than an uncompressed send would need (ceil(len/15000)).
        let uncompressed_chunks = text.len().div_ceil(STREAM_CHUNK_SIZE_BYTES);
        assert!(chunks.len() >= 2);
        assert!(chunks.len() < uncompressed_chunks);
        assert_eq!(chunks[0].content.len(), STREAM_CHUNK_SIZE_BYTES); // first chunk is full MTU
        let total: usize = chunks.iter().map(|c| c.content.len()).sum();
        assert!(total < text.len()); // compressed
        assert_trailer(p.last().unwrap());
    }

    #[tokio::test]
    async fn v2_compress_false_short_inlines_raw() {
        let (m, sent) = setup();
        let text = "hello hello compressible world";
        let opts =
            StreamTextOptions { compress: Some(false), ..text_opts("chat", &["alice", "bob"]) };
        m.send_text(text, opts, &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert_eq!(h.compression, none_i32());
        assert_eq!(h.inline_content.as_ref().unwrap().as_slice(), text.as_bytes());
    }

    #[tokio::test]
    async fn v2_compress_false_large_is_uncompressed_multipacket() {
        let (m, sent) = setup();
        let text = "B".repeat(50_000);
        let opts =
            StreamTextOptions { compress: Some(false), ..text_opts("chat", &["alice", "bob"]) };
        m.send_text(&text, opts, &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 6); // header + 4 chunks + trailer
        assert_eq!(header(&p[0]).compression, none_i32());
        assert_eq!(chunk(&p[1]).content.len(), 15_000);
    }

    // --- Incremental writers never compress or inline ------------------------------------

    #[tokio::test]
    async fn stream_text_never_compresses_or_inlines() {
        let (m, sent) = setup();
        let writer = m.stream_text(text_opts("chat", &["noCompression"])).await.unwrap();
        assert_eq!(sent.lock().unwrap().len(), 1);
        let h0 = sent.lock().unwrap()[0].clone();
        assert!(is_text_header(header(&h0)));
        assert_eq!(header(&h0).compression, none_i32());
        assert!(header(&h0).inline_content.is_none());

        writer.write("hello world").await.unwrap();
        assert_eq!(sent.lock().unwrap().len(), 2);
        assert_eq!(chunk(&sent.lock().unwrap()[1]).content, b"hello world");

        writer.close().await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        assert_trailer(&p[2]);
    }

    #[tokio::test]
    async fn stream_bytes_never_compresses_or_inlines() {
        let (m, sent) = setup();
        let writer = m.stream_bytes(byte_opts("blob", &["noCompression"])).await.unwrap();
        assert_eq!(sent.lock().unwrap().len(), 1);
        assert_eq!(header(&sent.lock().unwrap()[0]).compression, none_i32());

        writer.write(&[0u8, 1, 2, 3]).await.unwrap();
        assert_eq!(chunk(&sent.lock().unwrap()[1]).content, vec![0, 1, 2, 3]);

        writer.close().await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        assert_trailer(&p[2]);
    }

    // --- send_bytes inline behavior ------------------------------------------------------

    #[tokio::test]
    async fn v2_send_bytes_short_compressible_inlines_compressed() {
        let (m, sent) = setup();
        let payload = "hello hello compressible world".as_bytes().to_vec();
        let mut opts = byte_opts("blob", &["alice", "bob"]);
        opts.attributes.insert("foo".to_string(), "bar".to_string());
        let info = m.send_bytes(&payload, opts, &all_v2_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert!(is_byte_header(h));
        assert_eq!(h.compression, deflate_raw_i32());
        assert_ne!(h.inline_content.as_ref().unwrap().as_slice(), payload.as_slice());
        assert_eq!(info.name, "unknown");
        assert_eq!(info.mime_type, "application/octet-stream");
        assert_eq!(info.total_length, Some(payload.len() as u64));
        assert_eq!(info.attributes.get("foo"), Some(&"bar".to_string()));
    }

    // --- Mixed room ----------------------------------------------------------------------

    #[tokio::test]
    async fn mixed_broadcast_falls_back_to_legacy() {
        let (m, sent) = setup();
        m.send_text("hello world", text_opts("chat", &[]), &mixed_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        assert_eq!(header(&p[0]).compression, none_i32());
        assert!(header(&p[0]).inline_content.is_none());
        assert_eq!(chunk(&p[1]).content, b"hello world");
    }

    #[tokio::test]
    async fn mixed_targeted_v2_subset_inlines_compressed() {
        let (m, sent) = setup();
        let text = "hello hello compressible world";
        m.send_text(text, text_opts("chat", &["bob", "jim"]), &mixed_room()).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert_eq!(h.compression, deflate_raw_i32());
        assert_ne!(h.inline_content.as_ref().unwrap().as_slice(), text.as_bytes());
    }

    #[tokio::test]
    async fn mixed_targeted_subset_missing_cap_inlines_uncompressed() {
        let (m, sent) = setup();
        let text = "hello hello compressible world";
        m.send_text(text, text_opts("chat", &["bob", "jim", "noCompression"]), &mixed_room())
            .await
            .unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 1);
        let h = header(&p[0]);
        assert_eq!(h.compression, none_i32());
        assert_eq!(h.inline_content.as_ref().unwrap().as_slice(), text.as_bytes());
    }

    // --- send_file -----------------------------------------------------------------------

    async fn write_temp_file(bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lk_ds_test_{}.bin", create_random_uuid()));
        tokio::fs::write(&path, bytes).await.unwrap();
        path
    }

    #[tokio::test]
    async fn send_file_never_inlines_and_compresses_when_eligible() {
        let (m, sent) = setup();
        let path = write_temp_file(&vec![0x01u8; 10_000]).await;
        m.send_file(&path, byte_opts("file", &["alice", "bob"]), &all_v2_room()).await.unwrap();
        let _ = tokio::fs::remove_file(&path).await;
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3); // header + 1 chunk + trailer, NOT inline
        let h = header(&p[0]);
        assert!(is_byte_header(h));
        assert_eq!(h.compression, deflate_raw_i32());
        assert!(h.inline_content.is_none());
        assert!(chunk(&p[1]).content.len() < 10_000); // compressed
        assert_trailer(&p[2]);
    }

    #[tokio::test]
    async fn send_file_uncompressed_splits_at_mtu() {
        let (m, sent) = setup();
        let path = write_temp_file(&vec![0x07u8; 20_000]).await;
        m.send_file(&path, byte_opts("file", &[]), &pre_v2_room()).await.unwrap();
        let _ = tokio::fs::remove_file(&path).await;
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 4); // header + 15000 + 5000 + trailer
        assert_eq!(header(&p[0]).compression, none_i32());
        assert_eq!(chunk(&p[1]).content.len(), 15_000);
        assert_eq!(chunk(&p[2]).content.len(), 5_000);
        assert_eq!(chunk(&p[2]).chunk_index, 1);
        assert_trailer(&p[3]);
    }

    // --- Header size limit ---------------------------------------------------------------

    #[tokio::test]
    async fn oversized_attributes_on_chunked_path_errors() {
        let (m, _sent) = setup();
        let mut opts = text_opts("chat", &[]); // pre-v2 below => chunked path
        opts.attributes.insert("big".to_string(), "x".repeat(20_000));
        let result = m.send_text("hello", opts, &pre_v2_room()).await;
        assert!(matches!(result, Err(StreamError::HeaderTooLarge)));
    }

    type Sent = Arc<std::sync::Mutex<Vec<proto::DataPacket>>>;

    fn setup() -> (OutgoingStreamManager, Sent) {
        let (manager, mut packet_rx) = OutgoingStreamManager::new();
        let sent: Sent = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = sent.clone();
        tokio::spawn(async move {
            while let Ok((packet, responder)) = packet_rx.recv().await {
                sink.lock().unwrap().push(packet);
                let _ = responder.respond(Ok(()));
            }
        });
        (manager, sent)
    }

    fn ids(list: &[&str]) -> Vec<ParticipantIdentity> {
        list.iter().map(|s| ParticipantIdentity(s.to_string())).collect()
    }

    fn text_opts(topic: &str, dests: &[&str]) -> StreamTextOptions {
        StreamTextOptions {
            topic: topic.to_string(),
            destination_identities: ids(dests),
            ..Default::default()
        }
    }

    fn byte_opts(topic: &str, dests: &[&str]) -> StreamByteOptions {
        StreamByteOptions {
            topic: topic.to_string(),
            destination_identities: ids(dests),
            ..Default::default()
        }
    }

    fn header(p: &proto::DataPacket) -> &proto::data_stream::Header {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamHeader(h) => h,
            _ => panic!("expected stream header"),
        }
    }

    fn chunk(p: &proto::DataPacket) -> &proto::data_stream::Chunk {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamChunk(c) => c,
            _ => panic!("expected stream chunk"),
        }
    }

    fn is_text_header(h: &proto::data_stream::Header) -> bool {
        matches!(h.content_header, Some(proto::data_stream::header::ContentHeader::TextHeader(_)))
    }

    fn is_byte_header(h: &proto::data_stream::Header) -> bool {
        matches!(h.content_header, Some(proto::data_stream::header::ContentHeader::ByteHeader(_)))
    }

    fn assert_trailer(p: &proto::DataPacket) {
        match p.value.as_ref().unwrap() {
            proto::data_packet::Value::StreamTrailer(t) => assert_eq!(t.reason, ""),
            _ => panic!("expected stream trailer"),
        }
    }

    fn none_i32() -> i32 {
        proto::data_stream::CompressionType::None as i32
    }

    #[tokio::test]
    async fn short_text_is_legacy_multipacket() {
        let (m, sent) = setup();
        m.send_text("hello world", text_opts("chat", &[])).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        let h = header(&p[0]);
        assert!(is_text_header(h));
        assert_eq!(h.topic, "chat");
        assert_eq!(h.compression, none_i32());
        assert!(h.inline_content.is_none());
        let c = chunk(&p[1]);
        assert_eq!(c.chunk_index, 0);
        assert_eq!(c.content, b"hello world");
        assert_trailer(&p[2]);
    }

    #[tokio::test]
    async fn long_text_splits_at_mtu() {
        let (m, sent) = setup();
        let text = "A".repeat(40_000);
        m.send_text(&text, text_opts("chat", &[])).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 5); // header + 3 chunks + trailer
        assert_eq!(header(&p[0]).compression, none_i32());
        assert_eq!(chunk(&p[1]).content.len(), 15_000);
        assert_eq!(chunk(&p[2]).content.len(), 15_000);
        assert_eq!(chunk(&p[3]).content.len(), 10_000);
        assert_eq!(chunk(&p[1]).chunk_index, 0);
        assert_eq!(chunk(&p[3]).chunk_index, 2);
        assert_trailer(&p[4]);
    }

    #[tokio::test]
    async fn bytes_is_legacy_multipacket() {
        let (m, sent) = setup();
        m.send_bytes([0u8, 1, 2, 3], byte_opts("blob", &[])).await.unwrap();
        let p = sent.lock().unwrap().clone();
        assert_eq!(p.len(), 3);
        let h = header(&p[0]);
        assert!(is_byte_header(h));
        assert_eq!(h.compression, none_i32());
        assert!(h.inline_content.is_none());
        assert_eq!(chunk(&p[1]).content, vec![0, 1, 2, 3]);
        assert_trailer(&p[2]);
    }

    // Regression test for CLT-2773: dropping a `RawStream` on a thread that has
    // no Tokio runtime in TLS (e.g. the .NET GC finalizer thread in the Unity
    // SDK) used to panic because `Drop` called `tokio::spawn` unconditionally.
    #[test]
    fn drop_raw_stream_on_non_tokio_thread_does_not_panic() {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let raw_stream = rt.block_on(async {
            let (packet_tx, mut packet_rx) =
                bmrng::unbounded_channel::<proto::DataPacket, Result<(), SendError>>();

            tokio::spawn(async move {
                while let Ok((_packet, responder)) = packet_rx.recv().await {
                    let _ = responder.respond(Ok(()));
                }
            });

            let header = proto::data_stream::Header {
                stream_id: "gc-test-stream".to_string(),
                timestamp: 0,
                topic: "gc-test-topic".to_string(),
                mime_type: TEXT_MIME_TYPE.to_owned(),
                total_length: None,
                encryption_type: proto::encryption::Type::None.into(),
                attributes: HashMap::new(),
                content_header: None,
                // Data streams v2 fields
                inline_content: None,
                compression: proto::data_stream::CompressionType::None as i32,
            };

            RawStream::open(RawStreamOpenOptions {
                header,
                destination_identities: vec![],
                packet_tx,
            })
            .await
            .expect("RawStream should open")
        });

        let drop_thread = std::thread::spawn(move || drop(raw_stream));

        drop_thread.join().expect("Dropping RawStream on a non-Tokio thread must not panic");
    }
}
