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

/*
use std::time::Duration;
use livekit_token::{AccessToken, VideoGrants};
use crate::FfiHandleId;
use crate::{proto, server};
//use livekit_token::{AccessToken, VideoGrants};

// Small FfiClient implementation used for testing
// This can be used as an example for a real implementation
mod client {
    use crate::{
        livekit_ffi_drop_handle, livekit_ffi_request, proto, FfiCallbackFn, FfiHandleId,
        INVALID_HANDLE,
    };
    use lazy_static::lazy_static;
    use parking_lot::Mutex;
    use prost::Message;
    use tokio::sync::mpsc;

    lazy_static! {
        static ref EVENT_TX: Mutex<Option<mpsc::UnboundedSender<proto::ffi_event::Message>>> =
            Default::default();
        pub static ref FFI_CLIENT: Mutex<FfiClient> = Default::default();
    }

    pub struct FfiHandle(pub FfiHandleId);

    pub struct FfiClient {
        event_rx: mpsc::UnboundedReceiver<proto::ffi_event::Message>,
    }

    impl Default for FfiClient {
        fn default() -> Self {
            let (event_tx, event_rx) = mpsc::unbounded_channel();
            *EVENT_TX.lock() = Some(event_tx);
            Self { event_rx }
        }
    }

    impl FfiClient {
        pub async fn recv_event(&mut self) -> proto::ffi_event::Message {
            self.event_rx.recv().await.unwrap()
        }

        pub fn initialize(&self) {
            self.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::Initialize(
                    proto::InitializeRequest {
                        event_callback_ptr: test_events_callback as FfiCallbackFn as u64,
                    },
                )),
            });
        }

        pub fn send_request(&self, request: proto::FfiRequest) -> proto::FfiResponse {
            let data = request.encode_to_vec();

            let mut res_ptr: Box<*const u8> = Box::new(std::ptr::null());
            let mut res_len: Box<usize> = Box::new(0);

            let handle = livekit_ffi_request(
                data.as_ptr(),
                data.len(),
                res_ptr.as_mut(),
                res_len.as_mut(),
            );
            let handle = FfiHandle(handle); // drop at end of scope

            let res = unsafe {
                assert_ne!(handle.0, INVALID_HANDLE);
                assert_ne!(*res_ptr, std::ptr::null());
                assert_ne!(*res_len, 0);
                std::slice::from_raw_parts(*res_ptr, *res_len)
            };

            proto::FfiResponse::decode(res).unwrap()
        }
    }

    impl Drop for FfiHandle {
        fn drop(&mut self) {
            assert!(livekit_ffi_drop_handle(self.0));
        }
    }

    #[no_mangle]
    unsafe extern "C" fn test_events_callback(data_ptr: *const u8, len: usize) {
        let data = unsafe { std::slice::from_raw_parts(data_ptr, len) };
        let event = proto::FfiEvent::decode(data).unwrap();
        EVENT_TX
            .lock()
            .as_ref()
            .unwrap()
            .send(event.message.unwrap())
            .unwrap();
    }
}

struct TestScope {}

impl TestScope {
    fn new() -> (Self, parking_lot::MutexGuard<'static, client::FfiClient>) {
        // Run one test at a time
        let client = client::FFI_CLIENT.lock();

        (TestScope {}, client)
    }
}

impl Drop for TestScope {
    fn drop(&mut self) {
        // At the end of a test, no more handle should exist
        assert!(server::FFI_SERVER.ffi_handles.is_empty());
    }
}

fn test_env() -> (String, String, String) {
    let lk_url = std::env::var("LK_TEST_URL").expect("LK_TEST_URL isn't set");
    let lk_api_key = std::env::var("LK_TEST_API_KEY").expect("LK_TEST_API_KEY isn't set");
    let lk_api_secret = std::env::var("LK_TEST_API_SECRET").expect("LK_TEST_API_SECRET isn't set");
    (lk_url, lk_api_key, lk_api_secret)
}

macro_rules! wait_for_event {
    ($client:ident, $variant:ident, $timeout:expr) => {
        tokio::time::timeout(Duration::from_secs($timeout), async {
            loop {
                let event = $client.recv_event().await;
                if let proto::ffi_event::Message::$variant(event) = event {
                    return event;
                }
            }
        })
    };
}

#[test]
fn create_i420_buffer() {
    let (_test, client) = TestScope::new();

    // Create a new I420Buffer
    let res = client.send_request(proto::FfiRequest {
        message: Some(proto::ffi_request::Message::AllocVideoBuffer(
            proto::AllocVideoBufferRequest {
                r#type: proto::VideoFrameBufferType::I420 as i32,
                width: 640,
                height: 480,
            },
        )),
    });

    let proto::ffi_response::Message::AllocVideoBuffer(alloc) = res.message.unwrap() else {
        panic!("unexpected response");
    };

    // Convert to I420 (copy/no-op)
    let i420_handle = client::FfiHandle(alloc.buffer.unwrap().handle.unwrap().id as FfiHandleId);

    let res = client.send_request(proto::FfiRequest {
        message: Some(proto::ffi_request::Message::ToI420(proto::ToI420Request {
            flip_y: false,
            from: Some(proto::to_i420_request::From::Buffer(proto::FfiHandleId {
                id: i420_handle.0 as u64,
            })),
        })),
    });

    let proto::ffi_response::Message::ToI420(to_i420) = res.message.unwrap() else {
        panic!("unexpected response");
    };

    // Make sure to drop the handles
    client::FfiHandle(to_i420.buffer.unwrap().handle.unwrap().id as FfiHandleId);
}

#[test]
#[ignore] // Ignore for now ( need to setup GHA )
fn publish_video_track() {
    let (_test, mut client) = TestScope::new();
    let (lk_url, lk_api_key, lk_api_secret) = test_env();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            client.initialize();

            let token = AccessToken::with_api_key(&lk_api_key, &lk_api_secret)
                .with_grants(VideoGrants {
                    room: "livekit-ffi-test".to_string(),
                    ..Default::default()
                })
                .with_identity("video_test")
                .to_jwt()
                .unwrap();

            // Connect to the room
            client.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::Connect(
                    proto::ConnectRequest {
                        url: lk_url.clone(),
                        token,
                        ..Default::default()
                    },
                )),
            });

            let connect = wait_for_event!(client, Connect, 5).await.unwrap();
            assert!(connect.error.is_none());

            let room_handle =
                client::FfiHandle(connect.room.unwrap().handle.unwrap().id as FfiHandleId);

            // Create a new VideoSource
            const VIDEO_WIDTH: u32 = 640;
            const VIDEO_HEIGHT: u32 = 480;
            const VIDEO_FPS: f64 = 8.0;

            let res = client.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::NewVideoSource(
                    proto::NewVideoSourceRequest {
                        r#type: proto::VideoSourceType::VideoSourceNative as i32,
                        resolution: Some(proto::VideoSourceResolution {
                            width: VIDEO_WIDTH,
                            height: VIDEO_HEIGHT,
                        }),
                    },
                )),
            });

            let proto::ffi_response::Message::NewVideoSource(new_video_source) =
                res.message.unwrap() else {
                panic!("unexpected response");
            };

            let source_handle = client::FfiHandle(
                new_video_source.source.unwrap().handle.unwrap().id as FfiHandleId,
            );

            // Create a new VideoTrack
            let res = client.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::CreateVideoTrack(
                    proto::CreateVideoTrackRequest {
                        name: "video_test".to_string(),
                        source_handle: Some(proto::FfiHandleId {
                            id: source_handle.0 as u64,
                        }),
                    },
                )),
            });

            let proto::ffi_response::Message::CreateVideoTrack(create_video_track) =
                res.message.unwrap() else {
                panic!("unexpected response");
            };

            let track_handle = client::FfiHandle(
                create_video_track.track.unwrap().handle.unwrap().id as FfiHandleId,
            );

            let publish_options = proto::TrackPublishOptions {
                video_codec: proto::VideoCodec::H264 as i32,
                source: proto::TrackSource::SourceCamera as i32,
                ..Default::default()
            };

            // Publish the VideoTrack
            client.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::PublishTrack(
                    proto::PublishTrackRequest {
                        room_handle: Some(proto::FfiHandleId {
                            id: room_handle.0 as u64,
                        }),
                        track_handle: Some(proto::FfiHandleId {
                            id: track_handle.0 as u64,
                        }),
                        options: Some(publish_options),
                    },
                )),
            });

            let publish_track = wait_for_event!(client, PublishTrack, 5).await.unwrap();
            assert!(publish_track.error.is_none());

            // Send red frames
            let rgba: Vec<u32> = vec![0xff0000ff; (VIDEO_WIDTH * VIDEO_HEIGHT) as usize];
            let res = client.send_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::ToI420(proto::ToI420Request {
                    flip_y: false,
                    from: Some(proto::to_i420_request::From::Argb(proto::ArgbBufferInfo {
                        ptr: rgba.as_ptr() as u64,
                        format: proto::VideoFormatType::FormatAbgr as i32,
                        width: VIDEO_WIDTH,
                        height: VIDEO_HEIGHT,
                        stride: VIDEO_WIDTH * 4,
                    })),
                })),
            });

            let proto::ffi_response::Message::ToI420(to_i420) = res.message.unwrap() else {
                panic!("unexpected response");
            };

            let buffer_handle =
                client::FfiHandle(to_i420.buffer.unwrap().handle.unwrap().id as FfiHandleId);

            // 2 seconds
            for _ in 0..16 {
                client.send_request(proto::FfiRequest {
                    message: Some(proto::ffi_request::Message::CaptureVideoFrame(
                        proto::CaptureVideoFrameRequest {
                            source_handle: Some(proto::FfiHandleId {
                                id: source_handle.0 as u64,
                            }),
                            buffer_handle: Some(proto::FfiHandleId {
                                id: buffer_handle.0 as u64,
                            }),
                            frame: Some(proto::VideoFrameInfo {
                                timestamp_us: 0,
                                rotation: proto::VideoRotation::VideoRotation0 as i32,
                            }),
                        },
                    )),
                });

                tokio::time::sleep(std::time::Duration::from_millis(1000 / VIDEO_FPS as u64)).await;
            }
        })
}
*/

use std::{
    panic,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Barrier,
    },
    thread,
    time::Duration,
};

use parking_lot::Mutex;

use super::{FfiConfig, FfiHandle};
use crate::{proto, FfiError, FfiHandleId, FFI_SERVER};

/// Serializes the tests that dispose the shared server.
static DISPOSE_LOCK: Mutex<()> = Mutex::new(());

struct DropsHandle {
    handle: FfiHandleId,
    did_attempt_drop: Arc<AtomicBool>,
    drop_count: Arc<AtomicUsize>,
}

impl FfiHandle for DropsHandle {}

impl Drop for DropsHandle {
    fn drop(&mut self) {
        self.did_attempt_drop.store(true, Ordering::SeqCst);
        self.drop_count.fetch_add(1, Ordering::SeqCst);
        let _ = FFI_SERVER.drop_handle(self.handle);
    }
}

/// `dispose` empties the whole handle map, and `FFI_SERVER` is a process-wide
/// static, so this test is destructive to every other test holding a handle.
/// It runs alone. Every live test that stores a handle carries a `serial_test`
/// marker to stay out of that window — `parallel` where it only needs to avoid
/// this test, `serial` for the resampler tests, which also fail among
/// themselves. Mark new ones the same way.
#[serial_test::serial]
#[test]
fn dispose_cleans_up_resources() {
    let _dispose = DISPOSE_LOCK.lock();
    let did_attempt_drop = Arc::new(AtomicBool::new(false));
    let drop_count = Arc::new(AtomicUsize::new(0));
    let child_handle = FFI_SERVER.next_id();
    let first_parent_handle = FFI_SERVER.next_id();
    let second_parent_handle = FFI_SERVER.next_id();

    // Configure the server so disposal must also clear its callback state.
    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new(|_| {}),
        capture_logs: false,
        sdk: "livekit-ffi-test".to_owned(),
        sdk_version: "test".to_owned(),
    });
    assert!(FFI_SERVER.is_setup());

    // Parent drops re-enter drop_handle for this child, matching nested FFI
    // resource cleanup during shutdown.
    FFI_SERVER.store_handle(child_handle, ());
    let mut child_dropped_rx = FFI_SERVER.watch_handle_dropped(child_handle);
    FFI_SERVER.store_handle(
        first_parent_handle,
        DropsHandle {
            handle: child_handle,
            did_attempt_drop: did_attempt_drop.clone(),
            drop_count: drop_count.clone(),
        },
    );
    FFI_SERVER.store_handle(
        second_parent_handle,
        DropsHandle {
            handle: child_handle,
            did_attempt_drop: did_attempt_drop.clone(),
            drop_count: drop_count.clone(),
        },
    );

    // Dispose must release handles, cancel their watchers, and clear config.
    FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());

    assert!(did_attempt_drop.load(Ordering::SeqCst));
    assert_eq!(drop_count.load(Ordering::SeqCst), 2);
    assert!(FFI_SERVER.ffi_handles.is_empty());
    assert!(!FFI_SERVER.is_setup());
    assert!(matches!(
        child_dropped_rx.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Closed)
    ));

    // A repeated shutdown is a no-op after the first cleanup.
    FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
    assert!(FFI_SERVER.ffi_handles.is_empty());
    assert!(!FFI_SERVER.is_setup());
}

#[test]
fn dispose_waits_for_callback_in_progress() {
    let _dispose = DISPOSE_LOCK.lock();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let calls = Arc::new(AtomicUsize::new(0));
    let returned = Arc::new(AtomicBool::new(false));

    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new({
            let calls = calls.clone();
            let returned = returned.clone();
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                let _ = entered_tx.send(());
                let _ = release_rx.lock().recv();
                returned.store(true, Ordering::SeqCst);
            }
        }),
        capture_logs: false,
        sdk: "livekit-ffi-test".to_owned(),
        sdk_version: "test".to_owned(),
    });

    // A worker is inside the client callback when the client disposes.
    let sender = thread::spawn(|| {
        FFI_SERVER.send_event(proto::Panic { message: "in flight".to_owned() }.into())
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).expect("callback was not called");

    let (disposed_tx, disposed_rx) = mpsc::channel();
    let disposer = thread::spawn({
        let returned = returned.clone();
        move || {
            FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
            let _ = disposed_tx.send(returned.load(Ordering::SeqCst));
        }
    });

    // Dispose must not return while the callback is still running.
    assert!(disposed_rx.recv_timeout(Duration::from_millis(500)).is_err());

    release_tx.send(()).unwrap();
    let callback_had_returned =
        disposed_rx.recv_timeout(Duration::from_secs(5)).expect("dispose did not return");
    assert!(callback_had_returned);
    disposer.join().unwrap();
    assert!(sender.join().unwrap().is_ok());

    // No callback starts after dispose.
    assert!(matches!(
        FFI_SERVER.send_event(proto::Panic { message: "after dispose".to_owned() }.into()),
        Err(FfiError::NotConfigured)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

fn test_config(callback: impl Fn(proto::FfiEvent) + Send + Sync + 'static) -> FfiConfig {
    FfiConfig {
        callback_fn: Arc::new(callback),
        capture_logs: false,
        sdk: "livekit-ffi-test".to_owned(),
        sdk_version: "test".to_owned(),
    }
}

/// Configures the server with a callback that blocks until released, and
/// sends one event from another thread. Returns once that callback is running.
fn start_blocking_callback() -> mpsc::Sender<()> {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    FFI_SERVER.setup(test_config(move |_| {
        let _ = entered_tx.send(());
        let _ = release_rx.lock().recv();
    }));
    thread::spawn(|| FFI_SERVER.send_event(proto::Panic { message: "blocking".to_owned() }.into()));
    entered_rx.recv_timeout(Duration::from_secs(5)).expect("callback was not called");
    release_tx
}

fn spawn_dispose() -> mpsc::Receiver<()> {
    let (disposed_tx, disposed_rx) = mpsc::channel();
    thread::spawn(move || {
        FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
        let _ = disposed_tx.send(());
    });
    disposed_rx
}

#[test]
fn dispose_keeps_waiting_for_a_slow_callback() {
    let _dispose = DISPOSE_LOCK.lock();
    let release_tx = start_blocking_callback();
    let disposed_rx = spawn_dispose();

    // Past the point where dispose warns, it is still waiting.
    let past_warning = super::CALLBACK_DRAIN_WARN_INTERVAL + Duration::from_secs(1);
    assert!(disposed_rx.recv_timeout(past_warning).is_err());

    release_tx.send(()).unwrap();
    disposed_rx.recv_timeout(Duration::from_secs(5)).expect("dispose did not return");
}

#[test]
fn dispose_from_inside_a_callback_does_not_wait_for_itself() {
    let _dispose = DISPOSE_LOCK.lock();
    FFI_SERVER.setup(test_config(|_| FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose())));

    let (sent_tx, sent_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = FFI_SERVER.send_event(proto::Panic { message: "dispose".to_owned() }.into());
        let _ = sent_tx.send(result.is_ok());
    });

    let sent = sent_rx.recv_timeout(Duration::from_secs(2)).expect("dispose waited for itself");
    assert!(sent);
    assert!(!FFI_SERVER.is_setup());
}

#[test]
fn setup_waits_until_dispose_returns() {
    let _dispose = DISPOSE_LOCK.lock();
    let release_tx = start_blocking_callback();
    let disposed_rx = spawn_dispose();

    // Dispose has cleared the callback and is waiting for the one in progress.
    while FFI_SERVER.is_setup() {
        thread::sleep(Duration::from_millis(1));
    }
    let (setup_tx, setup_rx) = mpsc::channel();
    thread::spawn(move || {
        FFI_SERVER.setup(test_config(|_| {}));
        let _ = setup_tx.send(());
    });

    assert!(setup_rx.recv_timeout(Duration::from_millis(500)).is_err());
    assert!(!FFI_SERVER.is_setup());

    release_tx.send(()).unwrap();
    disposed_rx.recv_timeout(Duration::from_secs(5)).expect("dispose did not return");
    setup_rx.recv_timeout(Duration::from_secs(5)).expect("setup did not return");
    assert!(FFI_SERVER.is_setup());
}

fn send_from_thread(done_tx: &mpsc::Sender<bool>) {
    let done_tx = done_tx.clone();
    thread::spawn(move || {
        let result = FFI_SERVER.send_event(proto::Panic { message: "send".to_owned() }.into());
        let _ = done_tx.send(result.is_ok());
    });
}

#[test]
fn callbacks_that_dispose_at_the_same_time_do_not_deadlock() {
    let _dispose = DISPOSE_LOCK.lock();
    let both_running = Arc::new(Barrier::new(2));
    FFI_SERVER.setup(test_config(move |_| {
        both_running.wait();
        FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
    }));

    let (done_tx, done_rx) = mpsc::channel();
    send_from_thread(&done_tx);
    send_from_thread(&done_tx);

    for _ in 0..2 {
        assert!(done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("disposing callbacks deadlocked"));
    }
    assert!(!FFI_SERVER.is_setup());
}

#[test]
fn a_callback_that_disposes_does_not_wait_for_a_callback_it_blocks() {
    let _dispose = DISPOSE_LOCK.lock();
    let host_lock = Arc::new(Mutex::new(()));
    let calls = AtomicUsize::new(0);
    let (locked_tx, locked_rx) = mpsc::channel();
    let (second_tx, second_rx) = mpsc::channel();
    let second_rx = Mutex::new(second_rx);
    FFI_SERVER.setup(test_config(move |_| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            // Holds a host lock across dispose, while the second callback waits for it.
            let _host = host_lock.lock();
            let _ = locked_tx.send(());
            let _ = second_rx.lock().recv();
            FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose());
        } else {
            let _ = second_tx.send(());
            let _host = host_lock.lock();
        }
    }));

    let (done_tx, done_rx) = mpsc::channel();
    send_from_thread(&done_tx);
    locked_rx.recv_timeout(Duration::from_secs(5)).expect("first callback was not called");
    send_from_thread(&done_tx);

    for _ in 0..2 {
        assert!(done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("dispose waited on a blocked callback"));
    }
    assert!(!FFI_SERVER.is_setup());
}

#[test]
fn setup_from_a_callback_while_dispose_drains_is_ignored() {
    let _dispose = DISPOSE_LOCK.lock();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let go_rx = Mutex::new(go_rx);
    FFI_SERVER.setup(test_config(move |_| {
        let _ = entered_tx.send(());
        let _ = go_rx.lock().recv();
        FFI_SERVER.setup(test_config(|_| {}));
    }));

    let (done_tx, done_rx) = mpsc::channel();
    send_from_thread(&done_tx);
    entered_rx.recv_timeout(Duration::from_secs(5)).expect("callback was not called");
    let disposed_rx = spawn_dispose();
    while FFI_SERVER.is_setup() {
        thread::sleep(Duration::from_millis(1));
    }

    // Dispose is draining and waits for this callback, so its setup must not wait for dispose.
    go_tx.send(()).unwrap();
    assert!(done_rx.recv_timeout(Duration::from_secs(5)).expect("setup waited for dispose"));
    disposed_rx.recv_timeout(Duration::from_secs(5)).expect("dispose did not return");
    assert!(!FFI_SERVER.is_setup());
}

struct PanicsOnDrop;

impl FfiHandle for PanicsOnDrop {}

impl Drop for PanicsOnDrop {
    fn drop(&mut self) {
        panic!("handle drop panicked");
    }
}

#[test]
fn setup_does_not_wait_for_a_dispose_that_panicked() {
    let _dispose = DISPOSE_LOCK.lock();
    FFI_SERVER.setup(test_config(|_| {}));
    FFI_SERVER.store_handle(FFI_SERVER.next_id(), PanicsOnDrop);

    // The Dispose request runs inside the catch_unwind of livekit_ffi_request.
    let disposed = panic::catch_unwind(|| FFI_SERVER.async_runtime.block_on(FFI_SERVER.dispose()));
    assert!(disposed.is_err());

    let (setup_tx, setup_rx) = mpsc::channel();
    thread::spawn(move || {
        FFI_SERVER.setup(test_config(|_| {}));
        let _ = setup_tx.send(());
    });
    setup_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("setup waited for a dispose that panicked");
    assert!(FFI_SERVER.is_setup());
}
