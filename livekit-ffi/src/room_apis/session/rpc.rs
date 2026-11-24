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

//! Deadline-bound RPC calls and replies through the room's core RPC transport.

use super::{NativeRoomEvent, NativeRoomSession, NativeSessionError};
use livekit::participant::{PerformRpcData, RpcError, RpcErrorCode};
use parking_lot::Mutex;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

fn rpc_session_error(error: NativeSessionError) -> RpcError {
    RpcError::built_in(RpcErrorCode::SendFailed, Some(error.to_string()))
}

/// An RPC error that can travel in both directions across UniFFI.
#[derive(uniffi::Error, thiserror::Error, Clone, Debug, PartialEq)]
pub enum NativeRpcError {
    #[error("RPC {code}: {reason}")]
    Failed { code: u32, reason: String, data: Option<String> },
}
impl From<RpcError> for NativeRpcError {
    fn from(e: RpcError) -> Self {
        Self::Failed { code: e.code, reason: e.message, data: e.data }
    }
}
impl From<NativeRpcError> for RpcError {
    fn from(e: NativeRpcError) -> Self {
        let NativeRpcError::Failed { code, reason, data } = e;
        Self::new(code, reason, data)
    }
}

/// Parameters for an outgoing RPC. Timeouts are in milliseconds.
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeRpcOptions {
    pub destination_identity: String,
    pub method: String,
    pub payload: String,
    #[uniffi(default = 15000)]
    pub response_timeout_ms: u32,
    #[uniffi(default = 7000)]
    pub max_round_trip_latency_ms: u32,
}

/// Values describing an incoming RPC and its reply deadline.
#[derive(uniffi::Record, Clone, Debug)]
pub struct NativeRpcInvocationInfo {
    pub request_id: String,
    pub caller_identity: String,
    pub method: String,
    pub payload: String,
    pub response_timeout_ms: u64,
}

/// A one-shot reply handle. Dropping it rejects the invocation; late replies return false.
#[derive(uniffi::Object, Debug)]
pub struct NativeRpcInvocation {
    info: NativeRpcInvocationInfo,
    reply: Mutex<Option<oneshot::Sender<Result<String, RpcError>>>>,
}
impl PartialEq for NativeRpcInvocation {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}
#[uniffi::export]
impl NativeRpcInvocation {
    /// Returns the request values and the timeout supplied by the caller.
    pub fn info(&self) -> NativeRpcInvocationInfo {
        self.info.clone()
    }
    /// Replies successfully once, returning false after the deadline or a previous reply.
    pub fn respond(&self, payload: String) -> bool {
        self.complete(Ok(payload))
    }
    /// Rejects the request with an application or built-in RPC error.
    pub fn reject(&self, error: NativeRpcError) -> bool {
        self.complete(Err(error.into()))
    }
}
impl NativeRpcInvocation {
    pub(super) fn payload_bytes(&self) -> usize {
        self.info.payload.len()
    }

    fn complete(&self, value: Result<String, RpcError>) -> bool {
        self.reply.lock().take().is_some_and(|reply| reply.send(value).is_ok())
    }
}

#[uniffi::export]
impl NativeRoomSession {
    /// Performs an RPC on the core transport, preserving its error code and data.
    pub async fn perform_rpc(&self, options: NativeRpcOptions) -> Result<String, NativeRpcError> {
        let room = self.shared.room().map_err(rpc_session_error)?;
        self.shared
            .command(async move {
                let data = PerformRpcData {
                    destination_identity: options.destination_identity,
                    method: options.method,
                    payload: options.payload,
                    response_timeout: Duration::from_millis(options.response_timeout_ms.into()),
                    max_round_trip_latency: Duration::from_millis(
                        options.max_round_trip_latency_ms.into(),
                    ),
                };
                room.local_participant().perform_rpc(data).await
            })
            .await
            .map_err(rpc_session_error)?
            .map_err(Into::into)
    }

    /// Registers an RPC method. Requests arrive on the ordered room event cursor.
    /// At most 64 incoming invocations can wait for foreign replies per session.
    pub async fn register_rpc_method(&self, method: String) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        let weak = Arc::downgrade(&self.shared);
        self.shared
            .command(async move {
                room.local_participant().register_rpc_method(method.clone(), move |data| {
                    let weak = weak.clone();
                    let method = method.clone();
                    Box::pin(async move {
                        let shared = weak.upgrade().ok_or_else(|| {
                            RpcError::built_in(RpcErrorCode::RecipientDisconnected, None)
                        })?;
                        let permit =
                            shared.rpc_slots.clone().try_acquire_owned().map_err(|_| {
                                RpcError::built_in(
                                    RpcErrorCode::ApplicationError,
                                    Some("Too many pending RPC requests".into()),
                                )
                            })?;
                        let (reply, receive) = oneshot::channel();
                        let timeout = data.response_timeout;
                        let invocation = Arc::new(NativeRpcInvocation {
                            info: NativeRpcInvocationInfo {
                                request_id: data.request_id,
                                caller_identity: data.caller_identity.into(),
                                method,
                                payload: data.payload,
                                response_timeout_ms: timeout.as_millis().min(u64::MAX as u128)
                                    as u64,
                            },
                            reply: Mutex::new(Some(reply)),
                        });
                        let event_owner = Arc::downgrade(&shared);
                        shared
                            .command(async move {
                                let _permit = permit;
                                if let Some(shared) = event_owner.upgrade() {
                                    shared.update(|inner| {
                                        if inner.terminal.is_none() {
                                            inner
                                                .emit(NativeRoomEvent::RpcRequested { invocation });
                                        }
                                    });
                                }
                                match tokio::time::timeout(timeout, receive).await {
                                    Ok(Ok(result)) => result,
                                    Ok(Err(_)) => Err(RpcError::built_in(
                                        RpcErrorCode::ApplicationError,
                                        Some("Reply handle released".into()),
                                    )),
                                    Err(_) => {
                                        Err(RpcError::built_in(RpcErrorCode::ResponseTimeout, None))
                                    }
                                }
                            })
                            .await
                            .map_err(rpc_session_error)?
                    })
                });
                Ok(())
            })
            .await?
    }

    /// Stops accepting new invocations of this method; pending replies keep their deadline.
    pub async fn unregister_rpc_method(&self, method: String) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        self.shared
            .command(async move {
                room.local_participant().unregister_rpc_method(method);
                Ok(())
            })
            .await?
    }
}
