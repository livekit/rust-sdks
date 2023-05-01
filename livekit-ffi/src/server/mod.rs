use crate::proto;
use crate::{FFIError, FFIHandle, FFIHandleId, FFIResult};
use futures_util::stream::StreamExt;
use livekit::prelude::*;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{native::I420BufferExt, BoxVideoFrameBuffer, I420Buffer};
use parking_lot::{Mutex, RwLock};
use prost::Message;
use std::collections::HashMap;
use std::slice;
use std::sync::atomic::{AtomicUsize, Ordering};

mod conversion;
mod room;
mod video_stream;

type CallbackFn = unsafe extern "C" fn(*const u8, usize); // The "C" callback must be threadsafe

pub struct FFIConfig {
    callback_fn: CallbackFn,
}

pub struct FFIServer {
    rooms: RwLock<HashMap<RoomSid, room::FFIRoom>>,
    ffi_handles: RwLock<HashMap<FFIHandleId, FFIHandle>>,
    next_id: AtomicUsize,
    async_runtime: tokio::runtime::Runtime,
    config: Mutex<Option<FFIConfig>>,
}

impl Default for FFIServer {
    fn default() -> Self {
        Self {
            rooms: Default::default(),
            ffi_handles: Default::default(),
            next_id: AtomicUsize::new(1), // 0 is invalid
            async_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            config: Default::default(),
        }
    }
}

// Using &'static self inside the implementation, not sure if this is really idiomatic
// It simplifies the code a lot tho. In most cases the server is used until the end of the process
impl FFIServer {
    pub async fn close(&'static self) {
        // Close all rooms
        for (_, ffi_room) in self.rooms.write().drain() {
            ffi_room.close().await;
        }
    }

    pub fn next_id(&'static self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn ffi_handles(&'static self) -> &RwLock<HashMap<FFIHandleId, FFIHandle>> {
        &self.ffi_handles
    }

    pub fn send_event(&'static self, message: proto::ffi_event::Message) -> FFIResult<()> {
        let callback_fn = self
            .config
            .lock()
            .as_ref()
            .map_or_else(|| Err(FFIError::NotConfigured), |c| Ok(c.callback_fn))?;

        let message = proto::FfiEvent {
            message: Some(message),
        }
        .encode_to_vec();

        unsafe {
            callback_fn(message.as_ptr(), message.len());
        }
        Ok(())
    }
}

impl FFIServer {
    fn on_initialize(
        &'static self,
        init: proto::InitializeRequest,
    ) -> FFIResult<proto::InitializeResponse> {
        if self.config.lock().is_some() {
            return Err(FFIError::AlreadyInitialized);
        }

        // # SAFETY: The foreign language is responsible for ensuring that the callback function is valid
        unsafe {
            *self.config.lock() = Some(FFIConfig {
                callback_fn: std::mem::transmute(init.event_callback_ptr),
            });
        }

        Ok(proto::InitializeResponse::default())
    }

    fn on_dispose(
        &'static self,
        dispose: proto::DisposeRequest,
    ) -> FFIResult<proto::DisposeResponse> {
        *self.config.lock() = None;

        let close = self.close();
        if !dispose.r#async {
            self.async_runtime.block_on(close);
            Ok(proto::DisposeResponse::default())
        } else {
            let async_id = self.next_id();
            self.async_runtime.spawn(async move {
                close.await;
            });
            Ok(proto::DisposeResponse {
                async_id: Some(proto::FfiAsyncId {
                    id: async_id as u64,
                }),
            })
        }
    }

    // Room

    fn on_connect(
        &'static self,
        connect: proto::ConnectRequest,
    ) -> FFIResult<proto::ConnectResponse> {
        let async_id = self.next_id();
        self.async_runtime.spawn(async move {
            // Try to connect to the Room
            let res = room::FFIRoom::connect(&self, connect).await;

            let async_id = proto::FfiAsyncId {
                id: async_id as u64,
            };

            let mut success = false;
            let mut room_info = None;

            if let Ok(ffi_room) = res {
                let session = ffi_room.session();
                room_info = Some(proto::RoomInfo::from(&session));
                success = true;

                self.rooms.write().insert(session.sid(), ffi_room);
            }

            // Send the async response
            let _ = self.send_event(proto::ffi_event::Message::Connect(proto::ConnectCallback {
                async_id: Some(async_id),
                success,
                room: room_info,
            }));
        });

        Ok(proto::ConnectResponse {
            async_id: Some(proto::FfiAsyncId {
                id: async_id as u64,
            }),
        })
    }

    fn on_disconnect(
        &'static self,
        disconnect: proto::DisconnectRequest,
    ) -> FFIResult<proto::DisconnectResponse> {
        Ok(proto::DisconnectResponse::default())
    }

    fn on_publish_track(
        &'static self,
        publish: proto::PublishTrackRequest,
    ) -> FFIResult<proto::PublishTrackResponse> {
        Ok(proto::PublishTrackResponse::default())
    }

    fn on_unpublish_track(
        &'static self,
        unpublish: proto::UnpublishTrackRequest,
    ) -> FFIResult<proto::UnpublishTrackResponse> {
        Ok(proto::UnpublishTrackResponse::default())
    }

    // Track
    fn on_create_video_track(
        &'static self,
        create: proto::CreateVideoTrackRequest,
    ) -> FFIResult<proto::CreateVideoTrackResponse> {
        Ok(proto::CreateVideoTrackResponse::default())
    }

    fn on_create_audio_track(
        &'static self,
        create: proto::CreateAudioTrackRequest,
    ) -> FFIResult<proto::CreateAudioTrackResponse> {
        Ok(proto::CreateAudioTrackResponse::default())
    }

    // Video

    fn on_alloc_video_buffer(
        &'static self,
        alloc: proto::AllocVideoBufferRequest,
    ) -> FFIResult<proto::AllocVideoBufferResponse> {
        let frame_type = proto::VideoFrameBufferType::from_i32(alloc.r#type).unwrap();
        let buffer: BoxVideoFrameBuffer = match frame_type {
            proto::VideoFrameBufferType::I420 => {
                Box::new(I420Buffer::new(alloc.width, alloc.height))
            }
            _ => {
                return Err(FFIError::InvalidRequest(
                    "frame type is not supported".to_owned(),
                ))
            }
        };

        let handle_id = self.next_id();
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &buffer);
        self.ffi_handles()
            .write()
            .insert(handle_id, Box::new(buffer));

        Ok(proto::AllocVideoBufferResponse {
            buffer: Some(buffer_info),
        })
    }

    fn on_new_video_stream(
        &'static self,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FFIResult<proto::NewVideoStreamResponse> {
        let stream = video_stream::FFIVideoStream::from_request(&self, new_stream)?;

        let handle_id = self.next_id();
        let stream_info = proto::VideoStreamInfo::from(handle_id, &stream);

        self.ffi_handles()
            .write()
            .insert(handle_id, Box::new(stream));

        Ok(proto::NewVideoStreamResponse {
            stream: Some(stream_info),
        })
    }

    fn on_new_video_source(
        &'static self,
        new_source: proto::NewVideoSourceRequest,
    ) -> FFIResult<proto::NewVideoSourceResponse> {
        Ok(proto::NewVideoSourceResponse::default())
    }

    fn on_push_video_frame(
        &'static self,
        push: proto::PushVideoFrameRequest,
    ) -> FFIResult<proto::PushVideoFrameResponse> {
        Ok(proto::PushVideoFrameResponse::default())
    }

    fn on_to_i420(
        &'static self,
        to_i420: proto::ToI420Request,
    ) -> FFIResult<proto::ToI420Response> {
        let from = to_i420
            .from
            .ok_or(FFIError::InvalidRequest("from is empty".to_string()))?;
        let flip_y = to_i420.flip_y;

        let i420 = match from {
            proto::to_i420_request::From::Argb(argb_info) => {
                let mut i420 = I420Buffer::new(argb_info.width, argb_info.height);

                let format = proto::VideoFormatType::from_i32(argb_info.format).unwrap();
                let argb_ptr = argb_info.ptr as *const u8;
                let argb_len = (argb_info.stride * argb_info.height) as usize;
                let argb = unsafe { slice::from_raw_parts(argb_ptr, argb_len) };
                let stride = argb_info.stride;
                let (stride_y, stride_u, stride_v) = i420.strides();
                let (data_y, data_u, data_v) = i420.data_mut();
                let width = argb_info.width as i32;
                let mut height = argb_info.height as i32;
                if flip_y {
                    height = -height;
                }

                match format {
                    proto::VideoFormatType::FormatArgb => {
                        yuv_helper::argb_to_i420(
                            argb, stride, data_y, stride_y, data_u, stride_u, data_v, stride_v,
                            width, height,
                        )
                        .unwrap();
                    }
                    proto::VideoFormatType::FormatAbgr => {
                        yuv_helper::abgr_to_i420(
                            argb, stride, data_y, stride_y, data_u, stride_u, data_v, stride_v,
                            width, height,
                        )
                        .unwrap();
                    }
                    _ => {
                        return Err(FFIError::InvalidRequest(
                            "the format is not supported".to_string(),
                        ))
                    }
                }

                i420
            }
            proto::to_i420_request::From::Buffer(handle) => {
                let ffi_handles = self.ffi_handles().read();
                let handle_id = handle.id as FFIHandleId;
                let buffer = ffi_handles
                    .get(&handle_id)
                    .ok_or(FFIError::HandleNotFound)?;
                let i420 = buffer
                    .downcast_ref::<BoxVideoFrameBuffer>()
                    .ok_or(FFIError::InvalidHandle)?
                    .to_i420();

                i420
            }
        };

        let i420: BoxVideoFrameBuffer = Box::new(i420);
        let mut ffi_handles = self.ffi_handles().write();
        let handle_id = self.next_id() as FFIHandleId;
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &i420);
        ffi_handles.insert(handle_id, Box::new(i420));
        Ok(proto::ToI420Response {
            buffer: Some(buffer_info),
        })
    }

    fn on_to_argb(
        &'static self,
        to_argb: proto::ToArgbRequest,
    ) -> FFIResult<proto::ToArgbResponse> {
        let ffi_handles = self.ffi_handles.read();
        let handle_id = to_argb
            .buffer
            .ok_or(FFIError::InvalidRequest("buffer is empty".to_string()))?
            .id as FFIHandleId;
        let buffer = ffi_handles
            .get(&handle_id)
            .ok_or(FFIError::HandleNotFound)?;
        let buffer = buffer
            .downcast_ref::<BoxVideoFrameBuffer>()
            .ok_or(FFIError::InvalidHandle)?;
        let flip_y = to_argb.flip_y;
        let dst_format = proto::VideoFormatType::from_i32(to_argb.dst_format).unwrap();
        let dst_buf = unsafe {
            slice::from_raw_parts_mut(
                to_argb.dst_ptr as *mut u8,
                (to_argb.dst_stride * to_argb.dst_height) as usize,
            )
        };
        let dst_stride = to_argb.dst_stride;
        let dst_width = to_argb.dst_width as i32;
        let mut dst_height = to_argb.dst_height as i32;
        if flip_y {
            dst_height = -dst_height;
        }

        buffer
            .to_argb(
                dst_format.into(),
                dst_buf,
                dst_stride,
                dst_width,
                dst_height,
            )
            .unwrap();

        Ok(proto::ToArgbResponse::default())
    }

    // Audio

    fn on_alloc_audio_buffer(
        &'static self,
        alloc: proto::AllocAudioBufferRequest,
    ) -> FFIResult<proto::AllocAudioBufferResponse> {
        Ok(proto::AllocAudioBufferResponse::default())
    }

    fn on_new_audio_stream(
        &'static self,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FFIResult<proto::NewAudioStreamResponse> {
        Ok(proto::NewAudioStreamResponse::default())
    }

    fn on_new_audio_source(
        &'static self,
        new_source: proto::NewAudioSourceRequest,
    ) -> FFIResult<proto::NewAudioSourceResponse> {
        Ok(proto::NewAudioSourceResponse::default())
    }

    fn on_push_audio_frame(
        &'static self,
        push: proto::PushAudioFrameRequest,
    ) -> FFIResult<proto::PushAudioFrameResponse> {
        Ok(proto::PushAudioFrameResponse::default())
    }

    pub fn handle_request(
        &'static self,
        request: proto::FfiRequest,
    ) -> FFIResult<proto::FfiResponse> {
        let request = request
            .message
            .ok_or(FFIError::InvalidRequest("message is empty".to_string()))?;

        let mut res = proto::FfiResponse::default();
        res.message = Some(match request {
            proto::ffi_request::Message::Initialize(init) => {
                proto::ffi_response::Message::Initialize(self.on_initialize(init)?)
            }
            proto::ffi_request::Message::Dispose(dispose) => {
                proto::ffi_response::Message::Dispose(self.on_dispose(dispose)?)
            }
            proto::ffi_request::Message::Connect(connect) => {
                proto::ffi_response::Message::Connect(self.on_connect(connect)?)
            }
            proto::ffi_request::Message::Disconnect(disconnect) => {
                proto::ffi_response::Message::Disconnect(self.on_disconnect(disconnect)?)
            }
            proto::ffi_request::Message::PublishTrack(publish) => {
                proto::ffi_response::Message::PublishTrack(self.on_publish_track(publish)?)
            }
            proto::ffi_request::Message::UnpublishTrack(unpublish) => {
                proto::ffi_response::Message::UnpublishTrack(self.on_unpublish_track(unpublish)?)
            }
            proto::ffi_request::Message::CreateVideoTrack(create) => {
                proto::ffi_response::Message::CreateVideoTrack(self.on_create_video_track(create)?)
            }
            proto::ffi_request::Message::CreateAudioTrack(create) => {
                proto::ffi_response::Message::CreateAudioTrack(self.on_create_audio_track(create)?)
            }
            proto::ffi_request::Message::AllocVideoBuffer(alloc) => {
                proto::ffi_response::Message::AllocVideoBuffer(self.on_alloc_video_buffer(alloc)?)
            }
            proto::ffi_request::Message::NewVideoStream(new_stream) => {
                proto::ffi_response::Message::NewVideoStream(self.on_new_video_stream(new_stream)?)
            }
            proto::ffi_request::Message::NewVideoSource(new_source) => {
                proto::ffi_response::Message::NewVideoSource(self.on_new_video_source(new_source)?)
            }
            proto::ffi_request::Message::PushVideoFrame(push) => {
                proto::ffi_response::Message::PushVideoFrame(self.on_push_video_frame(push)?)
            }
            proto::ffi_request::Message::ToI420(to_i420) => {
                proto::ffi_response::Message::ToI420(self.on_to_i420(to_i420)?)
            }
            proto::ffi_request::Message::ToArgb(to_argb) => {
                proto::ffi_response::Message::ToArgb(self.on_to_argb(to_argb)?)
            }
            proto::ffi_request::Message::AllocAudioBuffer(alloc) => {
                proto::ffi_response::Message::AllocAudioBuffer(self.on_alloc_audio_buffer(alloc)?)
            }
            proto::ffi_request::Message::NewAudioStream(new_stream) => {
                proto::ffi_response::Message::NewAudioStream(self.on_new_audio_stream(new_stream)?)
            }
            proto::ffi_request::Message::NewAudioSource(new_source) => {
                proto::ffi_response::Message::NewAudioSource(self.on_new_audio_source(new_source)?)
            }
            proto::ffi_request::Message::PushAudioFrame(push) => {
                proto::ffi_response::Message::PushAudioFrame(self.on_push_audio_frame(push)?)
            }
        });

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FFI_SRV_GLOBAL;

    // Create two I420Buffer, and ensure the logic is correct ( ids, and responses )
    #[test]
    fn create_i420_buffer() {
        let res = FFI_SRV_GLOBAL
            .handle_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::AllocVideoBuffer(
                    proto::AllocVideoBufferRequest {
                        r#type: proto::VideoFrameBufferType::I420 as i32,
                        width: 640,
                        height: 480,
                    },
                )),
            })
            .unwrap();

        let proto::ffi_response::Message::AllocVideoBuffer(alloc) = res.message.unwrap() else {
            panic!("unexpected response");
        };

        let i420_handle = alloc.buffer.unwrap().handle.unwrap().id as usize;
        assert_eq!(i420_handle, 1);

        let res = FFI_SRV_GLOBAL
            .handle_request(proto::FfiRequest {
                message: Some(proto::ffi_request::Message::ToI420(proto::ToI420Request {
                    flip_y: false,
                    from: Some(proto::to_i420_request::From::Buffer(proto::FfiHandleId {
                        id: i420_handle as u64,
                    })),
                })),
            })
            .unwrap();

        FFI_SRV_GLOBAL
            .ffi_handles()
            .write()
            .remove(&i420_handle)
            .unwrap();

        let proto::ffi_response::Message::ToI420(to_i420) = res.message.unwrap() else {
            panic!("unexpected response");
        };

        let new_handle = to_i420.buffer.unwrap().handle.unwrap().id as usize;
        assert_eq!(new_handle, 2);

        FFI_SRV_GLOBAL
            .ffi_handles()
            .write()
            .remove(&new_handle)
            .unwrap();
    }
}
