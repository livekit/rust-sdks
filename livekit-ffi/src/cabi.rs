use crate::{
    proto,
    server::{self, FfiConfig},
    FfiHandleId, FFI_SERVER,
};
use prost::Message;
use server::FfiDataBuffer;
use std::sync::Arc;

/// # SAFTEY: The "C" callback must be threadsafe and not block
pub type FfiCallbackFn = unsafe extern "C" fn(*const u8, usize);

/// # Safety
///
/// The foreign language must only provide valid pointers
#[no_mangle]
pub unsafe extern "C" fn livekit_initialize(cb: FfiCallbackFn, capture_logs: bool) {
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
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let res = match proto::FfiRequest::decode(data) {
        Ok(res) => res,
        Err(err) => {
            panic!("failed to decode request: {}", err);
        }
    };

    let res = match server::requests::handle_request(&FFI_SERVER, res) {
        Ok(res) => res,
        Err(err) => {
            panic!("failed to handle request: {}", err);
        }
    }
    .encode_to_vec();

    unsafe {
        *res_ptr = res.as_ptr();
        *res_len = res.len();
    }

    let handle_id = FFI_SERVER.next_id();
    let ffi_data = FfiDataBuffer {
        handle: handle_id,
        data: Arc::new(res),
    };

    FFI_SERVER.store_handle(handle_id, ffi_data);
    handle_id
}

#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FfiHandleId) -> bool {
    FFI_SERVER.drop_handle(handle_id)
}
