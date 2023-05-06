use crate::FfiHandleId;
use crate::{proto, server};
use lazy_static::lazy_static;
use std::sync::Mutex;

// Small FfiClient implementation used for testing
// This can be used as an example for a real implementation
mod client {
    use crate::{
        livekit_ffi_drop_handle, livekit_ffi_request, proto, FfiCallbackFn, FfiHandleId,
        INVALID_HANDLE,
    };
    use lazy_static::lazy_static;
    use prost::Message;

    lazy_static! {
        pub static ref FFI_CLIENT: FfiClient = FfiClient::default();
    }

    pub struct FfiHandle(pub FfiHandleId);

    #[derive(Default)]
    pub struct FfiClient {}

    impl FfiClient {
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
    }
}

// Used to run test one at a time
// Since we use the global server, we need to ensure that only one test is running at a time
lazy_static! {
    static ref TEST_MUTEX: Mutex<()> = Mutex::new(());
}

struct TestScope {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Default for TestScope {
    fn default() -> Self {
        let _guard = TEST_MUTEX.lock().unwrap();
        TestScope { _guard }
    }
}

impl Drop for TestScope {
    fn drop(&mut self) {
        // At the end of a test, no more handle should exist
        assert!(server::FFI_SERVER.ffi_handles().read().is_empty());
    }
}

// Create two I420Buffer, and ensure the logic is correct ( ids, and responses )
#[test]
fn create_i420_buffer() {
    let _test = TestScope::default();
    let res = client::FFI_CLIENT.send_request(proto::FfiRequest {
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

    let i420_handle = client::FfiHandle(alloc.buffer.unwrap().handle.unwrap().id as FfiHandleId);

    let res = client::FFI_CLIENT.send_request(proto::FfiRequest {
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

    client::FfiHandle(to_i420.buffer.unwrap().handle.unwrap().id as FfiHandleId);
}

#[test]
fn publish_track() {
    let _test = TestScope::default();

    client::FFI_CLIENT.initialize();
}
