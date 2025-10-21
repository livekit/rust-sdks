#[allow(unused_imports)]
use uniffi_runtime_javascript::{self as js, uniffi as u, IntoJs, IntoRust};
use wasm_bindgen::prelude::wasm_bindgen;
extern "C" {
    fn uniffi_livekit_uniffi_fn_func_build_version(
        status_: &mut u::RustCallStatus,
    ) -> u::RustBuffer;
    fn uniffi_livekit_uniffi_fn_func_generate_token(
        options: u::RustBuffer,
        credentials: u::RustBuffer,
        status_: &mut u::RustCallStatus,
    ) -> u::RustBuffer;
    fn uniffi_livekit_uniffi_fn_func_log_forward_bootstrap(
        level: u::RustBuffer,
        status_: &mut u::RustCallStatus,
    );
    fn uniffi_livekit_uniffi_fn_func_log_forward_receive() -> u64;
    fn uniffi_livekit_uniffi_fn_func_verify_token(
        token: u::RustBuffer,
        credentials: u::RustBuffer,
        status_: &mut u::RustCallStatus,
    ) -> u::RustBuffer;
    fn ffi_livekit_uniffi_rust_future_poll_u8(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_u8(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_u8(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_u8(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u8;
    fn ffi_livekit_uniffi_rust_future_poll_i8(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_i8(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_i8(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_i8(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> i8;
    fn ffi_livekit_uniffi_rust_future_poll_u16(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_u16(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_u16(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_u16(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u16;
    fn ffi_livekit_uniffi_rust_future_poll_i16(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_i16(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_i16(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_i16(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> i16;
    fn ffi_livekit_uniffi_rust_future_poll_u32(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_u32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_u32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_u32(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u32;
    fn ffi_livekit_uniffi_rust_future_poll_i32(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_i32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_i32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_i32(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> i32;
    fn ffi_livekit_uniffi_rust_future_poll_u64(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_u64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_u64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_u64(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u64;
    fn ffi_livekit_uniffi_rust_future_poll_i64(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_i64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_i64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_i64(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> i64;
    fn ffi_livekit_uniffi_rust_future_poll_f32(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_f32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_f32(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_f32(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> f32;
    fn ffi_livekit_uniffi_rust_future_poll_f64(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_f64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_f64(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_f64(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> f64;
    fn ffi_livekit_uniffi_rust_future_poll_pointer(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_pointer(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_pointer(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_pointer(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u::VoidPointer;
    fn ffi_livekit_uniffi_rust_future_poll_rust_buffer(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_rust_buffer(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_rust_buffer(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_rust_buffer(
        handle: u64,
        status_: &mut u::RustCallStatus,
    ) -> u::RustBuffer;
    fn ffi_livekit_uniffi_rust_future_poll_void(
        handle: u64,
        callback: rust_future_continuation_callback::FnSig,
        callback_data: u64,
    );
    fn ffi_livekit_uniffi_rust_future_cancel_void(handle: u64);
    fn ffi_livekit_uniffi_rust_future_free_void(handle: u64);
    fn ffi_livekit_uniffi_rust_future_complete_void(
        handle: u64,
        status_: &mut u::RustCallStatus,
    );
    fn uniffi_livekit_uniffi_checksum_func_build_version() -> u16;
    fn uniffi_livekit_uniffi_checksum_func_generate_token() -> u16;
    fn uniffi_livekit_uniffi_checksum_func_log_forward_bootstrap() -> u16;
    fn uniffi_livekit_uniffi_checksum_func_log_forward_receive() -> u16;
    fn uniffi_livekit_uniffi_checksum_func_verify_token() -> u16;
    fn ffi_livekit_uniffi_uniffi_contract_version() -> u32;
}
#[wasm_bindgen]
pub fn ubrn_uniffi_livekit_uniffi_fn_func_build_version(
    f_status_: &mut js::RustCallStatus,
) -> js::ForeignBytes {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe { uniffi_livekit_uniffi_fn_func_build_version(&mut u_status_) };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub fn ubrn_uniffi_livekit_uniffi_fn_func_generate_token(
    options: js::ForeignBytes,
    credentials: js::ForeignBytes,
    f_status_: &mut js::RustCallStatus,
) -> js::ForeignBytes {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        uniffi_livekit_uniffi_fn_func_generate_token(
            u::RustBuffer::into_rust(options),
            u::RustBuffer::into_rust(credentials),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub fn ubrn_uniffi_livekit_uniffi_fn_func_log_forward_bootstrap(
    level: js::ForeignBytes,
    f_status_: &mut js::RustCallStatus,
) {
    let mut u_status_ = u::RustCallStatus::default();
    unsafe {
        uniffi_livekit_uniffi_fn_func_log_forward_bootstrap(
            u::RustBuffer::into_rust(level),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_fn_func_log_forward_receive() -> js::Handle {
    uniffi_livekit_uniffi_fn_func_log_forward_receive().into_js()
}
#[wasm_bindgen]
pub fn ubrn_uniffi_livekit_uniffi_fn_func_verify_token(
    token: js::ForeignBytes,
    credentials: js::ForeignBytes,
    f_status_: &mut js::RustCallStatus,
) -> js::ForeignBytes {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        uniffi_livekit_uniffi_fn_func_verify_token(
            u::RustBuffer::into_rust(token),
            u::RustBuffer::into_rust(credentials),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_u8(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_u8(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_u8(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_u8(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_u8(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_u8(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_u8(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::UInt8 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_u8(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_i8(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_i8(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_i8(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_i8(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_i8(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_i8(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_i8(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Int8 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_i8(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_u16(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_u16(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_u16(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_u16(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_u16(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_u16(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_u16(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::UInt16 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_u16(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_i16(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_i16(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_i16(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_i16(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_i16(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_i16(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_i16(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Int16 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_i16(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_u32(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_u32(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_u32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_u32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_u32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_u32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_u32(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::UInt32 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_u32(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_i32(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_i32(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_i32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_i32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_i32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_i32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_i32(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Int32 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_i32(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_u64(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_u64(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_u64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_u64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_u64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_u64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_u64(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::UInt64 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_u64(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_i64(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_i64(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_i64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_i64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_i64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_i64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_i64(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Int64 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_i64(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_f32(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_f32(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_f32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_f32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_f32(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_f32(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_f32(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Float32 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_f32(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_f64(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_f64(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_f64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_f64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_f64(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_f64(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_f64(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::Float64 {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_f64(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_pointer(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_pointer(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_pointer(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_pointer(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_pointer(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_pointer(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_pointer(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::VoidPointer {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_pointer(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_rust_buffer(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_rust_buffer(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_rust_buffer(
    handle: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_cancel_rust_buffer(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_rust_buffer(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_rust_buffer(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_rust_buffer(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) -> js::ForeignBytes {
    let mut u_status_ = u::RustCallStatus::default();
    let value_ = unsafe {
        ffi_livekit_uniffi_rust_future_complete_rust_buffer(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
    value_.into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_poll_void(
    handle: js::Handle,
    callback: rust_future_continuation_callback::JsCallbackFn,
    callback_data: js::Handle,
) {
    ffi_livekit_uniffi_rust_future_poll_void(
        u64::into_rust(handle),
        rust_future_continuation_callback::FnSig::into_rust(callback),
        u64::into_rust(callback_data),
    );
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_cancel_void(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_cancel_void(u64::into_rust(handle));
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_rust_future_free_void(handle: js::Handle) {
    ffi_livekit_uniffi_rust_future_free_void(u64::into_rust(handle));
}
#[wasm_bindgen]
pub fn ubrn_ffi_livekit_uniffi_rust_future_complete_void(
    handle: js::Handle,
    f_status_: &mut js::RustCallStatus,
) {
    let mut u_status_ = u::RustCallStatus::default();
    unsafe {
        ffi_livekit_uniffi_rust_future_complete_void(
            u64::into_rust(handle),
            &mut u_status_,
        )
    };
    f_status_.copy_from(u_status_);
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_checksum_func_build_version() -> js::UInt16 {
    uniffi_livekit_uniffi_checksum_func_build_version().into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_checksum_func_generate_token() -> js::UInt16 {
    uniffi_livekit_uniffi_checksum_func_generate_token().into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_checksum_func_log_forward_bootstrap() -> js::UInt16 {
    uniffi_livekit_uniffi_checksum_func_log_forward_bootstrap().into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_checksum_func_log_forward_receive() -> js::UInt16 {
    uniffi_livekit_uniffi_checksum_func_log_forward_receive().into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_uniffi_livekit_uniffi_checksum_func_verify_token() -> js::UInt16 {
    uniffi_livekit_uniffi_checksum_func_verify_token().into_js()
}
#[wasm_bindgen]
pub unsafe fn ubrn_ffi_livekit_uniffi_uniffi_contract_version() -> js::UInt32 {
    ffi_livekit_uniffi_uniffi_contract_version().into_js()
}
mod rust_future_continuation_callback {
    use super::*;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen]
        pub type JsCallbackFn;
        #[wasm_bindgen(method)]
        pub fn call(
            this_: &JsCallbackFn,
            ctx_: &JsCallbackFn,
            data: js::UInt64,
            poll_result: js::Int8,
        );
    }
    thread_local! {
        static CALLBACK : js::ForeignCell < JsCallbackFn > = js::ForeignCell::new();
    }
    impl IntoRust<JsCallbackFn> for FnSig {
        fn into_rust(callback: JsCallbackFn) -> Self {
            CALLBACK.with(|cell| cell.set(callback));
            implementation
        }
    }
    pub(super) type FnSig = extern "C" fn(data: u64, poll_result: i8);
    extern "C" fn implementation(data: u64, poll_result: i8) {
        CALLBACK
            .with(|cell_| {
                cell_
                    .with_value(|callback_| {
                        callback_.call(callback_, data.into_js(), poll_result.into_js())
                    })
            });
    }
}
