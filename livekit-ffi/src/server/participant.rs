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
    server::room::RoomInner,
    server::{FfiHandle, FfiServer},
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
    pub fn perform_rpc(
        &self,
        server: &'static FfiServer,
        request: proto::PerformRpcRequest,
    ) -> FfiResult<proto::PerformRpcResponse> {
        let async_id = server.next_id();

        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                return Err(FfiError::InvalidRequest("Expected local participant".into()))
            }
        };

        let handle = server.async_runtime.spawn(async move {
            let result = local
                .perform_rpc(
                    request.destination_identity.to_string(),
                    request.method,
                    request.payload,
                    request.response_timeout_ms,
                )
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

            let _ = server.send_event(proto::ffi_event::Message::PerformRpc(callback));
        });
        server.watch_panic(handle);
        Ok(proto::PerformRpcResponse { async_id })
    }

    pub fn register_rpc_method(
        &self,
        server: &'static FfiServer,
        request: proto::RegisterRpcMethodRequest,
    ) -> FfiResult<proto::RegisterRpcMethodResponse> {
        let async_id = server.next_id();
        let method = request.method.clone();

        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                return Err(FfiError::InvalidRequest("Expected local participant".into()))
            }
        };

        let local_participant_handle = self.handle.clone();
        let room: Arc<RoomInner> = self.room.clone();
        local.register_rpc_method(
            method.clone(),
            move |request_id, caller_identity, payload, response_timeout| {
                Box::pin({
                    let room = room.clone();
                    let method = method.clone();
                    async move {
                        forward_rpc_method_invocation(
                            server,
                            room,
                            local_participant_handle,
                            method,
                            request_id,
                            caller_identity,
                            payload,
                            response_timeout,
                        )
                        .await
                    }
                })
            },
        );
        Ok(proto::RegisterRpcMethodResponse { async_id })
    }

    pub fn unregister_rpc_method(
        &self,
        server: &'static FfiServer,
        request: proto::UnregisterRpcMethodRequest,
    ) -> FfiResult<proto::UnregisterRpcMethodResponse> {
        let async_id = server.next_id();

        let local = match &self.participant {
            Participant::Local(local) => local.clone(),
            Participant::Remote(_) => {
                return Err(FfiError::InvalidRequest("Expected local participant".into()))
            }
        };

        local.unregister_rpc_method(request.method);

        Ok(proto::UnregisterRpcMethodResponse { async_id })
    }
}

async fn forward_rpc_method_invocation(
    server: &'static FfiServer,
    room: Arc<RoomInner>,
    local_participant_handle: FfiHandleId,
    method: String,
    request_id: String,
    caller_identity: ParticipantIdentity,
    payload: String,
    response_timeout: Duration,
) -> Result<String, RpcError> {
    let (tx, rx) = oneshot::channel();
    let invocation_id = server.next_id();

    let _ = server.send_event(proto::ffi_event::Message::RpcMethodInvocation(
        proto::RpcMethodInvocationEvent {
            local_participant_handle: local_participant_handle as u64,
            invocation_id,
            method,
            request_id,
            caller_identity: caller_identity.into(),
            payload,
            response_timeout_ms: response_timeout.as_millis() as u32,
        },
    ));

    room.store_rpc_method_invocation_waiter(invocation_id, tx);

    rx.await.unwrap_or_else(|_| {
        Err(RpcError {
            code: RpcErrorCode::ApplicationError as u32,
            message: "Error from method handler".to_string(),
            data: None,
        })
    })
}
