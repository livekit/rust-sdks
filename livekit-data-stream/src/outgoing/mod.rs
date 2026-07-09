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

use bmrng::unbounded::{UnboundedRequestReceiver, UnboundedRequestSender};
use chrono::Utc;
use livekit_common::{
    ClientCapability, ParticipantIdentity, RemoteParticipantRegistry,
    CLIENT_PROTOCOL_DATA_STREAM_V2,
};
use livekit_protocol as proto;
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::Mutex;

use crate::info::{ByteStreamInfo, TextStreamInfo};
use crate::types::{
    ByteHeader, CompressionType, ContentHeader, Header, OperationType, StreamId, TextHeader,
};
use crate::utf8_chunk::Utf8AwareChunkExt;
use crate::utils::{SendError, StreamError, StreamResult};

mod stream_writer;
pub use stream_writer::{ByteStreamWriter, StreamWriter, TextStreamWriter};

mod constants;

mod raw_stream;
use raw_stream::{RawStream, RawStreamOpenOptions};

/// Generates a random stream identifier (UUID v4).
fn create_random_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
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
pub struct OutgoingDataStreamManager {
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

impl OutgoingDataStreamManager {
    pub fn new() -> (Self, UnboundedRequestReceiver<proto::DataPacket, Result<(), SendError>>) {
        let (packet_tx, packet_rx) = bmrng::unbounded_channel();
        let manager = Self { packet_tx };
        (manager, packet_rx)
    }

    pub async fn stream_text(&self, options: StreamTextOptions) -> StreamResult<TextStreamWriter> {
        // Incremental streams are never inlined or compressed (the content is unknown up front).
        let stream_id: StreamId = options.id.clone().unwrap_or_else(create_random_uuid).into();
        let dests = options.destination_identities.clone();
        let (header, text_header) =
            build_text_header(&options, stream_id, None, None, CompressionType::None);
        enforce_header_size(&header, &dests)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: dests,
            packet_tx: self.packet_tx.clone(),
        };
        let writer = TextStreamWriter::new(
            Arc::new(TextStreamInfo::from_headers(header, text_header)),
            Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        );
        Ok(writer)
    }

    pub async fn stream_bytes(&self, options: StreamByteOptions) -> StreamResult<ByteStreamWriter> {
        let stream_id: StreamId = options.id.clone().unwrap_or_else(create_random_uuid).into();
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
        let writer = ByteStreamWriter::new(
            Arc::new(ByteStreamInfo::from_headers(header, byte_header)),
            Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        );
        Ok(writer)
    }

    pub async fn send_text(
        &self,
        text: &str,
        options: StreamTextOptions,
        remote_participant_registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<TextStreamInfo> {
        let stream_id: StreamId = options.id.clone().unwrap_or_else(create_random_uuid).into();
        let total_length = text.len() as u64;

        let eligibility =
            evaluate_eligibility(remote_participant_registry, &options.destination_identities);
        let can_compress = options.compress.unwrap_or(true) && eligibility.compression;

        let text_bytes = text.as_bytes();
        let mut maybe_compressed = MaybeCollectedAsyncReader::from_async_reader(
            async_compression::futures::bufread::DeflateEncoder::new(
                futures_util::io::Cursor::new(text_bytes.to_vec()),
            ),
        );

        // Compress once up front when eligible (the deflate work happens at most once, cached in
        // the returned `Vec`), then decide whether it's worth using.
        let use_compression = can_compress
            && maybe_compressed.collect().await.is_ok_and(|c| c.len() < text_bytes.len());

        // 1. Inline single-packet attempt (no attachments; all recipients are >= v2).
        let (mut header, text_header) = if use_compression {
            build_text_header(
                &options,
                stream_id.clone(),
                Some(total_length),
                Some(maybe_compressed.collect().await?.to_owned()),
                CompressionType::DeflateRaw,
            )
        } else {
            build_text_header(
                &options,
                stream_id.clone(),
                Some(total_length),
                Some(text.as_bytes().to_vec()),
                CompressionType::None,
            )
        };

        {
            let proto_header = header.clone().into();
            if eligibility.inline
                && options.attached_stream_ids.is_empty()
                && header_packet_fits(&proto_header, &options.destination_identities)
            {
                let packet =
                    RawStream::create_header_packet(proto_header, options.destination_identities);
                RawStream::send_packet(&self.packet_tx, packet).await?;
                return Ok(TextStreamInfo::from_headers(header, text_header));
            }
        }

        // 2/3. Chunked, compressed when eligible else uncompressed.
        header.inline_content = None;
        enforce_header_size(&header, &options.destination_identities)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
        };
        let info = TextStreamInfo::from_headers(header, text_header);
        let mut stream = RawStream::open(open_options).await?;
        if use_compression {
            stream.write_raw_chunks(maybe_compressed.collect().await?).await?;
        } else {
            for chunk in text_bytes.utf8_aware_chunks(constants::STREAM_CHUNK_SIZE_BYTES) {
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
        remote_participant_registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<ByteStreamInfo> {
        if options.total_length.is_some() {
            log::warn!("Ignoring total_length option specified for send_bytes");
        }
        let bytes = data.as_ref();
        let stream_id: StreamId = options.id.clone().unwrap_or_else(create_random_uuid).into();
        let name = options.name.clone().unwrap_or_else(|| constants::BYTE_DEFAULT_NAME.to_owned());
        let total_length = bytes.len() as u64;

        let eligibility =
            evaluate_eligibility(remote_participant_registry, &options.destination_identities);
        let can_compress = options.compress.unwrap_or(true) && eligibility.compression;

        let mut maybe_compressed = MaybeCollectedAsyncReader::from_async_reader(
            async_compression::futures::bufread::DeflateEncoder::new(
                futures_util::io::Cursor::new(bytes.to_vec()),
            ),
        );

        // Compress once up front when eligible (the deflate work happens at most once, cached in
        // the returned `Vec`), then decide whether it's worth using.
        let use_compression =
            can_compress && maybe_compressed.collect().await.is_ok_and(|c| c.len() < bytes.len());

        // 1. Inline single-packet attempt (if all recipients are >= v2).
        let (mut header, byte_header) = if use_compression {
            build_byte_header(
                &options,
                stream_id.clone(),
                name.clone(),
                Some(total_length), // NOTE: this is purposely always uncompressed length
                Some(maybe_compressed.collect().await?.to_owned()),
                CompressionType::DeflateRaw,
            )
        } else {
            build_byte_header(
                &options,
                stream_id.clone(),
                name.clone(),
                Some(total_length), // NOTE: this is purposely always uncompressed length
                Some(bytes.to_vec()),
                CompressionType::None,
            )
        };
        {
            let proto_header = header.clone().into();
            if eligibility.inline
                && header_packet_fits(&proto_header, &options.destination_identities)
            {
                let packet =
                    RawStream::create_header_packet(proto_header, options.destination_identities);
                RawStream::send_packet(&self.packet_tx, packet).await?;
                return Ok(ByteStreamInfo::from_headers(header, byte_header));
            }
        }

        // 2/3. Chunked, compressed when eligible else uncompressed.
        header.inline_content = None;
        enforce_header_size(&header, &options.destination_identities)?;

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
        };
        let info = ByteStreamInfo::from_headers(header, byte_header);
        let mut stream = RawStream::open(open_options).await?;
        if use_compression {
            stream.write_raw_chunks(maybe_compressed.collect().await?).await?;
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
        remote_participant_registry: &dyn RemoteParticipantRegistry,
    ) -> StreamResult<ByteStreamInfo> {
        let path = path.as_ref();
        let file_size = tokio::fs::metadata(path)
            .await
            .map(|metadata| metadata.len())
            .map_err(StreamError::from)?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
        let stream_id: StreamId = options.id.clone().unwrap_or_else(create_random_uuid).into();
        let dests = options.destination_identities.clone();

        let eligibility = evaluate_eligibility(remote_participant_registry, &dests);
        let should_compress = options.compress.unwrap_or(true) && eligibility.compression;
        let compression =
            if should_compress { CompressionType::DeflateRaw } else { CompressionType::None };

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
        stream.write_file(path, should_compress).await?;
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

/// Wraps an [`AsyncRead`] whose bytes are produced lazily (e.g. a deflate encoder), caching them
/// on first [`Self::collect`] so the underlying work runs at most once. Later `collect` calls
/// return the cached bytes without re-reading.
///
/// (The streaming, still-a-reader-after-collect variant needed by `send_file` will be added when
/// that path is migrated; today's callers only need `collect`.)
enum MaybeCollectedAsyncReader<Reader: futures_util::io::AsyncRead + Unpin> {
    Reader(Reader),
    Collected(Vec<u8>),
}

impl<Reader: futures_util::io::AsyncRead + Unpin> MaybeCollectedAsyncReader<Reader> {
    fn from_async_reader(reader: Reader) -> Self {
        Self::Reader(reader)
    }

    async fn collect(&mut self) -> Result<&[u8], std::io::Error> {
        use futures_util::io::AsyncReadExt;
        match self {
            Self::Collected(_) => { /* no-op, handled below */ }
            Self::Reader(reader) => {
                let mut buf = Vec::new();
                reader.read_to_end(&mut buf).await?;
                *self = Self::Collected(buf);
            }
        }
        let Self::Collected(bytes) = self else { unreachable!("just set to Collected") };
        Ok(bytes)
    }
}

/// Whether the serialized header `DataPacket` fits within the MTU budget.
fn header_packet_fits(
    header: &proto::data_stream::Header,
    destinations: &[ParticipantIdentity],
) -> bool {
    use prost::Message;
    let packet = RawStream::create_header_packet(header.clone(), destinations.to_vec());
    packet.encoded_len() <= constants::STREAM_CHUNK_SIZE_BYTES
}

/// Enforces the header-packet MTU budget on the chunked path (the inline path falls back
/// gracefully instead of erroring).
fn enforce_header_size(header: &Header, destinations: &[ParticipantIdentity]) -> StreamResult<()> {
    let proto_header: proto::data_stream::Header = header.clone().into();
    if header_packet_fits(&proto_header, destinations) {
        Ok(())
    } else {
        Err(StreamError::HeaderTooLarge)
    }
}

fn build_text_header(
    options: &StreamTextOptions,
    stream_id: StreamId,
    total_length: Option<u64>,
    inline_content: Option<Vec<u8>>,
    compression: CompressionType,
) -> (Header, TextHeader) {
    let text_header = TextHeader {
        operation_type: options.operation_type.unwrap_or_default(),
        version: options.version.unwrap_or_default(),
        reply_to_stream_id: options.reply_to_stream_id.clone().map(StreamId::from),
        attached_stream_ids: options
            .attached_stream_ids
            .clone()
            .into_iter()
            .map(StreamId::from)
            .collect(),
        generated: options.generated.unwrap_or_default(),
    };
    let header = Header {
        stream_id,
        timestamp: Utc::now().timestamp_millis(),
        topic: options.topic.clone(),
        mime_type: constants::TEXT_MIME_TYPE.to_owned(),
        total_length,
        attributes: options.attributes.clone(),
        content_header: Some(ContentHeader::TextHeader(text_header.clone().into())),
        inline_content,
        compression,
    };
    (header, text_header)
}

fn build_byte_header(
    options: &StreamByteOptions,
    stream_id: StreamId,
    name: String,
    total_length: Option<u64>,
    inline_content: Option<Vec<u8>>,
    compression: CompressionType,
) -> (Header, ByteHeader) {
    let byte_header = ByteHeader { name };
    let header = Header {
        stream_id,
        timestamp: Utc::now().timestamp_millis(),
        topic: options.topic.clone(),
        mime_type: options
            .mime_type
            .clone()
            .unwrap_or_else(|| constants::BYTE_MIME_TYPE.to_owned()),
        total_length,
        attributes: options.attributes.clone(),
        content_header: Some(byte_header.clone().into()),
        inline_content,
        compression,
    };
    (header, byte_header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_common::{CLIENT_PROTOCOL_DATA_STREAM_RPC, CLIENT_PROTOCOL_DEFAULT};
    use std::sync::Mutex as StdMutex;

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
        FakeRegistry::new()
            .add("alice", CLIENT_PROTOCOL_DEFAULT, &[])
            .add("bob", CLIENT_PROTOCOL_DEFAULT, &[])
            .add("jim", CLIENT_PROTOCOL_DATA_STREAM_RPC, &[])
    }

    fn all_v2_room() -> FakeRegistry {
        FakeRegistry::new()
            .add(
                "alice",
                CLIENT_PROTOCOL_DATA_STREAM_V2,
                &[ClientCapability::CompressionDeflateRaw],
            )
            .add("bob", CLIENT_PROTOCOL_DATA_STREAM_V2, &[ClientCapability::CompressionDeflateRaw])
            .add("noCompression", CLIENT_PROTOCOL_DATA_STREAM_V2, &[])
    }

    fn mixed_room() -> FakeRegistry {
        FakeRegistry::new()
            .add("alice", CLIENT_PROTOCOL_DEFAULT, &[])
            .add("bob", CLIENT_PROTOCOL_DATA_STREAM_V2, &[ClientCapability::CompressionDeflateRaw])
            .add("jim", CLIENT_PROTOCOL_DATA_STREAM_V2, &[ClientCapability::CompressionDeflateRaw])
            .add("mallory", CLIENT_PROTOCOL_DEFAULT, &[])
            .add("noCompression", CLIENT_PROTOCOL_DATA_STREAM_V2, &[])
    }

    // --- Capture harness -----------------------------------------------------------------

    type Sent = Arc<StdMutex<Vec<proto::DataPacket>>>;

    fn setup() -> (OutgoingDataStreamManager, Sent) {
        let (manager, mut packet_rx) = OutgoingDataStreamManager::new();
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
    ///
    /// Seeded with a fixed value so the output is identical on every run.
    fn somewhat_compressible(blocks: usize) -> String {
        use rand::{rngs::StdRng, Rng, SeedableRng};

        /// Fixed RNG seed that keeps `somewhat_compressible` output identical on every run.
        const RANDOM_SEED: u64 = 0x1234_5678_9abc_def0;

        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
        let mut s = String::new();
        for _ in 0..blocks {
            s.push_str("hello world");
            for _ in 0..1000 {
                s.push(rng.random_range(b'a'..=b'z') as char);
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
        let uncompressed_chunks = text.len().div_ceil(constants::STREAM_CHUNK_SIZE_BYTES);
        assert!(chunks.len() >= 2);
        assert!(chunks.len() < uncompressed_chunks);
        assert_eq!(chunks[0].content.len(), constants::STREAM_CHUNK_SIZE_BYTES); // first chunk is full MTU
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

            let header = Header {
                stream_id: "gc-test-stream".into(),
                timestamp: 0,
                topic: "gc-test-topic".to_string(),
                mime_type: constants::TEXT_MIME_TYPE.to_owned(),
                total_length: None,
                attributes: HashMap::new(),
                content_header: None,
                // Data streams v2 fields
                inline_content: None,
                compression: CompressionType::None,
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
