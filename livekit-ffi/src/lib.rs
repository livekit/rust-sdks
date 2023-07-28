// Copyright 2023 LiveKit, Inc.
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

use livekit::prelude::*;
use prost::Message;
use std::any::Any;
use thiserror::Error;

mod conversion;
#[path = "livekit.proto.rs"]
mod proto;
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
pub extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> FfiHandleId {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let res = match proto::FfiRequest::decode(data) {
        Ok(res) => res,
        Err(err) => {
            log::error!("failed to decode request: {}", err);
            return INVALID_HANDLE;
        }
    };

    let res = match server::FFI_SERVER.handle_request(res) {
        Ok(res) => res,
        Err(err) => {
            log::error!("failed to handle request: {}", err);
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
        .ffi_handles
        .insert(handle_id, Box::new(res));

    handle_id
}

#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FfiHandleId) -> bool {
    // Free the memory
    server::FFI_SERVER.ffi_handles.remove(&handle_id).is_some()
}
