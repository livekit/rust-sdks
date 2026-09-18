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

//! Lifecycle tests for RPC methods registered through the FFI layer.
//!
//! These connect to a real LiveKit server, so they are gated behind `__lk-e2e-test`
//! exactly like the equivalent suites in the `livekit` crate, and expect a
//! `livekit-server --dev` on the usual development endpoint.
//!
//! `FFI_SERVER` is a process-wide singleton that each test configures and disposes, so
//! every test here is `#[serial]`.

use std::{
    env,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use livekit_token::{AccessToken, VideoGrants};
use serial_test::serial;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::{participant::FfiParticipant, room::FfiRoom, FfiConfig};
use crate::{proto, FfiHandleId, FFI_SERVER};

struct TestEnvironment {
    api_key: String,
    api_secret: String,
    server_url: String,
}

impl TestEnvironment {
    fn from_env_or_defaults() -> Self {
        Self {
            api_key: env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into()),
            api_secret: env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into()),
            server_url: env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into()),
        }
    }

    fn token(&self, room: &str, identity: &str) -> String {
        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_ttl(Duration::from_secs(10 * 60))
            .with_grants(VideoGrants {
                room_join: true,
                room: room.to_owned(),
                ..Default::default()
            })
            .with_identity(identity)
            .with_name(identity)
            .to_jwt()
            .expect("failed to generate a join token")
    }
}

fn unique_room_name() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("ffi_rpc_lifecycle_{nanos}")
}

/// Configures the FFI server and returns the stream of events it emits.
fn setup_server() -> UnboundedReceiver<proto::FfiEvent> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new(move |event| {
            let _ = event_tx.send(event);
        }),
        capture_logs: false,
        sdk: "livekit-ffi-test".to_owned(),
        sdk_version: "test".to_owned(),
    });
    event_rx
}

/// Connects a room through the FFI layer and returns its room and local participant
/// handles, with the room-event ready handshake already completed.
///
/// The handshake matters for lifecycle assertions: the connect task parks on it and
/// holds the room while it waits, so leaving it pending would keep the room alive for
/// reasons unrelated to what is being measured.
async fn connect_room(
    events: &mut UnboundedReceiver<proto::FfiEvent>,
    url: &str,
    token: &str,
) -> (FfiHandleId, FfiHandleId) {
    FfiRoom::connect(
        &FFI_SERVER,
        proto::ConnectRequest {
            url: url.to_owned(),
            token: token.to_owned(),
            options: proto::RoomOptions::default(),
            request_async_id: None,
        },
    );

    let handles = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event = events.recv().await.expect("the ffi event stream closed");
            let Some(proto::ffi_event::Message::Connect(connect)) = event.message else { continue };
            match connect.message {
                Some(proto::connect_callback::Message::Result(result)) => {
                    break (result.room.handle.id, result.local_participant.handle.id)
                }
                Some(proto::connect_callback::Message::Error(err)) => {
                    panic!("failed to connect to the room: {err}")
                }
                None => panic!("the connect callback carried no result"),
            }
        }
    })
    .await
    .expect("timed out waiting for the connect callback");

    FFI_SERVER
        .retrieve_handle::<FfiRoom>(handles.0)
        .expect("no room handle")
        .ready_for_room_event();

    handles
}

/// Registering an RPC method must not make the room impossible to release.
///
/// The handler is stored on the room's own RPC server: `RoomInner` owns the
/// `livekit::Room`, which owns the `RoomSession`, which owns the handler map. A handler
/// that captures `Arc<RoomInner>` therefore closes a cycle back onto the object that
/// transitively stores it, and nothing unregisters the method during teardown — neither
/// `FfiRoom::close` nor `FfiServer::dispose` touches the SDK's handler map. The room then
/// outlives `dispose()`, keeping the engine, its peer connections and the WebRTC runtime
/// resident for the rest of the process, which is precisely what `dispose()` exists to
/// prevent.
#[test]
#[serial]
fn registering_an_rpc_method_does_not_retain_the_room() {
    let test_env = TestEnvironment::from_env_or_defaults();
    let room_name = unique_room_name();
    let token = test_env.token(&room_name, "p0");
    let mut events = setup_server();

    let room_dropped = FFI_SERVER.async_runtime.block_on(async {
        let (room_handle, participant_handle) =
            connect_room(&mut events, &test_env.server_url, &token).await;

        let room_dropped = FFI_SERVER
            .retrieve_handle::<FfiRoom>(room_handle)
            .expect("no room handle")
            .drop_probe();

        FFI_SERVER
            .retrieve_handle::<FfiParticipant>(participant_handle)
            .expect("no local participant handle")
            .register_rpc_method(
                &FFI_SERVER,
                proto::RegisterRpcMethodRequest {
                    local_participant_handle: participant_handle,
                    method: "lifecycle-probe".to_owned(),
                },
            )
            .expect("failed to register the rpc method");

        FFI_SERVER.dispose().await;
        room_dropped
    });

    assert!(FFI_SERVER.ffi_handles.is_empty(), "dispose left handles behind");
    assert!(room_dropped(), "the registered RPC handler retained the room after dispose");
}

/// An RPC invocation still awaiting its FFI response must not survive disposal.
///
/// Making the handler capture the room weakly only breaks the *idle* cycle. Each accepted
/// invocation upgrades that weak reference to a strong `Arc<RoomInner>`, stores its
/// responder in the room and parks on the matching receiver with no timeout. Disposal
/// removes the client's handles, so the response can never arrive: unless teardown drains
/// the waiters, the handler stays pending forever and keeps the room alive exactly as the
/// idle cycle used to.
#[test]
#[serial]
fn an_unanswered_rpc_invocation_does_not_retain_the_room() {
    let test_env = TestEnvironment::from_env_or_defaults();
    let room_name = unique_room_name();
    let callee_token = test_env.token(&room_name, "callee");
    let caller_token = test_env.token(&room_name, "caller");
    let mut events = setup_server();

    let callee_room_dropped = FFI_SERVER.async_runtime.block_on(async {
        let (callee_room, callee_participant) =
            connect_room(&mut events, &test_env.server_url, &callee_token).await;
        let (_caller_room, caller_participant) =
            connect_room(&mut events, &test_env.server_url, &caller_token).await;

        let callee_room_dropped = FFI_SERVER
            .retrieve_handle::<FfiRoom>(callee_room)
            .expect("no room handle")
            .drop_probe();

        FFI_SERVER
            .retrieve_handle::<FfiParticipant>(callee_participant)
            .expect("no callee participant handle")
            .register_rpc_method(
                &FFI_SERVER,
                proto::RegisterRpcMethodRequest {
                    local_participant_handle: callee_participant,
                    method: "never-answered".to_owned(),
                },
            )
            .expect("failed to register the rpc method");

        FFI_SERVER
            .retrieve_handle::<FfiParticipant>(caller_participant)
            .expect("no caller participant handle")
            .perform_rpc(
                &FFI_SERVER,
                proto::PerformRpcRequest {
                    local_participant_handle: caller_participant,
                    destination_identity: "callee".to_owned(),
                    method: "never-answered".to_owned(),
                    payload: "ping".to_owned(),
                    // Long enough that the caller's own timeout cannot be what releases
                    // the room, which would make this assertion meaningless.
                    response_timeout_ms: Some(120_000),
                    request_async_id: None,
                    max_round_trip_latency_ms: None,
                },
            )
            .expect("failed to perform the rpc");

        // Wait until the callee's handler has actually parked on its responder: at this
        // point it holds a strong reference to the room.
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let event = events.recv().await.expect("the ffi event stream closed");
                if let Some(proto::ffi_event::Message::RpcMethodInvocation(invocation)) =
                    event.message
                {
                    assert_eq!(invocation.method, "never-answered");
                    break;
                }
            }
        })
        .await
        .expect("timed out waiting for the rpc invocation to reach the callee");

        // Deliberately never send an RpcMethodInvocationResponse.
        FFI_SERVER.dispose().await;
        callee_room_dropped
    });

    assert!(FFI_SERVER.ffi_handles.is_empty(), "dispose left handles behind");
    assert!(
        callee_room_dropped(),
        "an RPC invocation awaiting a response that will never arrive retained the room"
    );
}
