use prost::Message;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/livekit.rs"));
}

mod server;

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

    let res = match server::FFI_SRV_GLOBAL.handle_request(res) {
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

    let handle_id = server::FFI_SRV_GLOBAL.next_id();
    server::FFI_SRV_GLOBAL
        .ffi_handles()
        .write()
        .insert(handle_id, Box::new(res));

    handle_id
}

#[no_mangle]
extern "C" fn livekit_ffi_drop_handle(handle_id: server::FFIHandleId) -> bool {
    // Free the memory
    server::FFI_SRV_GLOBAL
        .ffi_handles()
        .write()
        .remove(&handle_id)
        .is_some()
}
