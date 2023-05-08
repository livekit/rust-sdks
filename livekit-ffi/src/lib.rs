use livekit::prelude::*;
use prost::Message;
use std::any::Any;
use thiserror::Error;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}
mod conversion;
mod server;

#[derive(Error, Debug)]
pub enum FfiError {
    #[error("the server is not configured")]
    NotConfigured,
    #[error("the server is already initialized")]
    AlreadyInitialized,
    #[error("room error {0}")]
    Room(#[from] RoomError),
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
}

/// # SAFTEY: The "C" callback must be threadsafe and not block
pub type FfiCallbackFn = unsafe extern "C" fn(*const u8, usize);
pub type FfiResult<T> = Result<T, FfiError>;
pub type FfiAsyncId = usize;
pub type FfiHandleId = usize;
pub type FfiHandle = Box<dyn Any + Send + Sync>;

pub const INVALID_HANDLE: FfiHandleId = 0;

#[no_mangle]
pub(crate) extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> FfiHandleId {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let res = match proto::FfiRequest::decode(data) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("failed to decode request: {}", err);
            return INVALID_HANDLE;
        }
    };

    let res = match server::FFI_SERVER.handle_request(res) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("failed to handle request: {}", err);
            return INVALID_HANDLE;
        }
    }
    .encode_to_vec();

    unsafe {
        *res_ptr = res.as_ptr();
        *res_len = res.len();
    }

    let handle_id = server::FFI_SERVER.next_id();
    server::FFI_SERVER
        .ffi_handles()
        .insert(handle_id, Box::new(res));

    handle_id
}

#[no_mangle]
pub(crate) extern "C" fn livekit_ffi_drop_handle(handle_id: FfiHandleId) -> bool {
    // Free the memory
    server::FFI_SERVER
        .ffi_handles()
        .remove(&handle_id)
        .is_some()
}
