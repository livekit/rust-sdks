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
use crate::{id::ParticipantIdentity, rtc_engine::EngineError, utils::Utf8AwareChunkExt};
use bmrng::unbounded::{UnboundedRequestReceiver, UnboundedRequestSender};
use chrono::Utc;
use libwebrtc::native::create_random_uuid;
use livekit_protocol as proto;
use parking_lot::Mutex;
use std::{collections::HashMap, path::Path};
use tokio::io::AsyncReadExt;

/// Writer for an open data stream.
pub trait StreamWriter<'a> {
    /// Type of input this writer accepts.
    type Input: 'a;

    /// Information about the underlying data stream.
    type Info;

    /// Returns a reference to the stream info.
    fn info(&self) -> &Self::Info;

    /// Writes to the stream.
    async fn write(&self, input: Self::Input) -> StreamResult<()>;

    /// Closes the stream normally.
    async fn close(self) -> StreamResult<()>;

    /// Closes the stream abnormally, specifying the reason for closure.
    async fn close_with_reason(self, reason: &str) -> StreamResult<()>;
}

/// Writer for an open byte data stream.
pub struct ByteStreamWriter {
    info: ByteStreamInfo,
    stream: RawStream,
}

/// Writer for an open text data stream.
pub struct TextStreamWriter {
    info: TextStreamInfo,
    stream: RawStream,
}

impl<'a> StreamWriter<'a> for ByteStreamWriter {
    type Input = &'a [u8];
    type Info = ByteStreamInfo;

    fn info(&self) -> &Self::Info {
        &self.info
    }

    async fn write(&self, bytes: &[u8]) -> StreamResult<()> {
        for chunk in bytes.chunks(CHUNK_SIZE) {
            self.stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.close(Some(reason.to_owned())).await
    }
}

impl ByteStreamWriter {
    /// Writes the contents of the file incrementally.
    async fn write_file_contents(&self, path: impl AsRef<Path>) -> StreamResult<()> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut buffer = vec![0; 8192]; // 8KB
        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            self.write(&buffer[..bytes_read]).await?;
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

    async fn write(&self, text: &str) -> StreamResult<()> {
        for chunk in text.as_bytes().utf8_aware_chunks(CHUNK_SIZE) {
            self.stream.write_chunk(chunk).await?;
        }
        Ok(())
    }

    async fn close(self) -> StreamResult<()> {
        self.stream.close(None).await
    }

    async fn close_with_reason(self, reason: &str) -> StreamResult<()> {
        self.stream.close(Some(reason.to_owned())).await
    }
}

struct RawStreamOpenOptions {
    header: proto::data_stream::Header,
    destination_identities: Vec<ParticipantIdentity>,
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
}

struct RawStream {
    id: String,
    progress: Mutex<StreamProgress>,
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
}

impl RawStream {
    async fn open(options: RawStreamOpenOptions) -> StreamResult<Self> {
        let id = options.header.stream_id.to_string();
        let bytes_total = options.header.total_length;

        let packet = proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            destination_identities: options
                .destination_identities
                .into_iter()
                .map(|id| id.0)
                .collect(),
            value: Some(livekit_protocol::data_packet::Value::StreamHeader(options.header)),
        };
        Self::send_packet(&options.packet_tx, packet).await?;

        Ok(Self {
            id,
            progress: Mutex::new(StreamProgress { bytes_total, ..Default::default() }),
            packet_tx: options.packet_tx,
        })
    }

    async fn write_chunk(&self, bytes: &[u8]) -> StreamResult<()> {
        let chunk_length = bytes.len();
        let chunk = proto::data_stream::Chunk {
            stream_id: self.id.to_owned(),
            chunk_index: self.progress.lock().chunk_index,
            content: bytes.to_vec(),
            ..Default::default()
        };
        let packet = proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            value: Some(livekit_protocol::data_packet::Value::StreamChunk(chunk)),
            ..Default::default()
        };

        let mut progress = self.progress.lock();
        progress.bytes_processed += chunk_length as u64;
        progress.chunk_index += 1;

        Self::send_packet(&self.packet_tx, packet).await
    }

    async fn close(self, reason: Option<String>) -> StreamResult<()> {
        let trailer = proto::data_stream::Trailer {
            stream_id: self.id,
            reason: reason.unwrap_or_default(),
            ..Default::default()
        };
        let packet = proto::DataPacket {
            kind: proto::data_packet::Kind::Reliable.into(),
            participant_identity: String::new(), // populate later
            value: Some(livekit_protocol::data_packet::Value::StreamTrailer(trailer)),
            ..Default::default()
        };
        Self::send_packet(&self.packet_tx, packet).await
    }

    async fn send_packet(
        tx: &UnboundedRequestSender<proto::DataPacket, Result<(), EngineError>>,
        packet: proto::DataPacket,
    ) -> StreamResult<()> {
        tx.send_receive(packet)
            .await
            .map_err(|_| StreamError::Internal)?
            .map_err(|_| StreamError::SendFailed)
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
    pub generated: bool,
}

#[derive(Clone)]
pub struct OutgoingStreamManager {
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
            generated: options.generated,
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
        };
        let writer = TextStreamWriter {
            info: TextStreamInfo::from_headers(header, text_header),
            stream: RawStream::open(open_options).await?,
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
        };
        let writer = ByteStreamWriter {
            info: ByteStreamInfo::from_headers(header, byte_header),
            stream: RawStream::open(open_options).await?,
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
            generated: options.generated,
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
        };
        let writer = TextStreamWriter {
            info: TextStreamInfo::from_headers(header, text_header),
            stream: RawStream::open(open_options).await?,
        };

        let info = writer.info.clone();
        writer.write(text).await?;
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
        };
        let writer = ByteStreamWriter {
            info: ByteStreamInfo::from_headers(header, byte_header),
            stream: RawStream::open(open_options).await?,
        };

        let info = writer.info.clone();
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
