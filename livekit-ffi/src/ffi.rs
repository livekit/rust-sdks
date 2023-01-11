use crate::proto;
use prost::Message;
use std::slice;

// These "C" functions are threadsafe.
// When the FFI returns a response to the foreign language
// The current thread is undetermined

#[no_mangle]
pub extern "C" fn livekit_ffi_init() {

}

#[no_mangle]
pub extern "C" fn livekit_ffi_request(data: *const u8, len: usize) {
    let data = unsafe { slice::from_raw_parts(data, len) };
    let request = proto::FfiRequest::decode(data)
        .expect("Failed to decode the FFIRequest, does the protocol version mismatch?");

}
