use super::{
    audio_source, audio_stream, participant, publication, room, track, video_source, video_stream,
    FfiConfig, FfiError, FfiResult, FfiServer,
};
use crate::proto;
use livekit::prelude::*;
use livekit::webrtc::native::{audio_resampler, yuv_helper};
use livekit::webrtc::prelude::*;
use livekit::webrtc::video_frame::{native::I420BufferExt, BoxVideoFrameBuffer, I420Buffer};
use parking_lot::Mutex;
use std::slice;
use std::sync::Arc;

impl FfiServer {
    /// This is the first request called by the foreign language
    /// It sets the callback function to be called when an event is received
    fn on_initialize(
        &'static self,
        init: proto::InitializeRequest,
    ) -> FfiResult<proto::InitializeResponse> {
        if self.config.lock().is_some() {
            return Err(FfiError::AlreadyInitialized);
        }

        // # SAFETY: The foreign language is responsible for ensuring that the callback function is valid
        *self.config.lock() = Some(FfiConfig {
            callback_fn: unsafe { std::mem::transmute(init.event_callback_ptr as usize) },
        });

        Ok(proto::InitializeResponse::default())
    }

    /// Dispose the server, close all rooms and clean up all handles
    /// It is not mandatory to call this function.
    fn on_dispose(
        &'static self,
        dispose: proto::DisposeRequest,
    ) -> FfiResult<proto::DisposeResponse> {
        *self.config.lock() = None;

        if !dispose.r#async {
            self.async_runtime.block_on(self.dispose());
            Ok(proto::DisposeResponse::default())
        } else {
            todo!("async dispose");
        }
    }

    /// Connect to a room, and start listening for events
    /// The returned room_handle is used to interact with the room and to
    /// recognized the incoming events
    fn on_connect(
        &'static self,
        connect: proto::ConnectRequest,
    ) -> FfiResult<proto::ConnectResponse> {
        let async_id = self.next_id();
        self.async_runtime.spawn(async move {
            let res = room::FfiRoom::connect(&self, connect).await;
            let _ = self
                .send_event(proto::ffi_event::Message::Connect(proto::ConnectCallback {
                    async_id,
                    error: res.err().map(|e| e.to_string()),
                    room: res.ok().map(|r| r.into()),
                }))
                .await;
        });

        Ok(proto::ConnectResponse { async_id })
    }

    /// Disconnect to a room
    fn on_disconnect(
        &'static self,
        disconnect: proto::DisconnectRequest,
    ) -> FfiResult<proto::DisconnectResponse> {
        let async_id = self.next_id();
        self.async_runtime.spawn(async move {
            let ffi_room = self
                .retrieve_handle::<room::FfiRoom>(disconnect.room_handle)
                .unwrap();

            ffi_room.close().await;

            let _ = self
                .send_event(proto::ffi_event::Message::Disconnect(
                    proto::DisconnectCallback { async_id },
                ))
                .await;
        });

        Ok(proto::DisconnectResponse { async_id })
    }

    fn on_publish_track(
        &'static self,
        publish: proto::PublishTrackRequest,
    ) -> FfiResult<proto::PublishTrackResponse> {
        let async_id = self.next_id();
        self.async_runtime.spawn(async move {
            let res = async {
                let participant = self.retrieve_handle::<participant::FfiParticipant>(
                    publish.local_participant_handle,
                )?;

                let Participant::Local(participant) = participant.participant() else {
                    return Err(FfiError::InvalidRequest("participant is not a LocalParticipant"));
                };

                let track = self.retrieve_handle::<track::FfiTrack>(publish.track_handle)?;
                let track = LocalTrack::try_from(track.track().clone())
                    .map_err(|_| FfiError::InvalidRequest("track is not a LocalTrack"))?;

                let publication = participant
                    .publish_track(track, publish.options.map(Into::into).unwrap_or_default())
                    .await?;

                Ok::<LocalTrackPublication, FfiError>(publication)
            }
            .await;

            let handle_id = self.next_id();
            if let Ok(publication) = res.as_ref() {
                let publication = publication::FfiPublication {
                    handle: handle_id,
                    publication: TrackPublication::Local(publication.clone()),
                };

                self.store_handle(handle_id, publication);
            }

            let _ = self
                .send_event(proto::ffi_event::Message::PublishTrack(
                    proto::PublishTrackCallback {
                        async_id,
                        error: res.as_ref().err().map(|e| e.to_string()),
                        track_sid: res.ok().map(|p| p.sid().to_owned()).unwrap_or_default(),
                    },
                ))
                .await;
        });

        Ok(proto::PublishTrackResponse { async_id })
    }

    fn on_unpublish_track(
        &'static self,
        _unpublish: proto::UnpublishTrackRequest,
    ) -> FfiResult<proto::UnpublishTrackResponse> {
        Ok(proto::UnpublishTrackResponse::default())
    }

    fn on_publish_data(
        &'static self,
        publish: proto::PublishDataRequest,
    ) -> FfiResult<proto::PublishDataResponse> {
        // Push the data to an async queue (avoid blocking and keep the order)
        let local_participant =
            self.retrieve_handle::<participant::FfiParticipant>(publish.local_participant_handle)?;

        local_participant.room().publish_data(self, publish)
    }

    fn on_set_subscribed(
        &'static self,
        set_subscribed: proto::SetSubscribedRequest,
    ) -> FfiResult<proto::SetSubscribedResponse> {
        let publication =
            self.retrieve_handle::<publication::FfiPublication>(set_subscribed.publication_handle)?;

        let TrackPublication::Remote(publication) = publication.publication() else {
            return Err(FfiError::InvalidRequest("publication is not a RemotePublication"));
        };

        publication.set_subscribed(set_subscribed.subscribe);
        Ok(proto::SetSubscribedResponse {})
    }

    // Track
    fn on_create_video_track(
        &'static self,
        create: proto::CreateVideoTrackRequest,
    ) -> FfiResult<proto::CreateVideoTrackResponse> {
        let source = self
            .retrieve_handle::<video_source::FfiVideoSource>(create.source_handle)?
            .inner_source()
            .clone();

        let handle_id = self.next_id();
        let video_track = LocalVideoTrack::create_video_track(&create.name, source);
        let track_info = proto::TrackInfo::from_local_video_track(
            proto::FfiOwnedHandle { id: handle_id },
            &video_track,
        );

        self.store_handle(
            handle_id,
            track::FfiTrack {
                handle: handle_id,
                track: Track::LocalVideo(video_track),
            },
        );

        Ok(proto::CreateVideoTrackResponse {
            track: Some(track_info),
        })
    }

    fn on_create_audio_track(
        &'static self,
        create: proto::CreateAudioTrackRequest,
    ) -> FfiResult<proto::CreateAudioTrackResponse> {
        let source = self
            .retrieve_handle::<audio_source::FfiAudioSource>(create.source_handle)?
            .inner_source()
            .clone();

        let handle_id = self.next_id();
        let audio_track = LocalAudioTrack::create_audio_track(&create.name, source);
        let track_info = proto::TrackInfo::from_local_audio_track(
            proto::FfiOwnedHandle { id: handle_id },
            &audio_track,
        );

        self.store_handle(
            handle_id,
            track::FfiTrack {
                handle: handle_id,
                track: Track::LocalAudio(audio_track),
            },
        );

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
        let buffer_info =
            proto::VideoFrameBufferInfo::from(proto::FfiOwnedHandle { id: handle_id }, &buffer);

        self.store_handle(handle_id, buffer);
        Ok(proto::AllocVideoBufferResponse {
            buffer: Some(buffer_info),
        })
    }

    fn on_new_video_stream(
        &'static self,
        new_stream: proto::NewVideoStreamRequest,
    ) -> FfiResult<proto::NewVideoStreamResponse> {
        let stream_info = video_stream::FfiVideoStream::setup(&self, new_stream)?;
        Ok(proto::NewVideoStreamResponse {
            stream: Some(stream_info),
        })
    }

    fn on_new_video_source(
        &'static self,
        new_source: proto::NewVideoSourceRequest,
    ) -> FfiResult<proto::NewVideoSourceResponse> {
        let source_info = video_source::FfiVideoSource::setup(&self, new_source)?;
        Ok(proto::NewVideoSourceResponse {
            source: Some(source_info),
        })
    }

    fn on_capture_video_frame(
        &'static self,
        push: proto::CaptureVideoFrameRequest,
    ) -> FfiResult<proto::CaptureVideoFrameResponse> {
        let source = self.retrieve_handle::<video_source::FfiVideoSource>(push.source_handle)?;
        source.capture_frame(self, push)?;
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
            proto::to_i420_request::From::Argb(info) => {
                let argb = unsafe {
                    let len = (info.stride * info.height) as usize;
                    slice::from_raw_parts(info.ptr as *const u8, len)
                };

                let w = info.width as i32;
                let mut h = info.height as i32;
                if to_i420.flip_y {
                    h = -h;
                }

                // Create a new I420 buffer
                let mut i420 = I420Buffer::new(info.width, info.height);
                let (sy, su, sv) = i420.strides();
                let (dy, du, dv) = i420.data_mut();

                match proto::VideoFormatType::from_i32(info.format).unwrap() {
                    proto::VideoFormatType::FormatArgb => {
                        yuv_helper::argb_to_i420(argb, info.stride, dy, sy, du, su, dv, sv, w, h)
                            .unwrap();
                    }
                    proto::VideoFormatType::FormatAbgr => {
                        yuv_helper::abgr_to_i420(argb, info.stride, dy, sy, du, su, dv, sv, w, h)
                            .unwrap();
                    }
                    _ => return Err(FfiError::InvalidRequest("the format is not supported")),
                }

                i420
            }

            proto::to_i420_request::From::BufferHandle(handle) => self
                .retrieve_handle::<BoxVideoFrameBuffer>(handle)?
                .to_i420(),
        };

        let i420: BoxVideoFrameBuffer = Box::new(i420);
        let handle_id = self.next_id();
        let buffer_info =
            proto::VideoFrameBufferInfo::from(proto::FfiOwnedHandle { id: handle_id }, &i420);
        self.ffi_handles.insert(handle_id, Box::new(i420));
        Ok(proto::ToI420Response {
            buffer: Some(buffer_info),
        })
    }

    fn on_to_argb(
        &'static self,
        to_argb: proto::ToArgbRequest,
    ) -> FfiResult<proto::ToArgbResponse> {
        let buffer = self.retrieve_handle::<BoxVideoFrameBuffer>(to_argb.buffer_handle)?;

        let argb = unsafe {
            slice::from_raw_parts_mut(
                to_argb.dst_ptr as *mut u8,
                (to_argb.dst_stride * to_argb.dst_height) as usize,
            )
        };

        let w = to_argb.dst_width as i32;
        let mut h = to_argb.dst_height as i32;
        if to_argb.flip_y {
            h = -h;
        }

        buffer
            .to_argb(
                proto::VideoFormatType::from_i32(to_argb.dst_format)
                    .unwrap()
                    .into(),
                argb,
                to_argb.dst_stride,
                w,
                h,
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

        let handle_id = self.next_id();
        let frame_info =
            proto::AudioFrameBufferInfo::from(proto::FfiOwnedHandle { id: handle_id }, &frame);
        self.ffi_handles.insert(handle_id, Box::new(frame));

        Ok(proto::AllocAudioBufferResponse {
            buffer: Some(frame_info),
        })
    }

    fn on_new_audio_stream(
        &'static self,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::NewAudioStreamResponse> {
        let stream_info = audio_stream::FfiAudioStream::setup(self, new_stream)?;
        Ok(proto::NewAudioStreamResponse {
            stream: Some(stream_info),
        })
    }

    fn on_new_audio_source(
        &'static self,
        new_source: proto::NewAudioSourceRequest,
    ) -> FfiResult<proto::NewAudioSourceResponse> {
        let source_info = audio_source::FfiAudioSource::setup(self, new_source)?;
        Ok(proto::NewAudioSourceResponse {
            source: Some(source_info),
        })
    }

    fn on_capture_audio_frame(
        &'static self,
        push: proto::CaptureAudioFrameRequest,
    ) -> FfiResult<proto::CaptureAudioFrameResponse> {
        let source = self
            .retrieve_handle::<audio_source::FfiAudioSource>(push.source_handle)?
            .clone();

        source.capture_frame(self, push)?;
        Ok(proto::CaptureAudioFrameResponse::default())
    }

    fn new_audio_resampler(
        &'static self,
        _: proto::NewAudioResamplerRequest,
    ) -> FfiResult<proto::NewAudioResamplerResponse> {
        let resampler = audio_resampler::AudioResampler::default();
        let resampler = Arc::new(Mutex::new(resampler));

        let handle_id = self.next_id();
        self.store_handle(handle_id, resampler);

        Ok(proto::NewAudioResamplerResponse {
            resampler: Some(proto::AudioResamplerInfo {
                handle: Some(proto::FfiOwnedHandle { id: handle_id }),
            }),
        })
    }

    fn remix_and_resample(
        &'static self,
        remix: proto::RemixAndResampleRequest,
    ) -> FfiResult<proto::RemixAndResampleResponse> {
        let resampler = self
            .retrieve_handle::<Arc<Mutex<audio_resampler::AudioResampler>>>(remix.resampler_handle)?
            .clone();

        let buffer = self.retrieve_handle::<AudioFrame>(remix.buffer_handle)?;
        let data = resampler
            .lock()
            .remix_and_resample(
                &buffer.data,
                buffer.samples_per_channel,
                buffer.num_channels,
                buffer.sample_rate,
                remix.num_channels,
                remix.sample_rate,
            )
            .to_owned();
        drop(buffer);

        let audio_frame = AudioFrame {
            data,
            num_channels: remix.num_channels,
            samples_per_channel: (data.len() / remix.num_channels as usize) as u32,
            sample_rate: remix.sample_rate,
        };

        let handle_id = self.next_id();
        let buffer_info = proto::AudioFrameBufferInfo::from(
            proto::FfiOwnedHandle { id: handle_id },
            &audio_frame,
        );
        self.store_handle(handle_id, audio_frame);

        Ok(proto::RemixAndResampleResponse {
            buffer: Some(buffer_info),
        })
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
            proto::ffi_request::Message::PublishData(publish) => {
                proto::ffi_response::Message::PublishData(self.on_publish_data(publish)?)
            }
            proto::ffi_request::Message::SetSubscribed(subscribed) => {
                proto::ffi_response::Message::SetSubscribed(self.on_set_subscribed(subscribed)?)
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
            proto::ffi_request::Message::NewAudioResampler(new_res) => {
                proto::ffi_response::Message::NewAudioResampler(self.new_audio_resampler(new_res)?)
            }
            proto::ffi_request::Message::RemixAndResample(remix) => {
                proto::ffi_response::Message::RemixAndResample(self.remix_and_resample(remix)?)
            }
        });

        Ok(res)
    }
}
