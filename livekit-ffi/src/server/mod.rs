use crate::proto;
use lazy_static::lazy_static;
use livekit::prelude::*;
use livekit::webrtc::video_frame::BoxVideoFrameBuffer;
use livekit::webrtc::video_frame::{
    native::I420BufferExt, native::VideoFrameBufferExt, I420Buffer,
};
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
        for (_, (handle, shutdown_tx)) in self.rooms.write().drain() {
            let _ = shutdown_tx.send(());
            let _ = handle.await;
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

        let config = config.as_ref().unwrap();
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
                self.async_runtime
                    .spawn(room::create_room(&FFI_SERVER, async_id, connect));

                return proto::FfiResponse {
                    async_id: Some(async_id),
                    ..Default::default()
                };
            }
            proto::ffi_request::Message::ToI420(to_i420) => {
                let ffi_owned = self.ffi_owned.read();
                let buffer = ffi_owned
                    .get(&(to_i420.buffer.unwrap().id as FFIHandleId))
                    .unwrap();

                let buffer = buffer
                    .downcast_ref::<BoxVideoFrameBuffer>()
                    .unwrap()
                    .to_i420();

                let handle_id = self.next_handle_id();
                let buffer_info = Some(proto::VideoFrameBufferInfo::from(handle_id, &buffer));

                drop(ffi_owned);
                self.insert_handle(handle_id, Box::new(buffer));

                return proto::FfiResponse {
                    message: Some(proto::ffi_response::Message::ToI420(
                        proto::ToI420Response {
                            buffer: buffer_info,
                        },
                    )),
                    ..Default::default()
                };
            }
            proto::ffi_request::Message::ToArgb(to_argb) => {
                let ffi_owned = self.ffi_owned.read();
                let buffer = ffi_owned
                    .get(&(to_argb.buffer.unwrap().id as FFIHandleId))
                    .unwrap();

                let buffer = buffer.downcast_ref::<BoxVideoFrameBuffer>().unwrap();
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
            proto::ffi_request::Message::AllocBuffer(alloc_buffer) => {
                let frame_type =
                    proto::VideoFrameBufferType::from_i32(alloc_buffer.r#type).unwrap();

                let buffer: BoxVideoFrameBuffer = match frame_type {
                    proto::VideoFrameBufferType::I420 => Box::new(I420Buffer::new(
                        alloc_buffer.width as u32,
                        alloc_buffer.height as u32,
                    )),
                    _ => {
                        panic!("unsupported buffer type: {:?}", frame_type);
                    }
                };

                let handle_id = self.next_handle_id();
                let buffer_info = Some(proto::VideoFrameBufferInfo::from(handle_id, &buffer));
                self.insert_handle(handle_id, Box::new(buffer));

                return proto::FfiResponse {
                    message: Some(proto::ffi_response::Message::AllocBuffer(
                        proto::AllocBufferResponse {
                            buffer: buffer_info,
                        },
                    )),
                    ..Default::default()
                };
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

    if res.as_ref().unwrap().message.is_none() {
        eprintln!("request message is empty");
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

#[cfg(test)]
mod tests {
    use super::*;

    // Create two I420Buffer, and ensure the logic is correct ( ids, and responses )
    #[test]
    fn create_i420_buffer() {
        let res = FFI_SERVER.handle_request(proto::ffi_request::Message::AllocBuffer(
            proto::AllocBufferRequest {
                r#type: proto::VideoFrameBufferType::I420 as i32,
                width: 640,
                height: 480,
            },
        ));

        let proto::ffi_response::Message::AllocBuffer(alloc) = res.message.unwrap() else {
            panic!("unexpected response");
        };

        let i420_handle = alloc.buffer.unwrap().handle.unwrap().id as usize;
        assert_eq!(i420_handle, 1);

        let res =
            FFI_SERVER.handle_request(proto::ffi_request::Message::ToI420(proto::ToI420Request {
                buffer: Some(proto::FfiHandleId {
                    id: i420_handle as u64,
                }),
            }));

        FFI_SERVER.release_handle(i420_handle).unwrap();

        let proto::ffi_response::Message::ToI420(to_i420) = res.message.unwrap() else {
            panic!("unexpected response");
        };

        let new_handle = to_i420.buffer.unwrap().handle.unwrap().id as usize;
        assert_eq!(new_handle, 2);

        FFI_SERVER.release_handle(new_handle).unwrap();
    }
}
