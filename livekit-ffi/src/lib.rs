use lazy_static::lazy_static;
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
pub enum FFIError {
    #[error("the server is not configured")]
    NotConfigured,
    #[error("the server is already initialized")]
    AlreadyInitialized,
    #[error("room error {0}")]
    Room(#[from] RoomError),
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
}

pub type FFIResult<T> = Result<T, FFIError>;
pub type FFIAsyncId = usize;
pub type FFIHandleId = usize;
pub type FFIHandle = Box<dyn Any + Send + Sync>;

pub const INVALID_HANDLE: FFIHandleId = 0;

lazy_static! {
    pub static ref FFI_SRV_GLOBAL: server::FFIServer = server::FFIServer::default();
}

#[no_mangle]
extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> FFIHandleId {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let res = match proto::FfiRequest::decode(data) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("failed to decode request: {}", err);
            return 0;
        }
    };

    let res = match FFI_SRV_GLOBAL.handle_request(res) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("failed to handle request: {}", err);
            return 0;
        }
    }
    .encode_to_vec();

    unsafe {
        *res_ptr = res.as_ptr();
        *res_len = res.len();
    }

    let handle_id = FFI_SRV_GLOBAL.next_id();
    FFI_SRV_GLOBAL
        .ffi_handles()
        .write()
        .insert(handle_id, Box::new(res));

    handle_id
}

#[no_mangle]
extern "C" fn livekit_ffi_drop_handle(handle_id: FFIHandleId) -> bool {
    // Free the memory
    FFI_SRV_GLOBAL
        .ffi_handles()
        .write()
        .remove(&handle_id)
        .is_some()
}
