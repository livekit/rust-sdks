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
use livekit::{id::ParticipantIdentity, ByteStreamReader, StreamReader, TextStreamReader};

use super::{FfiHandle, FfiServer};
use crate::{proto, FfiHandleId, FfiResult};

/// FFI wrapper around [ByteStreamReader].
pub struct FfiByteStreamReader {
    pub handle_id: FfiHandleId,
    inner: ByteStreamReader,
}

/// FFI wrapper around [TextStreamReader].
pub struct FfiTextStreamReader {
    pub handle_id: FfiHandleId,
    inner: TextStreamReader,
}

impl FfiHandle for FfiByteStreamReader {}
impl FfiHandle for FfiTextStreamReader {}

impl FfiByteStreamReader {
    pub fn from_handler(
        server: &'static FfiServer,
        reader: ByteStreamReader,
        identity: ParticipantIdentity,
    ) {
        let handle_id = server.next_id();

        let info = reader.info().clone();
        let reader = Self { handle_id, inner: reader.into() };

        server.store_handle(reader.handle_id, reader);

        let open_event = proto::ByteStreamOpenedEvent {
            reader: proto::OwnedByteStreamReader {
                handle: proto::FfiOwnedHandle { id: handle_id },
                info: info.into(),
            },
            participant_identity: identity.to_string(),
        };

        server.send_event(proto::ffi_event::Message::ByteStreamOpened(open_event));
    }

    pub fn read_incremental(
        self,
        server: &'static FfiServer,
        _request: proto::ByteStreamReaderReadIncrementalRequest,
    ) -> FfiResult<proto::ByteStreamReaderReadIncrementalResponse> {
        server.async_runtime.spawn(async move {
            let mut stream = self.inner;
            while let Some(result) = stream.next().await {
                match result {
                    Ok((bytes, progress)) => {
                        let detail = proto::ByteStreamReaderChunkReceived {
                            content: bytes.to_vec(),
                            progress: progress.into(),
                        };
                        let event = proto::ByteStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(proto::byte_stream_reader_event::Detail::ChunkReceived(
                                detail,
                            )),
                        };
                        server.send_event(proto::ffi_event::Message::ByteStreamReaderEvent(event));
                    }
                    Err(err) => {
                        let detail = proto::ByteStreamReaderEos { error: Some(err.into()) };
                        let event = proto::ByteStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(proto::byte_stream_reader_event::Detail::Eos(detail)),
                        };
                        server.send_event(proto::ffi_event::Message::ByteStreamReaderEvent(event));
                        return;
                    }
                }
            }

            let detail = proto::ByteStreamReaderEos { error: None };
            let event = proto::ByteStreamReaderEvent {
                reader_handle: self.handle_id,
                detail: Some(proto::byte_stream_reader_event::Detail::Eos(detail)),
            };
            server.send_event(proto::ffi_event::Message::ByteStreamReaderEvent(event));
        });
        Ok(proto::ByteStreamReaderReadIncrementalResponse {})
    }

    pub fn read_all(
        self,
        server: &'static FfiServer,
        _request: proto::ByteStreamReaderReadAllRequest,
    ) -> FfiResult<proto::ByteStreamReaderReadAllResponse> {
        let async_id = server.next_id();
        server.async_runtime.spawn(async move {
            let result = self.inner.read_all().await.into();
            let callback =
                proto::ByteStreamReaderReadAllCallback { async_id, result: Some(result) };
            server.send_event(proto::ffi_event::Message::ByteStreamReaderReadAll(callback));
        });
        Ok(proto::ByteStreamReaderReadAllResponse { async_id })
    }

    pub fn write_to_file(
        self,
        server: &'static FfiServer,
        request: proto::ByteStreamReaderWriteToFileRequest,
    ) -> FfiResult<proto::ByteStreamReaderWriteToFileResponse> {
        let async_id = server.next_id();

        server.async_runtime.spawn(async move {
            let result = self
                .inner
                .write_to_file(request.directory, request.name_override.as_deref())
                .await
                .into();
            let callback =
                proto::ByteStreamReaderWriteToFileCallback { async_id, result: Some(result) };
            server.send_event(proto::ffi_event::Message::ByteStreamReaderWriteToFile(callback));
        });

        Ok(proto::ByteStreamReaderWriteToFileResponse { async_id })
    }
}

impl FfiTextStreamReader {
    pub fn from_handler(
        server: &'static FfiServer,
        reader: TextStreamReader,
        identity: ParticipantIdentity,
    ) {
        let handle_id = server.next_id();

        let info = reader.info().clone();
        let reader = Self { handle_id, inner: reader.into() };

        server.store_handle(reader.handle_id, reader);

        let open_event = proto::TextStreamOpenedEvent {
            reader: proto::OwnedTextStreamReader {
                handle: proto::FfiOwnedHandle { id: handle_id },
                info: info.into(),
            },
            participant_identity: identity.to_string(),
        };
        server.send_event(proto::ffi_event::Message::TextStreamOpened(open_event));
    }

    pub fn read_incremental(
        self,
        server: &'static FfiServer,
        _request: proto::TextStreamReaderReadIncrementalRequest,
    ) -> FfiResult<proto::TextStreamReaderReadIncrementalResponse> {
        server.async_runtime.spawn(async move {
            let mut stream = self.inner;
            while let Some(result) = stream.next().await {
                match result {
                    Ok((text, progress)) => {
                        let detail = proto::TextStreamReaderChunkReceived {
                            content: text,
                            progress: progress.into(),
                        };
                        let event = proto::TextStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(proto::text_stream_reader_event::Detail::ChunkReceived(
                                detail,
                            )),
                        };
                        server.send_event(proto::ffi_event::Message::TextStreamReaderEvent(event));
                    }
                    Err(err) => {
                        let detail = proto::TextStreamReaderEos { error: Some(err.into()) };
                        let event = proto::TextStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(proto::text_stream_reader_event::Detail::Eos(detail)),
                        };
                        server.send_event(proto::ffi_event::Message::TextStreamReaderEvent(event));
                        return;
                    }
                }
            }

            let detail = proto::TextStreamReaderEos { error: None };
            let event = proto::TextStreamReaderEvent {
                reader_handle: self.handle_id,
                detail: Some(proto::text_stream_reader_event::Detail::Eos(detail)),
            };
            server.send_event(proto::ffi_event::Message::TextStreamReaderEvent(event));
        });
        Ok(proto::TextStreamReaderReadIncrementalResponse {})
    }

    pub fn read_all(
        self,
        server: &'static FfiServer,
        _request: proto::TextStreamReaderReadAllRequest,
    ) -> FfiResult<proto::TextStreamReaderReadAllResponse> {
        let async_id = server.next_id();
        server.async_runtime.spawn(async move {
            let result = self.inner.read_all().await.into();
            let callback =
                proto::TextStreamReaderReadAllCallback { async_id, result: Some(result) };
            server.send_event(proto::ffi_event::Message::TextStreamReaderReadAll(callback));
        });
        Ok(proto::TextStreamReaderReadAllResponse { async_id })
    }
}
