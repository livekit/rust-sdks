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

            let _ = server.send_event(proto::ffi_event::Message::PerformRpc(callback));
        });
        server.watch_panic(handle);
        Ok(proto::PerformRpcResponse { async_id })
    }
}
