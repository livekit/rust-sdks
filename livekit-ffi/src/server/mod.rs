use crate::{proto, FfiCallbackFn};
use crate::{FfiAsyncId, FfiError, FfiHandle, FfiHandleId, FfiResult};
use dashmap::DashMap;
use lazy_static::lazy_static;
use livekit::prelude::*;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{native::I420BufferExt, BoxVideoFrameBuffer, I420Buffer};
use parking_lot::Mutex;
use prost::Message;
use std::collections::HashMap;
use std::slice;
use std::sync::atomic::{AtomicUsize, Ordering};

pub mod audio_frame;
pub mod room;
pub mod utils;
pub mod video_frame;

#[cfg(test)]
mod tests;

lazy_static! {
    pub static ref FFI_SERVER: FfiServer = FfiServer::default();
}

pub struct FfiConfig {
    callback_fn: FfiCallbackFn,
}

pub struct FfiServer {
    rooms: Mutex<HashMap<RoomSid, FfiHandleId>>,
    /// Store all FFI handles inside an HashMap, if this isn't efficient enough
    /// We can still use Box::into_raw & Box::from_raw in the future (but keep it safe for now)
    ffi_handles: DashMap<FfiHandleId, FfiHandle>,
    next_id: AtomicUsize,
    async_runtime: tokio::runtime::Runtime,
    config: Mutex<Option<FfiConfig>>,
}

impl Default for FfiServer {
    fn default() -> Self {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

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
impl FfiServer {
    pub async fn dispose(&'static self) {
        // Close all rooms
        for (_, room_handle) in self.rooms.lock().drain() {
            let room = self.ffi_handles.remove(&room_handle);
            if let Some(room) = room {
                let ffi_room = room.1.downcast::<room::FfiRoom>().unwrap();
                ffi_room.close().await;
            }
        }
        // Drop all handles
        self.ffi_handles.clear();

        // Invalidate the config
        *self.config.lock() = None;
    }

    pub fn next_id(&'static self) -> usize {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    pub fn ffi_handles(&'static self) -> &DashMap<FfiHandleId, FfiHandle> {
        &self.ffi_handles
    }

    pub fn rooms(&'static self) -> &Mutex<HashMap<RoomSid, FfiHandleId>> {
        &self.rooms
    }

    pub fn send_event(&'static self, message: proto::ffi_event::Message) -> FfiResult<()> {
        let callback_fn = self
            .config
            .lock()
            .as_ref()
            .map_or_else(|| Err(FfiError::NotConfigured), |c| Ok(c.callback_fn))?;

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

impl FfiServer {
    fn on_initialize(
        &'static self,
        init: proto::InitializeRequest,
    ) -> FfiResult<proto::InitializeResponse> {
        if self.config.lock().is_some() {
            return Err(FfiError::AlreadyInitialized);
        }

        // # SAFETY: The foreign language is responsible for ensuring that the callback function is valid
        unsafe {
            *self.config.lock() = Some(FfiConfig {
                callback_fn: std::mem::transmute(init.event_callback_ptr),
            });
        }

        Ok(proto::InitializeResponse::default())
    }

    fn on_dispose(
        &'static self,
        dispose: proto::DisposeRequest,
    ) -> FfiResult<proto::DisposeResponse> {
        *self.config.lock() = None;

        let close = self.dispose();
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
    ) -> FfiResult<proto::ConnectResponse> {
        let async_id = self.next_id();
        self.async_runtime.spawn(async move {
            // Try to connect to the Room
            let res = room::FfiRoom::connect(&self, connect).await;

            // match res
            match res {
                Ok(room_info) => {
                    let _ = self.send_event(proto::ffi_event::Message::Connect(
                        proto::ConnectCallback {
                            async_id: Some(async_id.into()),
                            error: None,
                            room: Some(room_info),
                        },
                    ));
                }
                Err(err) => {
                    let _ = self.send_event(proto::ffi_event::Message::Connect(
                        proto::ConnectCallback {
                            async_id: Some(async_id.into()),
                            error: Some(err.to_string()),
                            room: None,
                        },
                    ));
                }
            }
        });

        Ok(proto::ConnectResponse {
            async_id: Some(proto::FfiAsyncId {
                id: async_id as u64,
            }),
        })
    }

    fn on_disconnect(
        &'static self,
        _disconnect: proto::DisconnectRequest,
    ) -> FfiResult<proto::DisconnectResponse> {
        Ok(proto::DisconnectResponse::default())
    }

    fn on_publish_track(
        &'static self,
        publish: proto::PublishTrackRequest,
    ) -> FfiResult<proto::PublishTrackResponse> {
        let async_id = self.next_id() as FfiAsyncId;
        tokio::spawn(async move {
            let res = async {
                let room_handle = publish
                    .room_handle
                    .as_ref()
                    .ok_or(FfiError::InvalidRequest("room_handle is empty"))?
                    .id as FfiHandleId;

                let room = self
                    .ffi_handles
                    .get(&room_handle)
                    .ok_or(FfiError::InvalidRequest("room not found"))?;

                let room = room
                    .downcast_ref::<room::FfiRoom>()
                    .ok_or(FfiError::InvalidRequest("room is not a FfiRoom"))?;

                let track_handle = publish
                    .track_handle
                    .as_ref()
                    .ok_or(FfiError::InvalidRequest("track_handle is empty"))?
                    .id as FfiHandleId;

                let track = self
                    .ffi_handles
                    .get(&track_handle)
                    .ok_or(FfiError::InvalidRequest("track not found"))?;

                let track = track
                    .downcast_ref::<LocalTrack>()
                    .ok_or(FfiError::InvalidRequest("track is not a LocalTrack"))?;

                let publication = room
                    .session()
                    .local_participant()
                    .publish_track(
                        track.clone(),
                        publish.options.map(Into::into).unwrap_or_default(),
                    )
                    .await?;

                Ok::<LocalTrackPublication, FfiError>(publication)
            }
            .await;

            if let Err(err) = self.send_event(proto::ffi_event::Message::PublishTrack(
                proto::PublishTrackCallback {
                    async_id: Some(async_id.into()),
                    error: res.as_ref().err().map(|e| e.to_string()),
                    publication: res.as_ref().ok().map(Into::into),
                },
            )) {
                log::warn!("error sending PublishTrack callback: {}", err);
            }
        });

        Ok(proto::PublishTrackResponse {
            async_id: Some(async_id.into()),
        })
    }

    fn on_unpublish_track(
        &'static self,
        _unpublish: proto::UnpublishTrackRequest,
    ) -> FfiResult<proto::UnpublishTrackResponse> {
        Ok(proto::UnpublishTrackResponse::default())
    }

    // Track
    fn on_create_video_track(
        &'static self,
        create: proto::CreateVideoTrackRequest,
    ) -> FfiResult<proto::CreateVideoTrackResponse> {
        let handle_id = create
            .source_handle
            .as_ref()
            .ok_or(FfiError::InvalidRequest("source_handle is empty"))?
            .id as FfiHandleId;

        let source = self
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("source not found"))?;

        let source = source
            .downcast_ref::<video_frame::FfiVideoSource>()
            .ok_or(FfiError::InvalidRequest("handle is not a video source"))?;

        let source = source.inner_source().clone();
        let video_track = match source {
            video_frame::VideoSource::Native(native_source) => LocalVideoTrack::create_video_track(
                &create.name,
                create.options.unwrap_or_default().into(),
                native_source,
            ),
        };

        let handle_id = self.next_id() as FfiHandleId;
        let track_info = proto::TrackInfo::from_local_video_track(handle_id, &video_track);

        self.ffi_handles
            .insert(handle_id, Box::new(LocalTrack::Video(video_track)));

        Ok(proto::CreateVideoTrackResponse {
            track: Some(track_info),
        })
    }

    fn on_create_audio_track(
        &'static self,
        create: proto::CreateAudioTrackRequest,
    ) -> FfiResult<proto::CreateAudioTrackResponse> {
        let handle_id = create
            .source_handle
            .as_ref()
            .ok_or(FfiError::InvalidRequest("source_handle is empty"))?
            .id as FfiHandleId;

        let source = self
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("source not found"))?;

        let source = source
            .downcast_ref::<audio_frame::FfiAudioSource>()
            .ok_or(FfiError::InvalidRequest("handle is not an audio source"))?;

        let source = source.inner_source().clone();
        let audio_track = match source {
            audio_frame::AudioSource::Native(native_source) => LocalAudioTrack::create_audio_track(
                &create.name,
                create.options.unwrap_or_default().into(),
                native_source,
            ),
        };

        let handle_id = self.next_id() as FfiHandleId;
        let track_info = proto::TrackInfo::from_local_audio_track(handle_id, &audio_track);

        self.ffi_handles
            .insert(handle_id, Box::new(LocalTrack::Audio(audio_track)));

        Ok(proto::CreateAudioTrackResponse {
            track: Some(track_info),
        })
    }

    // Video

    fn on_alloc_video_buffer(
        &'static self,
        alloc: proto::AllocVideoBufferRequest,
    ) -> FfiResult<proto::AllocVideoBufferResponse> {
        let frame_type = proto::VideoFrameBufferType::from_i32(alloc.r#type).unwrap();
        let buffer: BoxVideoFrameBuffer = match frame_type {
            proto::VideoFrameBufferType::I420 => {
                Box::new(I420Buffer::new(alloc.width, alloc.height))
            }
            _ => return Err(FfiError::InvalidRequest("frame type is not supported")),
        };

        let handle_id = self.next_id();
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &buffer);
        self.ffi_handles.insert(handle_id, Box::new(buffer));

        Ok(proto::AllocVideoBufferResponse {
            buffer: Some(buffer_info),
        })
    }

    fn on_new_video_stream(
        &'static self,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::NewVideoStreamResponse> {
        let stream_info = video_frame::FfiVideoStream::setup(&self, new_stream)?;
        Ok(proto::NewVideoStreamResponse {
            stream: Some(stream_info),
        })
    }

    fn on_new_video_source(
        &'static self,
        new_source: proto::NewVideoSourceRequest,
    ) -> FfiResult<proto::NewVideoSourceResponse> {
        let source_info = video_frame::FfiVideoSource::setup(&self, new_source)?;
        Ok(proto::NewVideoSourceResponse {
            source: Some(source_info),
        })
    }

    fn on_capture_video_frame(
        &'static self,
        push: proto::CaptureVideoFrameRequest,
    ) -> FfiResult<proto::CaptureVideoFrameResponse> {
        let handle_id = push
            .source_handle
            .as_ref()
            .ok_or(FfiError::InvalidRequest("source_handle is empty"))?
            .id as FfiHandleId;

        let video_source = self
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("source not found"))?;

        let video_source = video_source
            .downcast_ref::<video_frame::FfiVideoSource>()
            .ok_or(FfiError::InvalidRequest("handle is not a video source"))?;

        video_source.capture_frame(self, push)?;
        Ok(proto::CaptureVideoFrameResponse::default())
    }

    fn on_to_i420(
        &'static self,
        to_i420: proto::ToI420Request,
    ) -> FfiResult<proto::ToI420Response> {
        let from = to_i420
            .from
            .ok_or(FfiError::InvalidRequest("from is empty"))?;

        let i420 = match from {
            proto::to_i420_request::From::Argb(argb_info) => {
                let mut i420 = I420Buffer::new(argb_info.width, argb_info.height);
                let argb_format = proto::VideoFormatType::from_i32(argb_info.format).unwrap();
                let argb_ptr = argb_info.ptr as *const u8;
                let argb_len = (argb_info.stride * argb_info.height) as usize;
                let argb = unsafe { slice::from_raw_parts(argb_ptr, argb_len) };
                let argb_stride = argb_info.stride;

                let (stride_y, stride_u, stride_v) = i420.strides();
                let (data_y, data_u, data_v) = i420.data_mut();
                let width = argb_info.width as i32;
                let mut height = argb_info.height as i32;
                if to_i420.flip_y {
                    height = -height;
                }

                match argb_format {
                    proto::VideoFormatType::FormatArgb => {
                        yuv_helper::argb_to_i420(
                            argb,
                            argb_stride,
                            data_y,
                            stride_y,
                            data_u,
                            stride_u,
                            data_v,
                            stride_v,
                            width,
                            height,
                        )
                        .unwrap();
                    }
                    proto::VideoFormatType::FormatAbgr => {
                        yuv_helper::abgr_to_i420(
                            argb,
                            argb_stride,
                            data_y,
                            stride_y,
                            data_u,
                            stride_u,
                            data_v,
                            stride_v,
                            width,
                            height,
                        )
                        .unwrap();
                    }
                    _ => return Err(FfiError::InvalidRequest("the format is not supported")),
                }

                i420
            }
            proto::to_i420_request::From::Buffer(handle) => {
                let handle_id = handle.id as FfiHandleId;
                let buffer = self
                    .ffi_handles
                    .get(&handle_id)
                    .ok_or(FfiError::InvalidRequest("handle not found"))?;
                let i420 = buffer
                    .downcast_ref::<BoxVideoFrameBuffer>()
                    .ok_or(FfiError::InvalidRequest("handle is not a video buffer"))?
                    .to_i420();

                i420
            }
        };

        let i420: BoxVideoFrameBuffer = Box::new(i420);
        let handle_id = self.next_id() as FfiHandleId;
        let buffer_info = proto::VideoFrameBufferInfo::from(handle_id, &i420);
        self.ffi_handles.insert(handle_id, Box::new(i420));
        Ok(proto::ToI420Response {
            buffer: Some(buffer_info),
        })
    }

    fn on_to_argb(
        &'static self,
        to_argb: proto::ToArgbRequest,
    ) -> FfiResult<proto::ToArgbResponse> {
        let handle_id = to_argb
            .buffer
            .ok_or(FfiError::InvalidRequest("buffer is empty"))?
            .id as FfiHandleId;

        let buffer = self
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("buffer is not found"))?;

        let buffer = buffer
            .downcast_ref::<BoxVideoFrameBuffer>()
            .ok_or(FfiError::InvalidRequest("handle is not a video buffer"))?;

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
    ) -> FfiResult<proto::AllocAudioBufferResponse> {
        let frame = AudioFrame::new(
            alloc.sample_rate,
            alloc.num_channels,
            alloc.samples_per_channel,
        );

        let handle_id = self.next_id() as FfiHandleId;
        let frame_info = proto::AudioFrameBufferInfo::from(handle_id, &frame);
        self.ffi_handles.insert(handle_id, Box::new(frame));

        Ok(proto::AllocAudioBufferResponse {
            buffer: Some(frame_info),
        })
    }

    fn on_new_audio_stream(
        &'static self,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::NewAudioStreamResponse> {
        let stream_info = audio_frame::FfiAudioSream::setup(self, new_stream)?;
        Ok(proto::NewAudioStreamResponse {
            stream: Some(stream_info),
        })
    }

    fn on_new_audio_source(
        &'static self,
        new_source: proto::NewAudioSourceRequest,
    ) -> FfiResult<proto::NewAudioSourceResponse> {
        let source_info = audio_frame::FfiAudioSource::setup(self, new_source)?;
        Ok(proto::NewAudioSourceResponse {
            source: Some(source_info),
        })
    }

    fn on_capture_audio_frame(
        &'static self,
        push: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<proto::CaptureAudioFrameResponse> {
        let handle_id = push
            .source_handle
            .as_ref()
            .ok_or(FfiError::InvalidRequest("handle is empty"))?
            .id as FfiHandleId;

        let audio_source = self
            .ffi_handles
            .get(&handle_id)
            .ok_or(FfiError::InvalidRequest("audio_source not found"))?;

        let audio_source = audio_source
            .downcast_ref::<audio_frame::FfiAudioSource>()
            .ok_or(FfiError::InvalidRequest("handle is not a video source"))?;

        audio_source.capture_frame(self, push)?;
        Ok(proto::CaptureAudioFrameResponse::default())
    }

    pub fn handle_request(
        &'static self,
        request: proto::FfiRequest,
    ) -> FfiResult<proto::FfiResponse> {
        let request = request
            .message
            .ok_or(FfiError::InvalidRequest("message is empty"))?;

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
            proto::ffi_request::Message::CaptureVideoFrame(push) => {
                proto::ffi_response::Message::CaptureVideoFrame(self.on_capture_video_frame(push)?)
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
            proto::ffi_request::Message::CaptureAudioFrame(push) => {
                proto::ffi_response::Message::CaptureAudioFrame(self.on_capture_audio_frame(push)?)
            }
        });

        Ok(res)
    }
}
