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

use futures_util::StreamExt;
use livekit::{StreamError, StreamReader};
use std::sync::Arc;

use super::FfiServer;
use crate::{proto, FfiHandleId};

/// The shared uniffi objects. The handle map stores readers and writers as `Arc`s of these, so
/// that `from_ffi_handle_id` can hand the same object to the uniffi side.
pub use crate::data_stream::{
    incoming::{ByteStreamReader, TextStreamReader},
    outgoing::{ByteStreamWriter, TextStreamWriter},
};

/// Reads `reader` chunk by chunk, sending each chunk and then the end of the stream as events on
/// `reader_handle`.
pub fn read_byte_incremental(
    server: &'static FfiServer,
    reader_handle: FfiHandleId,
    reader: Arc<ByteStreamReader>,
) {
    let handle = server.async_runtime.spawn(async move {
        let Some(mut stream) = reader.take_inner().await else {
            let detail = proto::ByteStreamReaderEos {
                error: Some(StreamError::AlreadyClosed.into()),
                attributes: reader.info().attributes,
            };
            let event = proto::ByteStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
            let _ = server.send_event(event.into());
            return;
        };
        while let Some(result) = stream.next().await {
            match result {
                Ok(bytes) => {
                    let detail = proto::ByteStreamReaderChunkReceived { content: bytes.to_vec() };
                    let event =
                        proto::ByteStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
                    let _ = server.send_event(event.into());
                }
                Err(err) => {
                    let detail = proto::ByteStreamReaderEos {
                        error: Some(err.into()),
                        attributes: stream.info().attributes(),
                    };
                    let event =
                        proto::ByteStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
                    let _ = server.send_event(event.into());
                    return;
                }
            }
        }

        let detail =
            proto::ByteStreamReaderEos { error: None, attributes: stream.info().attributes() };
        let event = proto::ByteStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
        let _ = server.send_event(event.into());
    });
    server.watch_panic(handle);
}

/// Reads `reader` chunk by chunk, sending each chunk and then the end of the stream as events on
/// `reader_handle`.
pub fn read_text_incremental(
    server: &'static FfiServer,
    reader_handle: FfiHandleId,
    reader: Arc<TextStreamReader>,
) {
    let handle = server.async_runtime.spawn(async move {
        let Some(mut stream) = reader.take_inner().await else {
            let detail = proto::TextStreamReaderEos {
                error: Some(StreamError::AlreadyClosed.into()),
                attributes: reader.info().attributes,
            };
            let event = proto::TextStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
            let _ = server.send_event(event.into());
            return;
        };
        while let Some(result) = stream.next().await {
            match result {
                Ok(text) => {
                    let detail = proto::TextStreamReaderChunkReceived { content: text };
                    let event =
                        proto::TextStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
                    let _ = server.send_event(event.into());
                }
                Err(err) => {
                    let detail = proto::TextStreamReaderEos {
                        error: Some(err.into()),
                        attributes: stream.info().attributes(),
                    };
                    let event =
                        proto::TextStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
                    let _ = server.send_event(event.into());
                    return;
                }
            }
        }

        let detail =
            proto::TextStreamReaderEos { error: None, attributes: stream.info().attributes() };
        let event = proto::TextStreamReaderEvent { reader_handle, detail: Some(detail.into()) };
        let _ = server.send_event(event.into());
    });
    server.watch_panic(handle);
}
