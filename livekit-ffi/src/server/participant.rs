// Copyright 2023 LiveKit, Inc.
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

use std::sync::Arc;

use livekit::prelude::*;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::{
    proto,
    server::{
        data_stream::{FfiByteStreamWriter, FfiTextStreamWriter},
        room::RoomInner,
        FfiHandle, FfiServer,
    },
    FfiError, FfiHandleId, FfiResult,
};

#[derive(Clone)]
pub struct FfiParticipant {
    pub handle: FfiHandleId,
    pub participant: Participant,
    pub room: Arc<RoomInner>,
}

impl FfiHandle for FfiParticipant {}

impl FfiParticipant {
    fn guard_local_participant(&self) -> FfiResult<LocalParticipant> {
        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                Err(FfiError::InvalidRequest("Expected local participant".into()))?
            }
        };
        Ok(local)
    }

    pub fn perform_rpc(
        &self,
        server: &'static FfiServer,
        request: proto::PerformRpcRequest,
    ) -> FfiResult<proto::PerformRpcResponse> {
        let async_id = server.next_id();

        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = local
                .perform_rpc(PerformRpcData {
                    destination_identity: request.destination_identity.to_string(),
                    method: request.method,
                    payload: request.payload,
                    response_timeout: request
                        .response_timeout_ms
                        .map(|ms| Duration::from_millis(ms as u64))
                        .unwrap_or(PerformRpcData::default().response_timeout),
                })
                .await;

            let callback = proto::PerformRpcCallback {
                async_id,
                payload: result.as_ref().ok().cloned(),
                error: result.as_ref().err().map(|error| proto::RpcError {
                    code: error.code,
                    message: error.message.clone(),
                    data: error.data.clone(),
                }),
            };

            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::PerformRpcResponse { async_id })
    }

    pub fn register_rpc_method(
        &self,
        server: &'static FfiServer,
        request: proto::RegisterRpcMethodRequest,
    ) -> FfiResult<proto::RegisterRpcMethodResponse> {
        let method = request.method.clone();

        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                return Err(FfiError::InvalidRequest("Expected local participant".into()))
            }
        };

        let local_participant_handle = self.handle.clone();
        let room: Arc<RoomInner> = self.room.clone();
        local.register_rpc_method(method.clone(), move |data| {
            Box::pin({
                let room = room.clone();
                let method = method.clone();
                async move {
                    forward_rpc_method_invocation(
                        server,
                        room,
                        local_participant_handle,
                        method,
                        data,
                    )
                    .await
                }
            })
        });
        Ok(proto::RegisterRpcMethodResponse {})
    }

    pub fn unregister_rpc_method(
        &self,
        request: proto::UnregisterRpcMethodRequest,
    ) -> FfiResult<proto::UnregisterRpcMethodResponse> {
        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                return Err(FfiError::InvalidRequest("Expected local participant".into()))
            }
        };

        local.unregister_rpc_method(request.method);

        Ok(proto::UnregisterRpcMethodResponse {})
    }

    pub fn send_file(
        &self,
        server: &'static FfiServer,
        request: proto::StreamSendFileRequest,
    ) -> FfiResult<proto::StreamSendFileResponse> {
        let async_id = server.next_id();
        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = match local.send_file(&request.file_path, request.options.into()).await {
                Ok(info) => proto::stream_send_file_callback::Result::Info(info.into()),
                Err(err) => proto::stream_send_file_callback::Result::Error(err.into()),
            };
            let callback = proto::StreamSendFileCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::StreamSendFileResponse { async_id })
    }

    pub fn send_text(
        &self,
        server: &'static FfiServer,
        request: proto::StreamSendTextRequest,
    ) -> FfiResult<proto::StreamSendTextResponse> {
        let async_id = server.next_id();
        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = match local.send_text(&request.text, request.options.into()).await {
                Ok(info) => proto::stream_send_text_callback::Result::Info(info.into()),
                Err(err) => proto::stream_send_text_callback::Result::Error(err.into()),
            };
            let callback = proto::StreamSendTextCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::StreamSendTextResponse { async_id })
    }

    pub fn send_bytes(
        &self,
        server: &'static FfiServer,
        request: proto::StreamSendBytesRequest,
    ) -> FfiResult<proto::StreamSendBytesResponse> {
        let async_id = server.next_id();
        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = match local.send_bytes(&request.bytes, request.options.into()).await {
                Ok(info) => proto::stream_send_bytes_callback::Result::Info(info.into()),
                Err(err) => proto::stream_send_bytes_callback::Result::Error(err.into()),
            };
            let callback = proto::StreamSendBytesCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::StreamSendBytesResponse { async_id })
    }

    pub fn stream_bytes(
        &self,
        server: &'static FfiServer,
        request: proto::ByteStreamOpenRequest,
    ) -> FfiResult<proto::ByteStreamOpenResponse> {
        let async_id = server.next_id();
        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = match local.stream_bytes(request.options.into()).await {
                Ok(writer) => {
                    let ffi_writer = FfiByteStreamWriter::from_writer(server, writer);
                    proto::byte_stream_open_callback::Result::Writer(ffi_writer)
                }
                Err(err) => proto::byte_stream_open_callback::Result::Error(err.into()),
            };
            let callback = proto::ByteStreamOpenCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::ByteStreamOpenResponse { async_id })
    }

    pub fn stream_text(
        &self,
        server: &'static FfiServer,
        request: proto::TextStreamOpenRequest,
    ) -> FfiResult<proto::TextStreamOpenResponse> {
        let async_id = server.next_id();
        let local = self.guard_local_participant()?;

        let handle = server.async_runtime.spawn(async move {
            let result = match local.stream_text(request.options.into()).await {
                Ok(writer) => {
                    let ffi_writer = FfiTextStreamWriter::from_writer(server, writer);
                    proto::text_stream_open_callback::Result::Writer(ffi_writer)
                }
                Err(err) => proto::text_stream_open_callback::Result::Error(err.into()),
            };
            let callback = proto::TextStreamOpenCallback { async_id, result: Some(result) };
            let _ = server.send_event(callback.into());
        });
        server.watch_panic(handle);
        Ok(proto::TextStreamOpenResponse { async_id })
    }
}

async fn forward_rpc_method_invocation(
    server: &'static FfiServer,
    room: Arc<RoomInner>,
    local_participant_handle: FfiHandleId,
    method: String,
    data: RpcInvocationData,
) -> Result<String, RpcError> {
    let (tx, rx) = oneshot::channel();
    let invocation_id = server.next_id();

    room.store_rpc_method_invocation_waiter(invocation_id, tx);

    let _ = server.send_event(
        proto::RpcMethodInvocationEvent {
            local_participant_handle: local_participant_handle as u64,
            invocation_id,
            method,
            request_id: data.request_id,
            caller_identity: data.caller_identity.into(),
            payload: data.payload,
            response_timeout_ms: data.response_timeout.as_millis() as u32,
        }
        .into(),
    );

    rx.await.unwrap_or_else(|_| {
        Err(RpcError {
            code: RpcErrorCode::ApplicationError as u32,
            message: "Error from method handler".to_string(),
            data: None,
        })
    })
}
