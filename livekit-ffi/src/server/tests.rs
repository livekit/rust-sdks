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
use livekit_api::access_token::{AccessToken, VideoGrants};
use crate::FfiHandleId;
use crate::{proto, server};
//use livekit_api::access_token::{AccessToken, VideoGrants};

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
