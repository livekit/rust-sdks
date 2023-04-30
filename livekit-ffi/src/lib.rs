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
    let res = proto::FfiRequest::decode(data);
    if let Err(ref err) = res {
        eprintln!("failed to decode request: {:?}", err);
        return 0;
    }

    if res.as_ref().unwrap().message.is_none() {
        eprintln!("request message is none");
        return 0;
    }

    let message = res.unwrap().message.unwrap();
    if let proto::ffi_request::Message::Initialize(ref init) = message {
        FFI_SERVER.initialize(init);
    }

    if let proto::ffi_request::Message::Dispose(_) = message {
        FFI_SERVER.dispose();
    }

    if !FFI_SERVER.initialized() {
        eprintln!("the server is not initialized");
        return 0;
    }

    let res = FFI_SERVER.handle_request(message).encode_to_vec();
    unsafe {
        *res_ptr = res.as_ptr();
        *res_len = res.len();
    }

    let handle_id = FFI_SERVER.next_handle_id();
    FFI_SERVER.insert_handle(handle_id, Box::new(res));
    handle_id
}

#[no_mangle]
extern "C" fn livekit_ffi_drop_handle(handle_id: server::FFIHandleId) -> bool {
    FFI_SERVER.release_handle(handle_id).is_some() // Free the memory
}
