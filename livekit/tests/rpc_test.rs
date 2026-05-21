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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{Context, Result},
    common::test_rooms,
    livekit::{prelude::PerformRpcData, RoomEvent},
    std::time::Duration,
    tokio::sync::mpsc::UnboundedReceiver,
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_invocation() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (caller_room, _) = rooms.pop().unwrap();
    let (callee_room, _) = rooms.pop().unwrap();
    let callee_identity = callee_room.local_participant().identity();

    const METHOD_NAME: &str = "test-method";
    const PAYLOAD: &str = "test-payload";

    callee_room.local_participant().register_rpc_method(METHOD_NAME.to_string(), |data| {
        // Echo caller payload as return value
        Box::pin(async move { Ok(data.payload.to_string()) })
    });

    let perform_data = PerformRpcData {
        method: METHOD_NAME.to_string(),
        destination_identity: callee_identity.to_string(),
        payload: PAYLOAD.to_string(),
        response_timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let return_payload = caller_room
        .local_participant()
        .perform_rpc(perform_data)
        .await
        .context("Invocation failed")?;
    assert_eq!(return_payload, PAYLOAD, "Unexpected return value");
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_unregistered() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (caller_room, _) = rooms.pop().unwrap();
    let (callee_room, _) = rooms.pop().unwrap();
    let callee_identity = callee_room.local_participant().identity();

    const METHOD_NAME: &str = "unregistered-method";
    const PAYLOAD: &str = "test-payload";

    let perform_data = PerformRpcData {
        method: METHOD_NAME.to_string(),
        destination_identity: callee_identity.to_string(),
        payload: PAYLOAD.to_string(),
        response_timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let result = caller_room.local_participant().perform_rpc(perform_data).await;
    assert!(result.is_err(), "Expected error");
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_large_payload() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (caller_room, _) = rooms.pop().unwrap();
    let (callee_room, _) = rooms.pop().unwrap();
    let callee_identity = callee_room.local_participant().identity();

    const METHOD_NAME: &str = "large-payload-method";
    // 20KB payload - exceeds 15KB v1 limit but works with v2 data streams
    let large_payload: String = "x".repeat(20_000);

    callee_room.local_participant().register_rpc_method(METHOD_NAME.to_string(), |data| {
        Box::pin(async move { Ok(data.payload.to_string()) })
    });

    let perform_data = PerformRpcData {
        method: METHOD_NAME.to_string(),
        destination_identity: callee_identity.to_string(),
        payload: large_payload.clone(),
        response_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    let return_payload = caller_room
        .local_participant()
        .perform_rpc(perform_data)
        .await
        .context("Large payload invocation failed")?;
    assert_eq!(return_payload, large_payload, "Large payload mismatch");
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_error_response() -> Result<()> {
    use livekit::prelude::{RpcError, RpcErrorCode};

    let mut rooms = test_rooms(2).await?;
    let (caller_room, _) = rooms.pop().unwrap();
    let (callee_room, _) = rooms.pop().unwrap();
    let callee_identity = callee_room.local_participant().identity();

    const METHOD_NAME: &str = "error-method";

    callee_room.local_participant().register_rpc_method(METHOD_NAME.to_string(), |_data| {
        Box::pin(async move {
            Err(RpcError::new(42, "custom error".to_string(), Some("error data".to_string())))
        })
    });

    let perform_data = PerformRpcData {
        method: METHOD_NAME.to_string(),
        destination_identity: callee_identity.to_string(),
        payload: "test".to_string(),
        response_timeout: Duration::from_secs(5),
        ..Default::default()
    };
    let result = caller_room.local_participant().perform_rpc(perform_data).await;
    assert!(result.is_err(), "Expected error response");
    let err = result.unwrap_err();
    assert_eq!(err.code, 42, "Error code mismatch");
    assert_eq!(err.message, "custom error", "Error message mismatch");
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_unknown_destination() -> Result<()> {
    let mut rooms = test_rooms(1).await?;
    let (caller_room, _) = rooms.pop().unwrap();

    let perform_data = PerformRpcData {
        method: "unregistered-method".to_string(),
        destination_identity: "unknown-participant".to_string(),
        payload: "test-payload".to_string(),
        response_timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let result = caller_room.local_participant().perform_rpc(perform_data).await;
    assert!(result.is_err(), "Expected error");
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
pub async fn test_rpc_v2_does_not_emit_data_stream_events() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (caller_room, caller_events) = rooms.pop().unwrap();
    let (callee_room, callee_events) = rooms.pop().unwrap();
    let callee_identity = callee_room.local_participant().identity();

    callee_room
        .local_participant()
        .register_rpc_method("echo".to_string(), |data| Box::pin(async move { Ok(data.payload) }));

    async fn collect(mut rx: UnboundedReceiver<RoomEvent>) -> Vec<RoomEvent> {
        let mut events = Vec::new();
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
            events.push(ev);
        }
        events
    }
    let caller_handle = tokio::spawn(collect(caller_events));
    let callee_handle = tokio::spawn(collect(callee_events));

    let perform_data = PerformRpcData {
        method: "echo".to_string(),
        destination_identity: callee_identity.to_string(),
        payload: "hi".to_string(),
        response_timeout: Duration::from_millis(500),
        ..Default::default()
    };
    let resp = caller_room
        .local_participant()
        .perform_rpc(perform_data)
        .await
        .context("Invocation failed")?;
    assert_eq!(resp, "hi");

    let caller = caller_handle.await?;
    let callee = callee_handle.await?;
    for ev in caller.iter().chain(callee.iter()) {
        assert!(
            !matches!(
                ev,
                RoomEvent::TextStreamOpened { .. }
                    | RoomEvent::ByteStreamOpened { .. }
                    | RoomEvent::StreamHeaderReceived { .. }
                    | RoomEvent::StreamChunkReceived { .. }
                    | RoomEvent::StreamTrailerReceived { .. }
            ),
            "unexpected data-stream event leaked from internal RPC traffic: {:?}",
            ev,
        );
    }
    Ok(())
}
