use crate::proto;
use lazy_static::lazy_static;
use livekit::prelude::*;
use parking_lot::{Mutex, RwLock};
use prost::Message;
use std::any::Any;
use std::collections::HashMap;
use std::panic;
use std::slice;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

mod conversion;
mod room;

#[derive(Error, Debug)]
pub enum FFIError {
    #[error("the FFIServer isn't configured")]
    NotConfigured,
    #[error("failed to execute the FFICallback")]
    CallbackFailed,
}

pub type FFIHandleId = usize;
pub type FFIHandle = Box<dyn Any + Send + Sync>;

type CallbackFn = unsafe extern "C" fn(*const u8, usize); // This "C" callback must be threadsafe

lazy_static! {
    static ref FFI_SERVER: FFIServer = FFIServer::default();
}

pub struct FFIConfig {
    callback_fn: CallbackFn,
}

/// To use the FFI, the foreign language and the FFI server must share
/// the same memory space
pub struct FFIServer {
    // Object owned by the foreign language
    // The foreign language is responsible for freeing this memory
    //
    // NOTE: For VideoBuffers, we always store the enum VideoFrameBuffer
    ffi_owned: RwLock<HashMap<FFIHandleId, FFIHandle>>,
    next_handle_id: AtomicU64, // FFIHandleId
    next_async_id: AtomicU64,

    rooms: RwLock<HashMap<RoomSid, (JoinHandle<()>, oneshot::Sender<()>)>>,
    async_runtime: tokio::runtime::Runtime,
    initialized: AtomicBool,
    config: Mutex<Option<FFIConfig>>,
}

impl Default for FFIServer {
    fn default() -> Self {
        Self {
            ffi_owned: RwLock::new(HashMap::new()),
            next_handle_id: AtomicU64::new(1), // 0 is considered invalid
            next_async_id: AtomicU64::new(1),
            rooms: RwLock::new(HashMap::new()),
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            initialized: Default::default(),
            config: Default::default(),
        }
    }
}

impl FFIServer {
    pub fn initialize(&self, init: &proto::InitializeRequest) {
        if self.initialized() {
            self.dispose();
        }

        self.initialized.store(true, Ordering::SeqCst);
        *self.config.lock() = Some(FFIConfig {
            callback_fn: unsafe { std::mem::transmute(init.event_callback_ptr) },
        });
    }

    pub fn dispose(&self) {
        self.initialized.store(false, Ordering::SeqCst);
        *self.config.lock() = None;
        self.async_runtime.block_on(self.close());
    }

    pub async fn close(&self) {
        // Close all rooms
        for (k, (handle, shutdown_tx)) in self.rooms.write().drain() {
            shutdown_tx.send(());
            handle.await;
        }
    }

    pub fn add_room(&self, sid: RoomSid, handle: (JoinHandle<()>, oneshot::Sender<()>)) {
        self.rooms.write().insert(sid, handle);
    }

    pub fn initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    pub fn next_handle_id(&self) -> FFIHandleId {
        self.next_handle_id.fetch_add(1, Ordering::SeqCst) as FFIHandleId
    }

    pub fn next_async_id(&self) -> u64 {
        self.next_async_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert_handle(&self, handle_id: FFIHandleId, handle: FFIHandle) {
        self.ffi_owned.write().insert(handle_id, handle);
    }

    pub fn release_handle(&self, handle_id: FFIHandleId) -> Option<FFIHandle> {
        self.ffi_owned.write().remove(&handle_id)
    }

    pub fn send_event(
        &self,
        message: proto::ffi_event::Message,
        async_id: Option<u64>,
    ) -> Result<(), FFIError> {
        let config = self.config.lock();

        if !self.initialized() {
            Err(FFIError::NotConfigured)?
        }

        let message = proto::FfiEvent {
            async_id,
            message: Some(message),
        }
        .encode_to_vec();

        let config = config.unwrap();
        if let Err(err) = panic::catch_unwind(|| unsafe {
            (config.callback_fn)(message.as_ptr(), message.len());
        }) {
            eprintln!("panic when sending ffi event: {:?}", err);
            Err(FFIError::CallbackFailed)?
        }

        Ok(())
    }

    pub fn handle_request(&self, message: proto::ffi_request::Message) -> proto::FfiResponse {
        match message {
            proto::ffi_request::Message::AsyncConnect(connect) => {
                let async_id = self.next_async_id();
                let room_handle =
                    self.async_runtime
                        .spawn(room::create_room(&FFI_SERVER, async_id, connect));

                return proto::FfiResponse {
                    async_id: Some(async_id),
                    ..Default::default()
                };
            }
            proto::ffi_request::Message::ToI420(to_i420) => {
                let mut handle_id = 0; // Invalid handle
                let buffer = self.release_handle(to_i420.buffer.unwrap().id as FFIHandleId);
                if let Some(buffer) = buffer {
                    if let Ok(buffer) = buffer.downcast::<VideoFrameBuffer>() {
                        handle_id = self.next_handle_id();
                        self.insert_handle(handle_id, Box::new(buffer.to_i420()));
                    }
                }

                let res = proto::ToI420Response {
                    new_buffer: Some(proto::FfiHandleId {
                        id: handle_id as u64,
                    }),
                };

                return proto::FfiResponse {
                    message: Some(proto::ffi_response::Message::ToI420(res)),
                    ..Default::default()
                };
            }
            proto::ffi_request::Message::ToArgb(to_argb) => {
                let ffi_owned = self.ffi_owned.read();
                let buffer = ffi_owned.get(&(to_argb.buffer.unwrap().id as FFIHandleId));

                if let Some(buffer) = buffer {
                    if let Some(buffer) = buffer.downcast_ref::<VideoFrameBuffer>() {
                        let dst_buf = unsafe {
                            slice::from_raw_parts_mut(
                                to_argb.dst_ptr as *mut u8,
                                (to_argb.dst_stride * to_argb.dst_height) as usize,
                            )
                        };

                        if let Err(err) = buffer.to_argb(
                            proto::VideoFormatType::from_i32(to_argb.dst_format)
                                .unwrap()
                                .into(),
                            dst_buf,
                            to_argb.dst_stride,
                            to_argb.dst_width,
                            to_argb.dst_height,
                        ) {
                            eprintln!("failed to convert videoframe to argb: {:?}", err);
                        }
                    }
                }
            }
            _ => {}
        }

        proto::FfiResponse::default()
    }
}

/// This function is threadsafe, this is useful to run synchronous requests in another thread (e.g
/// color conversion)
#[no_mangle]
pub extern "C" fn livekit_ffi_request(
    data: *const u8,
    len: usize,
    data_ptr: *mut *const u8,
    data_len: *mut usize,
) -> FFIHandleId {
    let data = unsafe { slice::from_raw_parts(data, len) };
    let res = proto::FfiRequest::decode(data);
    if let Err(ref err) = res {
        eprintln!("failed to decode FfiRequest: {:?}", err);
        return 0;
    }

    if res.unwrap().message.is_none() {
        eprintln!("request message is empty");
        return 0;
    }

    let message = res.unwrap().message.unwrap();
    if let proto::ffi_request::Message::Initialize(ref init) = message {
        FFI_SERVER.initialize(init);
    }

    if let proto::ffi_request::Message::Dispose(ref dispose) = message {
        FFI_SERVER.dispose();
    }

    if !FFI_SERVER.initialized() {
        eprintln!("the FFIServer isn't initialized");
        return 0;
    }

    let res = FFI_SERVER.handle_request(message);
    let buf = res.encode_to_vec();

    unsafe {
        *data_ptr = buf.as_ptr();
        *data_len = buf.len();
    }

    let handle_id = FFI_SERVER.next_handle_id();
    FFI_SERVER.insert_handle(handle_id, Box::new(buf));
    handle_id
}

#[no_mangle]
pub extern "C" fn livekit_ffi_drop_handle(handle_id: FFIHandleId) -> bool {
    FFI_SERVER.release_handle(handle_id).is_some() // Free the memory
}
