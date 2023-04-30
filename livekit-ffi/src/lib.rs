use lazy_static::lazy_static;
use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

mod server;

lazy_static! {
    pub(crate) static ref FFI_SERVER: server::FFIServer = server::FFIServer::default();
}

#[no_mangle]
extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    res_ptr: *mut *const u8,
    res_len: *mut usize,
) -> server::FFIHandleId {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    let res = match proto::FfiRequest::decode(data) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("failed to decode request: {}", err);
            return 0;
        }
    };

    let res = match FFI_SERVER.handle_request(res) {
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

    let handle_id = FFI_SERVER.next_id();
    FFI_SERVER
        .ffi_handles()
        .write()
        .insert(handle_id, Box::new(res));

    handle_id
}

#[no_mangle]
extern "C" fn livekit_ffi_drop_handle(handle_id: server::FFIHandleId) -> bool {
    // Free the memory
    FFI_SERVER
        .ffi_handles()
        .write()
        .remove(&handle_id)
        .is_some()
}
