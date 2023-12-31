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

use std::borrow::Cow;

use lazy_static::lazy_static;
use livekit::prelude::*;
use thiserror::Error;

mod conversion;

pub mod cabi;
pub mod proto;
pub mod server;

#[derive(Error, Debug)]
pub enum FfiError {
    #[error("the server is not configured")]
    NotConfigured,
    #[error("the server is already initialized")]
    AlreadyInitialized,
    #[error("room error {0}")]
    Room(#[from] RoomError),
    #[error("invalid request: {0}")]
    InvalidRequest(Cow<'static, str>),
}

/// # SAFTEY: The "C" callback must be threadsafe and not block
pub type FfiCallbackFn = unsafe extern "C" fn(*const u8, usize);
pub type FfiResult<T> = Result<T, FfiError>;
pub type FfiHandleId = u64;

pub const INVALID_HANDLE: FfiHandleId = 0;

lazy_static! {
    pub static ref FFI_SERVER: server::FfiServer = server::FfiServer::default();
}
