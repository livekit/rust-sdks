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

//! Cancel-during-handshake tests for issue 1340.
//!
//! A TCP listener that accepts and never speaks WebSocket keeps `Room::connect`
//! parked in the handshake. Disconnect on the handle returned by
//! `ConnectResponse` must abort that future, emit a connect error, and never
//! send `Panic` to the host.

use std::{net::TcpListener, sync::Arc, time::Duration};

use parking_lot::Mutex;

use crate::{
    proto,
    server::{requests, room::FfiConnectingRoom, FfiConfig},
    FFI_SERVER,
};

/// `FFI_SERVER` is process-wide; serialize these tests so event sinks do not
/// overwrite each other.
static FFI_TEST_LOCK: Mutex<()> = Mutex::new(());

fn start_blackhole() -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind blackhole listener");
    let port = listener.local_addr().expect("local_addr").port();
    let thread = std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            match stream {
                Ok(s) => held.push(s),
                Err(_) => break,
            }
        }
        drop(held);
    });
    (port, thread)
}

fn install_event_sink() -> tokio::sync::mpsc::UnboundedReceiver<proto::FfiEvent> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new(move |event| {
            let _ = tx.send(event);
        }),
        capture_logs: false,
        sdk: "test".into(),
        sdk_version: "0".into(),
    });
    rx
}

async fn recv_until(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<proto::FfiEvent>,
    timeout: Duration,
    mut pred: impl FnMut(&proto::FfiEvent) -> bool,
) -> proto::FfiEvent {
    tokio::time::timeout(timeout, async {
        loop {
            let event = rx.recv().await.expect("ffi event channel closed");
            if let Some(proto::ffi_event::Message::Panic(panic)) = &event.message {
                panic!("host Panic event: {}", panic.message);
            }
            if pred(&event) {
                return event;
            }
        }
    })
    .await
    .expect("timed out waiting for expected FFI event")
}

fn connect_blackhole(port: u16) -> proto::ConnectResponse {
    crate::server::room::FfiRoom::connect(
        &FFI_SERVER,
        proto::ConnectRequest {
            url: format!("ws://127.0.0.1:{port}"),
            token: "test".into(),
            ..Default::default()
        },
    )
}

fn disconnect_in_flight(room_handle: u64) -> u64 {
    let disconnect = requests::handle_request(
        &FFI_SERVER,
        proto::FfiRequest {
            message: Some(proto::ffi_request::Message::Disconnect(proto::DisconnectRequest {
                room_handle,
                ..Default::default()
            })),
        },
    )
    .expect("disconnect of an in-flight connect must succeed");
    match disconnect.message {
        Some(proto::ffi_response::Message::Disconnect(resp)) => resp.async_id,
        other => panic!("expected DisconnectResponse, got {other:?}"),
    }
}

fn assert_connect_cancelled(event: proto::FfiEvent, async_id: u64) {
    match event.message {
        Some(proto::ffi_event::Message::Connect(cb)) => {
            assert_eq!(cb.async_id, async_id);
            match cb.message {
                Some(proto::connect_callback::Message::Error(err)) => {
                    assert!(
                        err.to_lowercase().contains("cancel"),
                        "expected cancelled connect, got {err}"
                    );
                }
                other => panic!("expected ConnectCallback error, got {other:?}"),
            }
        }
        other => panic!("expected Connect event, got {other:?}"),
    }
}

fn assert_no_live_room(room_handle: u64) {
    assert!(
        FFI_SERVER.list_rooms().into_iter().all(|room| room.inner.handle_id != room_handle),
        "aborted connect must not leave a live FfiRoom handle"
    );
    assert!(
        FFI_SERVER.retrieve_handle::<FfiConnectingRoom>(room_handle).is_err(),
        "aborted connect must not leave an FfiConnectingRoom handle"
    );
}

#[test]
fn disconnect_aborts_in_flight_connect() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let response = connect_blackhole(port);
    let room_handle =
        response.room_handle.expect("ConnectResponse must allocate a room_handle immediately");

    let _ = disconnect_in_flight(room_handle);

    let connect_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == response.async_id
            )
        },
    ));

    assert_connect_cancelled(connect_cb, response.async_id);
    assert_no_live_room(room_handle);
}

#[test]
fn drop_handle_aborts_in_flight_connect() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let response = connect_blackhole(port);
    let room_handle =
        response.room_handle.expect("ConnectResponse must allocate a room_handle immediately");

    assert!(FFI_SERVER.drop_handle(room_handle), "drop_handle must find the connecting handle");

    let connect_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == response.async_id
            )
        },
    ));

    assert_connect_cancelled(connect_cb, response.async_id);
    assert_no_live_room(room_handle);
}

#[test]
fn disconnect_callback_waits_until_connect_settles() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let response = connect_blackhole(port);
    let room_handle =
        response.room_handle.expect("ConnectResponse must allocate a room_handle immediately");
    let disconnect_async_id = disconnect_in_flight(room_handle);

    let mut saw_connect_error = false;
    FFI_SERVER.async_runtime.block_on(recv_until(&mut events, Duration::from_secs(15), |event| {
        if let Some(proto::ffi_event::Message::Connect(cb)) = &event.message {
            if cb.async_id == response.async_id {
                match &cb.message {
                    Some(proto::connect_callback::Message::Error(err)) => {
                        assert!(
                            err.to_lowercase().contains("cancel"),
                            "expected cancelled connect, got {err}"
                        );
                        saw_connect_error = true;
                    }
                    other => panic!("expected ConnectCallback error, got {other:?}"),
                }
            }
        }
        if let Some(proto::ffi_event::Message::Disconnect(cb)) = &event.message {
            if cb.async_id == disconnect_async_id {
                assert!(
                    saw_connect_error,
                    "DisconnectCallback must arrive after ConnectCallback error \
                         (handshake actually aborted)"
                );
                assert_no_live_room(room_handle);
                return true;
            }
        }
        false
    }));
}

#[test]
fn connect_after_aborted_connect_gets_a_fresh_handle() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let first = connect_blackhole(port);
    let first_handle =
        first.room_handle.expect("ConnectResponse must allocate a room_handle immediately");
    let _ = disconnect_in_flight(first_handle);

    let first_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == first.async_id
            )
        },
    ));
    assert_connect_cancelled(first_cb, first.async_id);
    assert_no_live_room(first_handle);

    let second = connect_blackhole(port);
    let second_handle =
        second.room_handle.expect("second ConnectResponse must allocate a room_handle");
    assert_ne!(
        first_handle, second_handle,
        "a new connect after abort must get a different room_handle"
    );

    let _ = disconnect_in_flight(second_handle);
    let second_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == second.async_id
            )
        },
    ));
    assert_connect_cancelled(second_cb, second.async_id);
    assert_no_live_room(second_handle);
}

#[test]
fn dispose_cancels_in_flight_connect() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let response = connect_blackhole(port);
    let room_handle =
        response.room_handle.expect("ConnectResponse must allocate a room_handle immediately");

    // Full `dispose()` clears FFI config and would break later tests on this
    // process-wide server. `dispose` calls `cancel_connecting_rooms`; test that.
    FFI_SERVER.async_runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(15), FFI_SERVER.cancel_connecting_rooms())
            .await
            .expect("cancel_connecting_rooms timed out")
    });

    let connect_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == response.async_id
            )
        },
    ));
    assert_connect_cancelled(connect_cb, response.async_id);
    assert_no_live_room(room_handle);
}

#[test]
fn ready_for_connecting_handle_is_accepted() {
    let _lock = FFI_TEST_LOCK.lock();
    let (port, _blackhole) = start_blackhole();
    let mut events = install_event_sink();

    let response = connect_blackhole(port);
    let room_handle =
        response.room_handle.expect("ConnectResponse must allocate a room_handle immediately");

    let ready = requests::handle_request(
        &FFI_SERVER,
        proto::FfiRequest {
            message: Some(proto::ffi_request::Message::ReadyForRoomEvent(
                proto::ReadyForRoomEventRequest { room_handle, ..Default::default() },
            )),
        },
    )
    .expect("ReadyForRoomEvent on a connecting handle must succeed");
    assert!(
        matches!(ready.message, Some(proto::ffi_response::Message::ReadyForRoomEvent(_))),
        "expected ReadyForRoomEventResponse, got {:?}",
        ready.message
    );

    let _ = disconnect_in_flight(room_handle);
    let connect_cb = FFI_SERVER.async_runtime.block_on(recv_until(
        &mut events,
        Duration::from_secs(15),
        |event| {
            matches!(
                &event.message,
                Some(proto::ffi_event::Message::Connect(cb))
                    if cb.async_id == response.async_id
            )
        },
    ));
    assert_connect_cancelled(connect_cb, response.async_id);
    assert_no_live_room(room_handle);
}
