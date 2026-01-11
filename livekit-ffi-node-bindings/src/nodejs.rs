// SPDX-FileCopyrightText: 2024 LiveKit, Inc.
//
// SPDX-License-Identifier: Apache-2.0

use livekit_ffi::{proto, server, FFI_SERVER};
use napi::{
    bindgen_prelude::*,
    threadsafe_function::{
        ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
    },
    JsFunction, Status,
};
use napi_derive::napi;
use prost::Message;
use std::sync::Arc;

#[napi(
    ts_args_type = "callback: (data: Uint8Array) => void, captureLogs: boolean, sdkVersion: string"
)]
fn livekit_initialize(cb: JsFunction, capture_logs: bool, sdk_version: String) {
    let tsfn: ThreadsafeFunction<proto::FfiEvent, ErrorStrategy::Fatal> = cb
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<proto::FfiEvent>| {
            let data = ctx.value.encode_to_vec();
            let buf = Uint8Array::new(data);
            Ok(vec![buf])
        })
        .unwrap();

    FFI_SERVER.setup(server::FfiConfig {
        callback_fn: Arc::new(move |event| {
            let status = tsfn.call(event, ThreadsafeFunctionCallMode::NonBlocking);
            if status != Status::Ok {
                eprintln!("error calling callback status: {}", status);
            }
        }),
        capture_logs,
        sdk: "node".to_string(),
        sdk_version,
    });
}

#[napi]
fn livekit_ffi_request(data: Uint8Array) -> Result<Uint8Array> {
    let data = data.to_vec();
    let res = match proto::FfiRequest::decode(data.as_slice()) {
        Ok(res) => res,
        Err(err) => {
            return Err(Error::from_reason(format!(
                "failed to decode request: {}",
                err.to_string()
            )));
        }
    };

    let res = match server::requests::handle_request(&FFI_SERVER, res.clone()) {
        Ok(res) => res,
        Err(err) => {
            return Err(Error::from_reason(format!(
                "failed to handle request: {} ({:?})",
                err.to_string(),
                res
            )));
        }
    }
    .encode_to_vec();
    Ok(Uint8Array::new(res))
}

// FfiHandle must be used instead
//#[napi]
//fn livekit_drop_handle(handle: BigInt) -> bool {
//    let (_, handle, _) = handle.get_u64();
//    FFI_SERVER.drop_handle(handle)
//}

#[napi]
fn livekit_retrieve_ptr(handle: Uint8Array) -> BigInt {
    BigInt::from(handle.as_ptr() as u64)
}

#[napi]
fn livekit_copy_buffer(ptr: BigInt, len: u32) -> Uint8Array {
    let (_, ptr, _) = ptr.get_u64();
    let data = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    Uint8Array::with_data_copied(data)
}

#[napi]
async fn livekit_dispose() {
    FFI_SERVER.dispose().await;
}

#[napi(custom_finalize)]
pub struct FfiHandle {
    handle: BigInt,
    disposed: bool,
    // TODO(theomonnom): add gc pressure memory
}

#[napi]
impl FfiHandle {
    #[napi(constructor)]
    pub fn new(handle: BigInt) -> Self {
        Self {
            handle,
            disposed: false,
        }
    }

    #[napi]
    pub fn dispose(&mut self) -> Result<()> {
        if self.disposed {
            return Ok(());
        }
        self.disposed = true;
        let (_, handle, _) = self.handle.get_u64();
        if !FFI_SERVER.drop_handle(handle) {
            return Err(Error::from_reason("trying to drop an invalid handle"));
        }

        Ok(())
    }

    #[napi(getter)]
    pub fn handle(&self) -> BigInt {
        self.handle.clone()
    }
}

impl ObjectFinalize for FfiHandle {
    fn finalize(mut self, env: Env) -> Result<()> {
        self.dispose()
    }
}
