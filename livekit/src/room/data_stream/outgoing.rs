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
    ByteStreamInfo, OperationType, StreamError, StreamProgress, StreamResult, TextStreamInfo,
};
use crate::{
    id::ParticipantIdentity, rtc_engine::EngineError, utils::utf8_chunk::Utf8AwareChunkExt,
};
use bmrng::unbounded::{UnboundedRequestReceiver, UnboundedRequestSender};
use chrono::Utc;
use libwebrtc::native::create_random_uuid;
use livekit_protocol as proto;
use std::{collections::HashMap, path::Path, sync::Arc};
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
        for chunk in bytes.chunks(CHUNK_SIZE) {
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

impl ByteStreamWriter {
    /// Writes the contents of the file incrementally.
    async fn write_file_contents(&self, path: impl AsRef<Path>) -> StreamResult<()> {
        let mut stream = self.stream.lock().await;
        let mut file = tokio::fs::File::open(path).await?;
        let mut buffer = vec![0; 8192]; // 8KB
        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            stream.write_chunk(&buffer[..bytes_read]).await?;
        }
        Ok(())
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
        for chunk in text.as_bytes().utf8_aware_chunks(CHUNK_SIZE) {
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
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
    packet_coalescing_enabled: bool,
}

struct RawStream {
    id: String,
    progress: StreamProgress,
    is_closed: bool,
    pending_header: Option<proto::DataPacket>,
    pending_chunk: Option<proto::data_stream::Chunk>,
    packet_coalescing_enabled: bool,
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
}

impl RawStream {
    async fn open(options: RawStreamOpenOptions) -> StreamResult<Self> {
        let id = options.header.stream_id.to_string();
        let bytes_total = options.header.total_length;
        let header_packet =
            Self::create_header_packet(options.header, options.destination_identities);
        let pending_header = if options.packet_coalescing_enabled {
            Some(header_packet)
        } else {
            Self::send_packet(&options.packet_tx, header_packet).await?;
            None
        };

        Ok(Self {
            id,
            progress: StreamProgress { bytes_total, ..Default::default() },
            is_closed: false,
            pending_header,
            pending_chunk: None,
            packet_coalescing_enabled: options.packet_coalescing_enabled,
            packet_tx: options.packet_tx,
        })
    }

    async fn write_chunk(&mut self, bytes: &[u8]) -> StreamResult<()> {
        if self.packet_coalescing_enabled {
            return self.write_chunk_with_coalescing(bytes).await;
        }
        let packet = Self::create_chunk_packet(&self.id, self.progress.chunk_index, bytes);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.progress.bytes_processed += bytes.len() as u64;
        self.progress.chunk_index += 1;
        Ok(())
    }

    async fn close(&mut self, reason: Option<&str>) -> StreamResult<()> {
        if self.is_closed {
            Err(StreamError::AlreadyClosed)?
        }
        if self.packet_coalescing_enabled {
            return self.close_with_coalescing(reason).await;
        }
        let packet = Self::create_trailer_packet(&self.id, reason);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.is_closed = true;
        Ok(())
    }

    async fn send_packet(
        tx: &UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
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

    fn create_chunk_packet_from(chunk: proto::data_stream::Chunk) -> proto::DataPacket {
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

    fn create_trailer(id: &str, reason: Option<&str>) -> proto::data_stream::Trailer {
        proto::data_stream::Trailer {
            stream_id: id.to_string(),
            reason: reason.unwrap_or_default().to_owned(),
            ..Default::default()
        }
    }

    async fn write_chunk_with_coalescing(&mut self, bytes: &[u8]) -> StreamResult<()> {
        let mut offset = 0;

        if let Some(mut header_packet) = self.pending_header.take() {
            if let Some(livekit_protocol::data_packet::Value::StreamHeader(ref mut header)) =
                header_packet.value
            {
                let remaining_header_capacity = CHUNK_SIZE.saturating_sub(header.content.len());
                let inline_len = bytes.len().min(remaining_header_capacity);
                if inline_len > 0 {
                    header.content.extend_from_slice(&bytes[..inline_len]);
                    self.progress.bytes_processed += inline_len as u64;
                    if self.progress.chunk_index == 0 {
                        self.progress.chunk_index += 1;
                    }
                    offset = inline_len;
                }
            }

            // Keep a chance to emit a one-packet open/body/close on close().
            if offset < bytes.len() {
                Self::send_packet(&self.packet_tx, header_packet).await?;
            } else {
                self.pending_header = Some(header_packet);
                return Ok(());
            }
        }

        while offset < bytes.len() {
            let end = (offset + CHUNK_SIZE).min(bytes.len());
            let chunk = proto::data_stream::Chunk {
                stream_id: self.id.clone(),
                chunk_index: self.progress.chunk_index,
                content: bytes[offset..end].to_vec(),
                ..Default::default()
            };
            self.progress.bytes_processed += (end - offset) as u64;
            self.progress.chunk_index += 1;
            offset = end;

            if let Some(previous) = self.pending_chunk.replace(chunk) {
                Self::send_packet(&self.packet_tx, Self::create_chunk_packet_from(previous))
                    .await?;
            }
        }

        Ok(())
    }

    async fn close_with_coalescing(&mut self, reason: Option<&str>) -> StreamResult<()> {
        let trailer = Self::create_trailer(&self.id, reason);

        if let Some(mut header_packet) = self.pending_header.take() {
            if let Some(livekit_protocol::data_packet::Value::StreamHeader(ref mut header)) =
                header_packet.value
            {
                header.trailer = Some(trailer);
            }
            Self::send_packet(&self.packet_tx, header_packet).await?;
            self.is_closed = true;
            return Ok(());
        }

        if let Some(mut chunk) = self.pending_chunk.take() {
            chunk.trailer = Some(trailer);
            Self::send_packet(&self.packet_tx, Self::create_chunk_packet_from(chunk)).await?;
            self.is_closed = true;
            return Ok(());
        }

        // No body was buffered in this mode, fallback to standalone trailer packet.
        let packet = Self::create_trailer_packet(&self.id, reason);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.is_closed = true;
        Ok(())
    }
}

impl Drop for RawStream {
    /// Close stream normally if not already closed.
    fn drop(&mut self) {
        if self.is_closed {
            return;
        }
        let packet = if self.packet_coalescing_enabled {
            if let Some(mut header_packet) = self.pending_header.take() {
                if let Some(livekit_protocol::data_packet::Value::StreamHeader(ref mut header)) =
                    header_packet.value
                {
                    header.trailer = Some(Self::create_trailer(&self.id, None));
                }
                header_packet
            } else if let Some(mut chunk) = self.pending_chunk.take() {
                chunk.trailer = Some(Self::create_trailer(&self.id, None));
                Self::create_chunk_packet_from(chunk)
            } else {
                Self::create_trailer_packet(&self.id, None)
            }
        } else {
            Self::create_trailer_packet(&self.id, None)
        };
        let packet_tx = self.packet_tx.clone();
        tokio::spawn(async move { Self::send_packet(&packet_tx, packet).await });
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
    pub(crate) packet_coalescing_enabled: bool,
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
    pub(crate) packet_coalescing_enabled: bool,
}

#[derive(Clone)]
pub(crate) struct OutgoingStreamManager {
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
}

impl OutgoingStreamManager {
    pub fn new() -> (Self, UnboundedRequestReceiver<proto::DataPacket, Result<(), EngineError>>) {
        let (packet_tx, packet_rx) = bmrng::unbounded_channel();
        let manager = Self { packet_tx };
        (manager, packet_rx)
    }

    pub async fn stream_text(&self, options: StreamTextOptions) -> StreamResult<TextStreamWriter> {
        let text_header = proto::data_stream::TextHeader {
            operation_type: options.operation_type.unwrap_or_default() as i32,
            version: options.version.unwrap_or_default(),
            reply_to_stream_id: options.reply_to_stream_id.unwrap_or_default(),
            attached_stream_ids: options.attached_stream_ids,
            generated: options.generated.unwrap_or_default(),
        };
        let header = proto::data_stream::Header {
            stream_id: options.id.unwrap_or_else(|| create_random_uuid()),
            timestamp: Utc::now().timestamp_millis(),
            topic: options.topic,
            mime_type: TEXT_MIME_TYPE.to_owned(),
            total_length: None,
            encryption_type: proto::encryption::Type::None.into(),
            attributes: options.attributes,
            content_header: Some(proto::data_stream::header::ContentHeader::TextHeader(
                text_header.clone(),
            )),
        };
        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
            packet_coalescing_enabled: options.packet_coalescing_enabled,
        };
        let writer = TextStreamWriter {
            info: Arc::new(TextStreamInfo::from_headers(header, text_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };
        Ok(writer)
    }

    pub async fn stream_bytes(&self, options: StreamByteOptions) -> StreamResult<ByteStreamWriter> {
        let byte_header = proto::data_stream::ByteHeader { name: options.name.unwrap_or_default() };
        let header = proto::data_stream::Header {
            stream_id: options.id.unwrap_or_else(|| create_random_uuid()),
            timestamp: Utc::now().timestamp_millis(),
            topic: options.topic,
            mime_type: options.mime_type.unwrap_or_else(|| BYTE_MIME_TYPE.to_owned()),
            total_length: options.total_length,
            encryption_type: proto::encryption::Type::None.into(),
            attributes: options.attributes,
            content_header: Some(proto::data_stream::header::ContentHeader::ByteHeader(
                byte_header.clone(),
            )),
        };

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
            packet_coalescing_enabled: options.packet_coalescing_enabled,
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
    ) -> StreamResult<TextStreamInfo> {
        let text_header = proto::data_stream::TextHeader {
            operation_type: options.operation_type.unwrap_or_default() as i32,
            version: options.version.unwrap_or_default(),
            reply_to_stream_id: options.reply_to_stream_id.unwrap_or_default(),
            attached_stream_ids: options.attached_stream_ids,
            generated: options.generated.unwrap_or_default(),
        };
        let header = proto::data_stream::Header {
            stream_id: options.id.unwrap_or_else(|| create_random_uuid()),
            timestamp: Utc::now().timestamp_millis(),
            topic: options.topic,
            mime_type: TEXT_MIME_TYPE.to_owned(),
            total_length: Some(text.bytes().len() as u64),
            encryption_type: proto::encryption::Type::None.into(),
            attributes: options.attributes,
            content_header: Some(proto::data_stream::header::ContentHeader::TextHeader(
                text_header.clone(),
            )),
        };
        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
            packet_coalescing_enabled: options.packet_coalescing_enabled,
        };
        let writer = TextStreamWriter {
            info: Arc::new(TextStreamInfo::from_headers(header, text_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };

        let info = (*writer.info).clone();
        writer.write(text).await?;
        writer.close().await?;

        Ok(info)
    }

    /// Send bytes to participants in the room.
    ///
    /// This method sends an in-memory blob of bytes to participants in the room
    /// as a byte stream. It opens a stream using the provided options, writes the
    /// entire buffer, and closes the stream before returning.
    ///
    /// The `total_length` in the header is set from the provided data and is not
    /// overridable by `options.total_length`.
    pub async fn send_bytes(
        &self,
        data: impl AsRef<[u8]>,
        options: StreamByteOptions,
    ) -> StreamResult<ByteStreamInfo> {
        if options.total_length.is_some() {
            log::warn!("Ignoring total_length option specified for send_bytes");
        }
        let bytes = data.as_ref();

        let byte_header = proto::data_stream::ByteHeader { name: options.name.unwrap_or_default() };
        let header = proto::data_stream::Header {
            stream_id: options.id.unwrap_or_else(|| create_random_uuid()),
            timestamp: Utc::now().timestamp_millis(),
            topic: options.topic,
            mime_type: options.mime_type.unwrap_or_else(|| BYTE_MIME_TYPE.to_owned()),
            total_length: Some(bytes.len() as u64), // not overridable
            encryption_type: proto::encryption::Type::None.into(),
            attributes: options.attributes,
            content_header: Some(proto::data_stream::header::ContentHeader::ByteHeader(
                byte_header.clone(),
            )),
        };

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
            packet_coalescing_enabled: options.packet_coalescing_enabled,
        };
        let writer = ByteStreamWriter {
            info: Arc::new(ByteStreamInfo::from_headers(header, byte_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };

        let info = (*writer.info).clone();
        writer.write(bytes).await?;
        writer.close().await?;

        Ok(info)
    }

    pub async fn send_file(
        &self,
        path: impl AsRef<Path>,
        options: StreamByteOptions,
    ) -> StreamResult<ByteStreamInfo> {
        let file_size = tokio::fs::metadata(path.as_ref())
            .await
            .map(|metadata| metadata.len())
            .map_err(|e| StreamError::from(e))?;
        let name =
            path.as_ref().file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();

        let byte_header = proto::data_stream::ByteHeader { name };
        let header = proto::data_stream::Header {
            stream_id: options.id.unwrap_or_else(|| create_random_uuid()),
            timestamp: Utc::now().timestamp_millis(),
            topic: options.topic,
            mime_type: options.mime_type.unwrap_or_else(|| BYTE_MIME_TYPE.to_owned()),
            total_length: Some(file_size as u64), // not overridable
            encryption_type: proto::encryption::Type::None.into(),
            attributes: options.attributes,
            content_header: Some(proto::data_stream::header::ContentHeader::ByteHeader(
                byte_header.clone(),
            )),
        };

        let open_options = RawStreamOpenOptions {
            header: header.clone(),
            destination_identities: options.destination_identities,
            packet_tx: self.packet_tx.clone(),
            packet_coalescing_enabled: options.packet_coalescing_enabled,
        };
        let writer = ByteStreamWriter {
            info: Arc::new(ByteStreamInfo::from_headers(header, byte_header)),
            stream: Arc::new(Mutex::new(RawStream::open(open_options).await?)),
        };

        let info = (*writer.info).clone();
        writer.write_file_contents(path).await?;
        writer.close().await?;

        Ok(info)
    }
}

/// Maximum number of bytes to send in a single chunk.
static CHUNK_SIZE: usize = 15000;

// Default MIME type to use for byte streams.
static BYTE_MIME_TYPE: &str = "application/octet-stream";

/// Default MIME type to use for text streams.
static TEXT_MIME_TYPE: &str = "text/plain";
