use prost::Message;
use server::FfiDataBuffer;
use std::{panic, sync::Arc};

use crate::{
    proto,
    server::{self, FfiConfig},
    FfiError, FfiHandleId, FFI_SERVER, INVALID_HANDLE,
};

/// # SAFTEY: The "C" callback must be threadsafe and not block
pub type FfiCallbackFn = unsafe extern "C" fn(*const u8, usize);

/// # Safety
///
/// The foreign language must only provide valid pointers
#[no_mangle]
pub unsafe extern "C" fn livekit_ffi_initialize(cb: FfiCallbackFn, capture_logs: bool) {
    FFI_SERVER.setup(FfiConfig {
        callback_fn: Arc::new(move |event| {
            let data = event.encode_to_vec();
            cb(data.as_ptr(), data.len());
        }),
        capture_logs,
    });

    log::info!("initializing ffi server v{}", env!("CARGO_PKG_VERSION"));
}

/// # Safety
///
/// The foreign language must only provide valid pointers
#[no_mangle]
pub unsafe extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> FfiHandleId {
    let res = panic::catch_unwind(|| {
        let data = unsafe { std::slice::from_raw_parts(data, len) };
        let req = match proto::FfiRequest::decode(data) {
            Ok(req) => req,
            Err(err) => {
                log::error!("failed to decode request: {:?}", err);
                return INVALID_HANDLE;
            }
        };

        let res = match server::requests::handle_request(&FFI_SERVER, req.clone()) {
            Ok(res) => res,
            Err(err) => {
                log::error!("failed to handle request {:?}: {:?}", req, err);
                return INVALID_HANDLE;
            }
        }
        .encode_to_vec();

        unsafe {
            *res_ptr = res.as_ptr();
            *res_len = res.len();
        }

        let handle_id = FFI_SERVER.next_id();
        let ffi_data = FfiDataBuffer { handle: handle_id, data: Arc::new(res) };

        FFI_SERVER.store_handle(handle_id, ffi_data);
        handle_id
    });

    match res {
        Ok(handle_id) => handle_id,
        Err(err) => {
            log::error!("panic while handling request: {:?}", err);
            FFI_SERVER.send_panic(Box::new(FfiError::InvalidRequest(
                "panic while handling request".into(),
            )));
            INVALID_HANDLE
        }
    }
}

#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FfiHandleId) -> bool {
    FFI_SERVER.drop_handle(handle_id)
}

#[cfg(target_os = "android")]
pub mod android {
    use jni::{
        sys::{jint, JNI_VERSION_1_6},
        JavaVM,
    };
    use std::os::raw::c_void;

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "C" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
        println!("JNI_OnLoad, initializing LiveKit");
        livekit::webrtc::android::initialize_android(&vm);
        JNI_VERSION_1_6
    }
}
