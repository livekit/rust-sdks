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

use super::{handler::HandlerRegistry, ChunkSender};
use crate::{
    data_stream::{
        incoming::AnyStreamReader,
        info::{AnyStreamInfo, StreamInfo},
        StreamError, StreamProgress,
    },
    id::ParticipantIdentity,
};
use bytes::Bytes;
use livekit_protocol::data_stream as proto;
use std::collections::HashMap;

pub struct IncomingChunk {
    pub progress: StreamProgress,
    pub content: Bytes,
}

struct Descriptor {
    progress: StreamProgress,
    chunk_tx: ChunkSender,
    // TODO: keep track of open time.
}

#[derive(Default)]
pub struct IncomingStreamManager {
    open_streams: HashMap<String, Descriptor>,
    pub(crate) handlers: HandlerRegistry,
}

impl IncomingStreamManager {
    /// Handles an incoming header packet.
    pub fn handle_header(&mut self, header: proto::Header, identity: String) {
        let Ok(info) =
            AnyStreamInfo::try_from(header).inspect_err(|e| log::error!("Invalid header: {}", e))
        else {
            return;
        };

        let id = info.id().to_owned();
        let bytes_total = info.total_length();

        if self.open_streams.contains_key(&id) {
            log::error!("Stream '{}' already open", id);
            return;
        }

        let (reader, chunk_tx) = AnyStreamReader::from(info);
        self.handlers.dispatch(reader, ParticipantIdentity(identity));
        // TODO: log unhandled stream, once per topic

        let descriptor =
            Descriptor { progress: StreamProgress { bytes_total, ..Default::default() }, chunk_tx };
        self.open_streams.insert(id, descriptor);
    }

    /// Handles an incoming chunk packet.
    pub fn handle_chunk(&mut self, chunk: proto::Chunk) {
        let id = chunk.stream_id;
        let Some(descriptor) = self.open_streams.get_mut(&id) else {
            return;
        };

        if descriptor.progress.chunk_index != chunk.chunk_index {
            self.close_stream_with_error(&id, StreamError::MissedChunk);
            return;
        }

        descriptor.progress.chunk_index += 1;
        descriptor.progress.bytes_processed += chunk.content.len() as u64;

        if match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed > total as u64,
            None => false,
        } {
            self.close_stream_with_error(&id, StreamError::LengthExceeded);
            return;
        }

        let chunk =
            IncomingChunk { progress: descriptor.progress, content: Bytes::from(chunk.content) };
        self.yield_chunk(&id, chunk);
    }

    /// Handles an incoming trailer packet.
    pub fn handle_trailer(&mut self, trailer: proto::Trailer) {
        let id = trailer.stream_id;
        let Some(descriptor) = self.open_streams.get_mut(&id) else {
            return;
        };

        if !match descriptor.progress.bytes_total {
            Some(total) => descriptor.progress.bytes_processed >= total as u64,
            None => true,
        } {
            self.close_stream_with_error(&id, StreamError::Incomplete);
            return;
        }
        if !trailer.reason.is_empty() {
            self.close_stream_with_error(&id, StreamError::AbnormalEnd(trailer.reason));
            return;
        }
        self.close_stream(&id);
    }

    fn yield_chunk(&mut self, id: &str, chunk: IncomingChunk) {
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
