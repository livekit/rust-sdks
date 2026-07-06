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

use bmrng::unbounded::UnboundedRequestSender;
use livekit_common::ParticipantIdentity;
use livekit_protocol as proto;
use std::{io::Write, path::Path};
use tokio::io::AsyncReadExt;

use crate::utils::{StreamProgress, StreamResult, StreamError, SendError};
use super::constants;

pub(crate) struct RawStreamOpenOptions {
    pub(crate) header: proto::data_stream::Header,
    pub(crate) destination_identities: Vec<ParticipantIdentity>,
    pub(crate) packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

pub(crate) struct RawStream {
    id: String,
    progress: StreamProgress,
    is_closed: bool,
    /// Request channel for sending packets.
    packet_tx: UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
}

impl RawStream {
    pub(crate) async fn open(options: RawStreamOpenOptions) -> StreamResult<Self> {
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

    pub(crate) async fn write_chunk(&mut self, bytes: &[u8]) -> StreamResult<()> {
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
    pub(crate) async fn write_raw_chunks(&mut self, bytes: &[u8]) -> StreamResult<()> {
        for chunk in bytes.chunks(constants::STREAM_CHUNK_SIZE_BYTES) {
            self.write_chunk(chunk).await?;
        }
        Ok(())
    }

    /// Streams a file's contents into MTU-sized chunks, optionally deflate-raw compressing
    /// on the fly. The whole file is never buffered in memory at once.
    pub(crate) async fn write_file(&mut self, path: impl AsRef<Path>, compress: bool) -> StreamResult<()> {
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
                while encoder.get_ref().len() >= constants::STREAM_CHUNK_SIZE_BYTES {
                    let rest = encoder.get_mut().split_off(constants::STREAM_CHUNK_SIZE_BYTES);
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
                while pending.len() >= constants::STREAM_CHUNK_SIZE_BYTES {
                    let rest = pending.split_off(constants::STREAM_CHUNK_SIZE_BYTES);
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

    pub(crate) async fn close(&mut self, reason: Option<&str>) -> StreamResult<()> {
        if self.is_closed {
            Err(StreamError::AlreadyClosed)?
        }
        let packet = Self::create_trailer_packet(&self.id, reason);
        Self::send_packet(&self.packet_tx, packet).await?;
        self.is_closed = true;
        Ok(())
    }

    pub(crate) async fn send_packet(
        tx: &UnboundedRequestSender<proto::DataPacket, Result<(), SendError>>,
        packet: proto::DataPacket,
    ) -> StreamResult<()> {
        tx.send_receive(packet)
            .await
            .map_err(|_| StreamError::Internal)? // request channel closed
            .map_err(|_| StreamError::SendFailed) // data channel error
    }

    pub(crate) fn create_header_packet(
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

    pub(crate) fn create_chunk_packet(id: &str, chunk_index: u64, content: &[u8]) -> proto::DataPacket {
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

    pub(crate) fn create_trailer_packet(id: &str, reason: Option<&str>) -> proto::DataPacket {
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
