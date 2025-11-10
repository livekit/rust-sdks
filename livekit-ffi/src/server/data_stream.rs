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
use livekit::{
    ByteStreamReader, ByteStreamWriter, StreamReader, StreamWriter, TextStreamReader,
    TextStreamWriter,
};

use super::{FfiHandle, FfiServer};
use crate::{proto, FfiHandleId, FfiResult};

/// FFI wrapper around [ByteStreamReader].
pub struct FfiByteStreamReader {
    pub handle_id: FfiHandleId,
    pub inner: ByteStreamReader,
}

/// FFI wrapper around [TextStreamReader].
pub struct FfiTextStreamReader {
    pub handle_id: FfiHandleId,
    pub inner: TextStreamReader,
}
/// FFI wrapper around [ByteStreamWriter].
pub struct FfiByteStreamWriter {
    pub handle_id: FfiHandleId,
    inner: ByteStreamWriter,
}

/// FFI wrapper around [TextStreamWriter].
pub struct FfiTextStreamWriter {
    pub handle_id: FfiHandleId,
    inner: TextStreamWriter,
}

impl FfiHandle for FfiByteStreamReader {}
impl FfiHandle for FfiTextStreamReader {}
impl FfiHandle for FfiByteStreamWriter {}
impl FfiHandle for FfiTextStreamWriter {}

impl FfiByteStreamReader {
    pub fn read_incremental(
        self,
        server: &'static FfiServer,
        _request: proto::ByteStreamReaderReadIncrementalRequest,
    ) -> FfiResult<proto::ByteStreamReaderReadIncrementalResponse> {
        let handle = server.async_runtime.spawn(async move {
            let mut stream = self.inner;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(bytes) => {
                        let detail =
                            proto::ByteStreamReaderChunkReceived { content: bytes.to_vec() };
                        let event = proto::ByteStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(detail.into()),
                        };
                        let _ = server.send_event(event.into());
                    }
                    Err(err) => {
                        let detail = proto::ByteStreamReaderEos { error: Some(err.into()) };
                        let event = proto::ByteStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(detail.into()),
                        };
                        let _ = server.send_event(event.into());
                        return;
                    }
                }
            }

            let detail = proto::ByteStreamReaderEos { error: None };
            let event = proto::ByteStreamReaderEvent {
                reader_handle: self.handle_id,
                detail: Some(detail.into()),
            };
            let _ = server.send_event(event.into());
        });
        server.watch_panic(handle);
        Ok(proto::ByteStreamReaderReadIncrementalResponse {})
    }

    pub fn read_all(
        self,
        server: &'static FfiServer,
        _request: proto::ByteStreamReaderReadAllRequest,
    ) -> FfiResult<proto::ByteStreamReaderReadAllResponse> {
        let async_id = server.next_id();
        let handle = server.async_runtime.spawn(async move {
            let result = self.inner.read_all().await.into();
            let callback =
                proto::ByteStreamReaderReadAllCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::ByteStreamReaderReadAllResponse { async_id })
    }

    pub fn write_to_file(
        self,
        server: &'static FfiServer,
        request: proto::ByteStreamReaderWriteToFileRequest,
    ) -> FfiResult<proto::ByteStreamReaderWriteToFileResponse> {
        let async_id = server.next_id();

        let handle = server.async_runtime.spawn(async move {
            let result = self
                .inner
                .write_to_file(request.directory, request.name_override.as_deref())
                .await
                .into();
            let callback =
                proto::ByteStreamReaderWriteToFileCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);

        Ok(proto::ByteStreamReaderWriteToFileResponse { async_id })
    }
}

impl FfiTextStreamReader {
    pub fn read_incremental(
        self,
        server: &'static FfiServer,
        _request: proto::TextStreamReaderReadIncrementalRequest,
    ) -> FfiResult<proto::TextStreamReaderReadIncrementalResponse> {
        let handle = server.async_runtime.spawn(async move {
            let mut stream = self.inner;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(text) => {
                        let detail = proto::TextStreamReaderChunkReceived { content: text };
                        let event = proto::TextStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(detail.into()),
                        };
                        let _ = server.send_event(event.into());
                    }
                    Err(err) => {
                        let detail = proto::TextStreamReaderEos { error: Some(err.into()) };
                        let event = proto::TextStreamReaderEvent {
                            reader_handle: self.handle_id,
                            detail: Some(detail.into()),
                        };
                        let _ = server.send_event(event.into());
                        return;
                    }
                }
            }

            let detail = proto::TextStreamReaderEos { error: None };
            let event = proto::TextStreamReaderEvent {
                reader_handle: self.handle_id,
                detail: Some(detail.into()),
            };
            let _ = server.send_event(event.into());
        });
        server.watch_panic(handle);
        Ok(proto::TextStreamReaderReadIncrementalResponse {})
    }

    pub fn read_all(
        self,
        server: &'static FfiServer,
        _request: proto::TextStreamReaderReadAllRequest,
    ) -> FfiResult<proto::TextStreamReaderReadAllResponse> {
        let async_id = server.next_id();
        let handle = server.async_runtime.spawn(async move {
            let result = self.inner.read_all().await.into();
            let callback =
                proto::TextStreamReaderReadAllCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::TextStreamReaderReadAllResponse { async_id })
    }
}

impl FfiByteStreamWriter {
    pub fn from_writer(
        server: &'static FfiServer,
        writer: ByteStreamWriter,
    ) -> proto::OwnedByteStreamWriter {
        let handle_id = server.next_id();
        let info = writer.info().clone();
        let writer = Self { handle_id, inner: writer };
        server.store_handle(handle_id, writer);
        proto::OwnedByteStreamWriter {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: info.into(),
        }
    }

    pub fn write(
        &self,
        server: &'static FfiServer,
        request: proto::ByteStreamWriterWriteRequest,
    ) -> FfiResult<proto::ByteStreamWriterWriteResponse> {
        let async_id = server.next_id();
        let inner = self.inner.clone();
        let handle = server.async_runtime.spawn(async move {
            let result = inner.write(&request.bytes).await;
            let callback = proto::ByteStreamWriterWriteCallback {
                async_id,
                error: result.map_err(|e| e.into()).err(),
            };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::ByteStreamWriterWriteResponse { async_id })
    }

    pub fn close(
        self,
        server: &'static FfiServer,
        request: proto::ByteStreamWriterCloseRequest,
    ) -> FfiResult<proto::ByteStreamWriterCloseResponse> {
        let async_id = server.next_id();
        let handle = server.async_runtime.spawn(async move {
            let result = match request.reason {
                Some(reason) => self.inner.close_with_reason(&reason).await,
                None => self.inner.close().await,
            };
            let callback = proto::ByteStreamWriterCloseCallback {
                async_id,
                error: result.map_err(|e| e.into()).err(),
            };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::ByteStreamWriterCloseResponse { async_id })
    }
}

impl FfiTextStreamWriter {
    pub fn from_writer(
        server: &'static FfiServer,
        writer: TextStreamWriter,
    ) -> proto::OwnedTextStreamWriter {
        let handle_id = server.next_id();
        let info = writer.info().clone();
        let writer = Self { handle_id, inner: writer };
        server.store_handle(handle_id, writer);
        proto::OwnedTextStreamWriter {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: info.into(),
        }
    }

    pub fn write(
        &self,
        server: &'static FfiServer,
        request: proto::TextStreamWriterWriteRequest,
    ) -> FfiResult<proto::TextStreamWriterWriteResponse> {
        let async_id = server.next_id();
        let inner = self.inner.clone();
        let handle = server.async_runtime.spawn(async move {
            let result = inner.write(&request.text).await;
            let callback = proto::TextStreamWriterWriteCallback {
                async_id,
                error: result.map_err(|e| e.into()).err(),
            };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::TextStreamWriterWriteResponse { async_id })
    }

    pub fn close(
        self,
        server: &'static FfiServer,
        request: proto::TextStreamWriterCloseRequest,
    ) -> FfiResult<proto::TextStreamWriterCloseResponse> {
        let async_id = server.next_id();
        let handle = server.async_runtime.spawn(async move {
            let result = match request.reason {
                Some(reason) => self.inner.close_with_reason(&reason).await,
                None => self.inner.close().await,
            };
            let callback = proto::TextStreamWriterCloseCallback {
                async_id,
                error: result.map_err(|e| e.into()).err(),
            };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::TextStreamWriterCloseResponse { async_id })
    }
}
