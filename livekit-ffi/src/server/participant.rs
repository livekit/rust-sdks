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

use std::collections::HashMap;
use std::{collections::HashSet, slice, sync::Arc};

use livekit::prelude::*;
use livekit::{participant, track};
use parking_lot::Mutex;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use super::FfiDataBuffer;
use crate::conversion::room;
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
                    data: error.data.clone().unwrap_or_default(),
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

        let ffi_handle = self.handle.clone();
        let handle = server.async_runtime.spawn(async move {
            local.register_rpc_method(
                method.clone(),
                move |request_id, caller_identity, payload, timeout| {
                    let method = method.clone();
                    Box::pin(async move {
                        let (tx, rx) = oneshot::channel();
                        let invocation_id = server.next_id();

                        let _ = server.send_event(proto::ffi_event::Message::RpcMethodInvocation(
                            proto::RpcMethodInvocationEvent {
                                local_participant_handle: ffi_handle,
                                invocation_id,
                                method: method,
                                request_id: request_id,
                                caller_identity: caller_identity.into(),
                                payload: payload,
                                timeout_ms: timeout.as_millis() as u32,
                            },
                        ));

                        server.store_rpc_method_invocation_waiter(invocation_id, tx);

                        match rx.await {
                            Ok(response) => match response {
                                Ok(payload) => Ok(payload),
                                Err(rpc_error) => Err(rpc_error),
                            },
                            Err(_) => Err(RpcError {
                                code: RpcErrorCode::ApplicationError as u32,
                                message: "Error from method handler".to_string(),
                                data: None,
                            }),
                        }
                    })
                },
            );

            let callback = proto::RegisterRpcMethodCallback { async_id };

            let _ = server.send_event(proto::ffi_event::Message::RegisterRpcMethod(callback));
        });

        server.watch_panic(handle);
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

        let handle = server.async_runtime.spawn(async move {
            local.unregister_rpc_method(request.method);

            let callback = proto::UnregisterRpcMethodCallback { async_id };

            let _ = server.send_event(proto::ffi_event::Message::UnregisterRpcMethod(callback));
        });

        server.watch_panic(handle);
        Ok(proto::UnregisterRpcMethodResponse { async_id })
    }
}
