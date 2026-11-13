// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! FFI bindings for livekit-capture sources.
//!
//! A capture source owns its producer (e.g. a GStreamer pipeline) and the
//! pump that feeds an RTC video source, so frames never cross the FFI
//! boundary. The RTC source is exposed as a regular [`FfiVideoSource`], so
//! the existing `CreateVideoTrack`/`PublishTrack` requests work unchanged.

#[cfg(feature = "capture-gstreamer")]
use livekit_capture::playout::{
    GStreamerAudioPlayout, GStreamerAudioPlayoutConfig, GStreamerAudioPlayoutControl,
};
use livekit_capture::{
    audio::{AudioPump, PcmAudioSource},
    encoded::{EncodedVideoPump, EncodedVideoSource},
    pixel::{PixelVideoPump, PixelVideoSource},
    pump::{PumpError, PumpExit, PumpStats, PumpStop, RunningPump},
};
use parking_lot::Mutex;

#[cfg(feature = "capture-clock")]
use livekit_capture::sources::clock::ClockVideoSource;
#[cfg(feature = "capture-pattern")]
use livekit_capture::sources::pattern::PatternVideoSource;
#[cfg(feature = "capture-gstreamer")]
use livekit_capture::sources::{
    gstreamer::GStreamerVideoSource, gstreamer_audio::GStreamerAudioSource,
};

use super::{
    audio_source::FfiAudioSource, room::FfiTrack, video_source::FfiVideoSource, FfiHandle,
    FfiServer,
};
#[cfg(feature = "capture-gstreamer")]
use crate::conversion::capture::gstreamer_config_from_proto;
#[cfg(feature = "capture-pattern")]
use crate::conversion::capture::pattern_config_from_proto;
use crate::{conversion::capture::video_codec_to_proto, proto, FfiError, FfiHandleId, FfiResult};

/// A capture pump of either kind, boxed at the FFI edge.
enum CapturePump {
    Audio(AudioPump<Box<dyn PcmAudioSource>>),
    Pixel(PixelVideoPump<Box<dyn PixelVideoSource>>),
    Encoded(EncodedVideoPump<Box<dyn EncodedVideoSource>>),
}

impl CapturePump {
    fn spawn(self) -> std::io::Result<RunningPump> {
        match self {
            Self::Audio(pump) => pump.spawn(),
            Self::Pixel(pump) => pump.spawn(),
            Self::Encoded(pump) => pump.spawn(),
        }
    }
}

/// State of the capture activity owned by an [`FfiCaptureSource`].
///
/// The activity can end without client action (end of stream, error); the
/// FFI object outlives it and is disposed only by the client.
enum CaptureState {
    /// Created but not started.
    Idle(CapturePump),
    /// Started.
    Running(RunningPump),
    /// Joined or failed to start.
    Finished,
}

pub struct FfiCaptureSource {
    pub handle_id: FfiHandleId,
    stop: PumpStop,
    state: Mutex<CaptureState>,
}

impl FfiHandle for FfiCaptureSource {}

#[cfg(feature = "capture-gstreamer")]
struct FfiAudioPlayout {
    playout: Mutex<Option<GStreamerAudioPlayout>>,
    control: GStreamerAudioPlayoutControl,
}

#[cfg(feature = "capture-gstreamer")]
impl FfiHandle for FfiAudioPlayout {}

pub fn on_new_capture_source(
    server: &'static FfiServer,
    request: proto::NewCaptureSourceRequest,
) -> FfiResult<proto::NewCaptureSourceResponse> {
    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        let message = match create_capture_source(server, request).await {
            Ok(source) => proto::new_capture_source_callback::Message::Source(source),
            Err(err) => proto::new_capture_source_callback::Message::Error(err.to_string()),
        };
        let _ = server.send_event(proto::ffi_event::Message::NewCaptureSource(
            proto::NewCaptureSourceCallback { async_id, message: Some(message) },
        ));
    });
    Ok(proto::NewCaptureSourceResponse { async_id })
}

async fn create_capture_source(
    server: &'static FfiServer,
    request: proto::NewCaptureSourceRequest,
) -> FfiResult<proto::OwnedCaptureSource> {
    let config =
        request.config.ok_or(FfiError::InvalidRequest("missing capture source config".into()))?;

    let pump = match config {
        #[cfg(feature = "capture-gstreamer")]
        proto::new_capture_source_request::Config::Gstreamer(config) => {
            let source = GStreamerVideoSource::new(gstreamer_config_from_proto(config)?)
                .await
                .map_err(|err| FfiError::InvalidRequest(err.to_string().into()))?;
            let source: Box<dyn EncodedVideoSource> = Box::new(source);
            CapturePump::Encoded(EncodedVideoPump::new(source))
        }
        #[cfg(feature = "capture-pattern")]
        proto::new_capture_source_request::Config::Pattern(config) => {
            let source = PatternVideoSource::new(pattern_config_from_proto(config)?)
                .await
                .map_err(|err| FfiError::InvalidRequest(err.to_string().into()))?;
            let source: Box<dyn PixelVideoSource> = Box::new(source);
            CapturePump::Pixel(PixelVideoPump::new(source))
        }
        #[cfg(feature = "capture-clock")]
        proto::new_capture_source_request::Config::Clock(config) => {
            let source = ClockVideoSource::new(config.into())
                .await
                .map_err(|err| FfiError::InvalidRequest(err.to_string().into()))?;
            let source: Box<dyn PixelVideoSource> = Box::new(source);
            CapturePump::Pixel(PixelVideoPump::new(source))
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(FfiError::InvalidRequest(
                "capture source is not enabled in this build".into(),
            ))
        }
    };

    Ok(register_capture_source(server, pump))
}

#[cfg(feature = "capture-gstreamer")]
pub fn on_new_audio_capture_source(
    server: &'static FfiServer,
    request: proto::NewAudioCaptureSourceRequest,
) -> FfiResult<proto::NewAudioCaptureSourceResponse> {
    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        let config = request.gstreamer;
        let result = GStreamerAudioSource::new(
            livekit_capture::sources::gstreamer_audio::GStreamerAudioSourceConfig {
                pipeline: config.pipeline,
                sample_rate: config.sample_rate.unwrap_or(48_000),
                num_channels: config.num_channels.unwrap_or(1),
                frame_duration_ms: config.frame_duration_ms.unwrap_or(10),
            },
        )
        .map(|source| {
            let source: Box<dyn PcmAudioSource> = Box::new(source);
            register_audio_capture_source(server, AudioPump::new(source))
        });
        let message = match result {
            Ok(source) => proto::new_audio_capture_source_callback::Message::Source(source),
            Err(error) => {
                proto::new_audio_capture_source_callback::Message::Error(error.to_string())
            }
        };
        let _ = server.send_event(proto::ffi_event::Message::NewAudioCaptureSource(
            proto::NewAudioCaptureSourceCallback { async_id, message: Some(message) },
        ));
    });
    Ok(proto::NewAudioCaptureSourceResponse { async_id })
}

#[cfg(feature = "capture-gstreamer")]
fn register_audio_capture_source(
    server: &'static FfiServer,
    pump: AudioPump<Box<dyn PcmAudioSource>>,
) -> proto::OwnedAudioCaptureSource {
    let format = pump.format();
    let stop = pump.stop_handle();
    let source_handle = server.next_id();
    let ffi_source = FfiAudioSource {
        handle_id: source_handle,
        source_type: proto::AudioSourceType::AudioSourceNative,
        source: pump.rtc_source(),
    };
    let source_info = proto::AudioSourceInfo::from(&ffi_source);
    server.store_handle(source_handle, ffi_source);

    let capture_handle = server.next_id();
    server.store_handle(
        capture_handle,
        FfiCaptureSource {
            handle_id: capture_handle,
            stop,
            state: Mutex::new(CaptureState::Idle(CapturePump::Audio(pump))),
        },
    );

    proto::OwnedAudioCaptureSource {
        handle: proto::FfiOwnedHandle { id: capture_handle },
        info: proto::AudioCaptureSourceInfo {
            sample_rate: format.sample_rate,
            num_channels: format.num_channels,
            samples_per_channel: format.samples_per_channel,
            audio_source: proto::OwnedAudioSource {
                handle: proto::FfiOwnedHandle { id: source_handle },
                info: source_info,
            },
        },
    }
}

#[cfg(feature = "capture-gstreamer")]
pub fn on_new_audio_playout(
    server: &'static FfiServer,
    request: proto::NewAudioPlayoutRequest,
) -> FfiResult<proto::NewAudioPlayoutResponse> {
    let tracks = request
        .track_handles
        .iter()
        .map(|handle| {
            let track = server.retrieve_handle::<FfiTrack>(*handle)?;
            match &track.track {
                livekit::track::Track::RemoteAudio(track) => Ok(track.rtc_track()),
                _ => Err(FfiError::InvalidRequest(
                    "audio playout track handles must reference remote audio tracks".into(),
                )),
            }
        })
        .collect::<FfiResult<Vec<_>>>()?;
    let playout = match GStreamerAudioPlayout::start(
        GStreamerAudioPlayoutConfig {
            pipeline: request.pipeline,
            sample_rate: request.sample_rate.unwrap_or(48_000),
            num_channels: request.num_channels.unwrap_or(1),
            frame_duration_ms: request.frame_duration_ms.unwrap_or(10),
        },
        tracks,
    ) {
        Ok(playout) => playout,
        Err(error) => {
            return Ok(proto::NewAudioPlayoutResponse {
                playout: None,
                error: Some(error.to_string()),
            })
        }
    };
    let handle = server.next_id();
    let control = playout.control();
    server.store_handle(handle, FfiAudioPlayout { playout: Mutex::new(Some(playout)), control });
    Ok(proto::NewAudioPlayoutResponse {
        playout: Some(proto::FfiOwnedHandle { id: handle }),
        error: None,
    })
}

#[cfg(feature = "capture-gstreamer")]
pub fn on_attach_audio_playout_track(
    server: &'static FfiServer,
    request: proto::AttachAudioPlayoutTrackRequest,
) -> FfiResult<proto::AttachAudioPlayoutTrackResponse> {
    let control =
        server.retrieve_handle::<FfiAudioPlayout>(request.playout_handle)?.control.clone();
    let track = server.retrieve_handle::<FfiTrack>(request.track_handle)?;
    let track = match &track.track {
        livekit::track::Track::RemoteAudio(track) => track.rtc_track(),
        _ => {
            return Err(FfiError::InvalidRequest(
                "audio playout track handle must reference a remote audio track".into(),
            ))
        }
    };
    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        let (track_id, error) = match control.attach_track(track).await {
            Ok(track_id) => (Some(track_id), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let _ = server.send_event(proto::ffi_event::Message::AttachAudioPlayoutTrack(
            proto::AttachAudioPlayoutTrackCallback { async_id, error, track_id },
        ));
    });
    Ok(proto::AttachAudioPlayoutTrackResponse { async_id })
}

#[cfg(feature = "capture-gstreamer")]
pub fn on_detach_audio_playout_track(
    server: &'static FfiServer,
    request: proto::DetachAudioPlayoutTrackRequest,
) -> FfiResult<proto::DetachAudioPlayoutTrackResponse> {
    let control =
        server.retrieve_handle::<FfiAudioPlayout>(request.playout_handle)?.control.clone();
    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        let (detached, error) = match control.detach_track(&request.track_id).await {
            Ok(detached) => (Some(detached), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let _ = server.send_event(proto::ffi_event::Message::DetachAudioPlayoutTrack(
            proto::DetachAudioPlayoutTrackCallback { async_id, error, detached },
        ));
    });
    Ok(proto::DetachAudioPlayoutTrackResponse { async_id })
}

#[cfg(feature = "capture-gstreamer")]
pub fn on_stop_audio_playout(
    server: &'static FfiServer,
    request: proto::StopAudioPlayoutRequest,
) -> FfiResult<proto::StopAudioPlayoutResponse> {
    let playout = server.take_handle::<FfiAudioPlayout>(request.playout_handle)?;
    let playout = playout
        .playout
        .lock()
        .take()
        .ok_or(FfiError::InvalidRequest("audio playout is already stopped".into()))?;
    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        let callback = match playout.stop().await {
            Ok(stats) => proto::StopAudioPlayoutCallback {
                async_id,
                error: None,
                frames_pushed: Some(stats.frames_pushed),
                frames_dropped: Some(stats.frames_dropped),
                bytes_pushed: Some(stats.bytes_pushed),
            },
            Err(error) => proto::StopAudioPlayoutCallback {
                async_id,
                error: Some(error.to_string()),
                frames_pushed: None,
                frames_dropped: None,
                bytes_pushed: None,
            },
        };
        let _ = server.send_event(proto::ffi_event::Message::StopAudioPlayout(callback));
    });
    Ok(proto::StopAudioPlayoutResponse { async_id })
}

/// Stores the capture source and its RTC video source as client-owned
/// handles.
fn register_capture_source(
    server: &'static FfiServer,
    pump: CapturePump,
) -> proto::OwnedCaptureSource {
    // The source type must match how the pump built the RTC source, so the
    // FFI video source accepts the matching frame and feedback requests.
    let (kind, source_type, resolution, codec, publish_options, rtc_source, stop) = match &pump {
        CapturePump::Audio(_) => unreachable!("audio pumps use register_audio_capture_source"),
        CapturePump::Pixel(pump) => (
            proto::CaptureSourceKind::CaptureSourcePixel,
            proto::VideoSourceType::VideoSourceNative,
            pump.source().resolution(),
            None,
            pump.publish_options(),
            pump.rtc_source(),
            pump.stop_handle(),
        ),
        CapturePump::Encoded(pump) => (
            proto::CaptureSourceKind::CaptureSourceEncoded,
            proto::VideoSourceType::VideoSourceEncoded,
            pump.source().resolution(),
            Some(pump.source().codec()),
            pump.publish_options(),
            pump.rtc_source(),
            pump.stop_handle(),
        ),
    };

    // The RTC source is a regular client-owned handle, used with the
    // existing CreateVideoTrack request.
    let source_handle_id = server.next_id();
    let video_source =
        FfiVideoSource { handle_id: source_handle_id, source_type, source: rtc_source };
    let video_source_info = proto::VideoSourceInfo::from(&video_source);
    server.store_handle(source_handle_id, video_source);

    let info = proto::CaptureSourceInfo {
        kind: kind.into(),
        resolution: proto::VideoSourceResolution {
            width: resolution.width,
            height: resolution.height,
        },
        codec: codec.and_then(video_codec_to_proto).map(Into::into),
        recommended_publish_options: recommended_publish_options_to_proto(&publish_options),
        video_source: proto::OwnedVideoSource {
            handle: proto::FfiOwnedHandle { id: source_handle_id },
            info: video_source_info,
        },
    };

    let capture_handle_id = server.next_id();
    server.store_handle(
        capture_handle_id,
        FfiCaptureSource {
            handle_id: capture_handle_id,
            stop,
            state: Mutex::new(CaptureState::Idle(pump)),
        },
    );

    proto::OwnedCaptureSource { handle: proto::FfiOwnedHandle { id: capture_handle_id }, info }
}

/// Maps the pump-derived publish options into the proto options the client
/// merges its own settings over.
fn recommended_publish_options_to_proto(
    options: &livekit::options::TrackPublishOptions,
) -> proto::TrackPublishOptions {
    use livekit::options::VideoCodec;
    let video_codec = match options.video_codec {
        VideoCodec::VP8 => proto::VideoCodec::Vp8,
        VideoCodec::H264 => proto::VideoCodec::H264,
        VideoCodec::AV1 => proto::VideoCodec::Av1,
        VideoCodec::VP9 => proto::VideoCodec::Vp9,
        VideoCodec::H265 => proto::VideoCodec::H265,
    };
    let video_encoder = match options.video_encoder {
        livekit::options::VideoEncoderBackend::PreEncoded => {
            Some(proto::VideoEncoderBackend::EncoderBackendPreEncoded.into())
        }
        _ => None,
    };
    proto::TrackPublishOptions {
        video_codec: Some(video_codec.into()),
        video_encoder,
        simulcast: Some(options.simulcast),
        ..Default::default()
    }
}

pub fn on_start_capture(
    server: &'static FfiServer,
    request: proto::StartCaptureRequest,
) -> FfiResult<proto::StartCaptureResponse> {
    let capture_handle = request.capture_handle;
    let ffi_capture = server.retrieve_handle::<FfiCaptureSource>(capture_handle)?;

    let mut state = ffi_capture.state.lock();
    let pump = match std::mem::replace(&mut *state, CaptureState::Finished) {
        CaptureState::Idle(pump) => pump,
        other => {
            let error = match &other {
                CaptureState::Running(_) => "capture is already started",
                _ => "capture has already finished",
            };
            *state = other;
            return Ok(proto::StartCaptureResponse { error: Some(error.to_owned()) });
        }
    };

    let running = match pump.spawn() {
        Ok(running) => running,
        Err(err) => {
            return Ok(proto::StartCaptureResponse {
                error: Some(format!("failed to start capture: {err}")),
            });
        }
    };
    *state = CaptureState::Running(running);

    Ok(proto::StartCaptureResponse { error: None })
}

impl From<Result<PumpStats, PumpError>> for proto::join_capture_callback::Message {
    fn from(result: Result<PumpStats, PumpError>) -> Self {
        match result {
            Ok(stats) => {
                let exit = match stats.exit {
                    PumpExit::Stopped => proto::CaptureExit::Stopped,
                    PumpExit::EndOfStream => proto::CaptureExit::EndOfStream,
                };
                Self::Finished(proto::CaptureFinished {
                    frames_captured: stats.frames_captured,
                    exit: exit.into(),
                    samples_captured: Some(stats.samples_captured),
                    bytes_captured: Some(stats.bytes_captured),
                    first_timestamp_ns: stats.first_timestamp_ns,
                    last_timestamp_ns: stats.last_timestamp_ns,
                })
            }
            Err(err) => {
                let kind = match err {
                    PumpError::Source(_) => proto::CaptureErrorKind::Source,
                    PumpError::CaptureFailed => proto::CaptureErrorKind::Rtc,
                    PumpError::Panicked(_) => proto::CaptureErrorKind::Panicked,
                    _ => proto::CaptureErrorKind::Source,
                };
                Self::Error(proto::CaptureError { error: err.to_string(), kind: kind.into() })
            }
        }
    }
}

pub fn on_stop_capture(
    server: &'static FfiServer,
    request: proto::StopCaptureRequest,
) -> FfiResult<proto::StopCaptureResponse> {
    let ffi_capture = server.retrieve_handle::<FfiCaptureSource>(request.capture_handle)?;
    ffi_capture.stop.stop();
    Ok(proto::StopCaptureResponse { error: None })
}

pub fn on_join_capture(
    server: &'static FfiServer,
    request: proto::JoinCaptureRequest,
) -> FfiResult<proto::JoinCaptureResponse> {
    let capture_handle = request.capture_handle;
    let finished = {
        let ffi_capture = server.retrieve_handle::<FfiCaptureSource>(capture_handle)?;
        let CaptureState::Running(running) = &*ffi_capture.state.lock() else {
            return Err(FfiError::InvalidRequest("capture is not running".into()));
        };
        running.finished()
    };

    let async_id = server.resolve_async_id(request.request_async_id);
    server.async_runtime.spawn(async move {
        finished.await;
        let running = match server.retrieve_handle::<FfiCaptureSource>(capture_handle) {
            Ok(ffi_capture) => {
                match std::mem::replace(&mut *ffi_capture.state.lock(), CaptureState::Finished) {
                    CaptureState::Running(running) => Ok(running),
                    _ => Err("capture was already joined by another request"),
                }
            }
            Err(_) => Err("capture source was disposed"),
        };
        let message = match running {
            Ok(running) => running.join().await.into(),
            Err(error) => proto::join_capture_callback::Message::Error(proto::CaptureError {
                error: error.into(),
                kind: proto::CaptureErrorKind::Source.into(),
            }),
        };
        let _ =
            server.send_event(proto::ffi_event::Message::JoinCapture(proto::JoinCaptureCallback {
                async_id,
                message: Some(message),
            }));
    });

    Ok(proto::JoinCaptureResponse { async_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{server::FfiConfig, FFI_SERVER};
    use livekit::webrtc::video_frame::BoxVideoFrame;
    use livekit_capture::{
        encoded::{EncodedVideoCodec, OwnedEncodedAccessUnit},
        error::SourceError,
        primitive::VideoResolution,
    };
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };

    fn server() -> &'static FfiServer {
        &FFI_SERVER
    }

    struct DropObservedSource {
        entered: Option<mpsc::Sender<()>>,
        dropped: mpsc::Sender<()>,
    }

    impl Drop for DropObservedSource {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }

    impl PixelVideoSource for DropObservedSource {
        fn resolution(&self) -> VideoResolution {
            VideoResolution { width: 64, height: 36 }
        }

        fn next_frame(&mut self, stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
            }
            while !stop.is_stopped() {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(50));
            Ok(None)
        }
    }

    struct IdleEncodedSource;

    impl EncodedVideoSource for IdleEncodedSource {
        fn resolution(&self) -> VideoResolution {
            VideoResolution { width: 64, height: 36 }
        }

        fn codec(&self) -> EncodedVideoCodec {
            EncodedVideoCodec::H264
        }

        fn next_access_unit(
            &mut self,
            _stop: &PumpStop,
        ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
            Ok(None)
        }
    }

    #[serial_test::serial]
    #[test]
    fn capture_video_source_type_matches_pump_kind() {
        server().setup(FfiConfig {
            callback_fn: Arc::new(|_| {}),
            capture_logs: false,
            sdk: "livekit-ffi-test".to_owned(),
            sdk_version: "test".to_owned(),
        });

        let (pixel, encoded) = server().async_runtime.block_on(async {
            let (entered, _) = mpsc::channel();
            let (dropped, _) = mpsc::channel();
            let pixel: Box<dyn PixelVideoSource> =
                Box::new(DropObservedSource { entered: Some(entered), dropped });
            let encoded: Box<dyn EncodedVideoSource> = Box::new(IdleEncodedSource);
            (
                register_capture_source(server(), CapturePump::Pixel(PixelVideoPump::new(pixel))),
                register_capture_source(
                    server(),
                    CapturePump::Encoded(EncodedVideoPump::new(encoded)),
                ),
            )
        });

        assert_eq!(pixel.info.kind(), proto::CaptureSourceKind::CaptureSourcePixel);
        assert_eq!(
            pixel.info.video_source.info.r#type(),
            proto::VideoSourceType::VideoSourceNative
        );
        assert_eq!(encoded.info.kind(), proto::CaptureSourceKind::CaptureSourceEncoded);
        assert_eq!(
            encoded.info.video_source.info.r#type(),
            proto::VideoSourceType::VideoSourceEncoded
        );

        // The stored handles must accept only the requests that match their
        // source type.
        let pixel_video =
            server().retrieve_handle::<FfiVideoSource>(pixel.info.video_source.handle.id).unwrap();
        assert!(pixel_video.take_encoded_feedback().is_err());
        let encoded_video = server()
            .retrieve_handle::<FfiVideoSource>(encoded.info.video_source.handle.id)
            .unwrap();
        assert!(encoded_video.take_encoded_feedback().is_ok());
        drop((pixel_video, encoded_video));

        for source in [pixel, encoded] {
            server().drop_handle(source.handle.id);
            server().drop_handle(source.info.video_source.handle.id);
        }
        *server().config.lock() = None;
    }

    #[serial_test::serial]
    #[test]
    fn server_dispose_waits_for_running_capture_cleanup() {
        server().setup(FfiConfig {
            callback_fn: Arc::new(|_| {}),
            capture_logs: false,
            sdk: "livekit-ffi-test".to_owned(),
            sdk_version: "test".to_owned(),
        });

        let (entered_tx, entered_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let pump = server().async_runtime.block_on(async {
            let source: Box<dyn PixelVideoSource> =
                Box::new(DropObservedSource { entered: Some(entered_tx), dropped: dropped_tx });
            PixelVideoPump::new(source)
        });
        let capture_handle = server().next_id();
        server().store_handle(
            capture_handle,
            FfiCaptureSource {
                handle_id: capture_handle,
                stop: pump.stop_handle(),
                state: Mutex::new(CaptureState::Idle(CapturePump::Pixel(pump))),
            },
        );

        let response =
            on_start_capture(server(), proto::StartCaptureRequest { capture_handle }).unwrap();
        assert_eq!(response.error, None);
        entered_rx.recv_timeout(Duration::from_secs(5)).expect("capture pump did not start");
        on_join_capture(
            server(),
            proto::JoinCaptureRequest { capture_handle, request_async_id: None },
        )
        .unwrap();

        server().async_runtime.block_on(server().dispose());
        dropped_rx
            .try_recv()
            .expect("FFI server disposal returned before the capture source was cleaned up");
    }

    #[cfg(feature = "capture-pattern")]
    #[serial_test::serial]
    #[test]
    fn pattern_capture_lifecycle() {
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        server().setup(FfiConfig {
            callback_fn: Arc::new(move |event| {
                let _ = event_tx.send(event);
            }),
            capture_logs: false,
            sdk: "livekit-ffi-test".to_owned(),
            sdk_version: "test".to_owned(),
        });

        let request = proto::NewCaptureSourceRequest {
            config: Some(proto::new_capture_source_request::Config::Pattern(
                proto::PatternVideoSourceConfig {
                    resolution: proto::VideoSourceResolution { width: 1280, height: 720 },
                    framerate_fps: 30,
                    pattern: proto::Pattern::Gradient.into(),
                },
            )),
            request_async_id: None,
        };
        let source = server()
            .async_runtime
            .block_on(create_capture_source(server(), request))
            .expect("pattern capture source should build");
        assert_eq!(source.info.kind(), proto::CaptureSourceKind::CaptureSourcePixel);
        assert_eq!(source.info.resolution.width, 1280);
        let capture_handle = source.handle.id;

        // Stopping before starting is allowed; the pump then exits
        // immediately once started, and join reports its result.
        let response =
            on_stop_capture(server(), proto::StopCaptureRequest { capture_handle }).unwrap();
        assert_eq!(response.error, None);

        let response =
            on_start_capture(server(), proto::StartCaptureRequest { capture_handle }).unwrap();
        assert_eq!(response.error, None);

        let response =
            on_start_capture(server(), proto::StartCaptureRequest { capture_handle }).unwrap();
        assert!(response.error.is_some(), "double start must be rejected");

        let response = on_join_capture(
            server(),
            proto::JoinCaptureRequest { capture_handle, request_async_id: Some(42) },
        )
        .unwrap();
        assert_eq!(response.async_id, 42);

        let event =
            event_rx.recv_timeout(Duration::from_secs(5)).expect("capture join did not complete");
        let callback = match event.message {
            Some(proto::ffi_event::Message::JoinCapture(callback)) => callback,
            other => panic!("expected join capture callback, got {other:?}"),
        };
        assert_eq!(callback.async_id, 42);
        let finished = match callback.message {
            Some(proto::join_capture_callback::Message::Finished(finished)) => finished,
            other => panic!("expected finished capture result, got {other:?}"),
        };
        assert_eq!(finished.exit(), proto::CaptureExit::Stopped);

        server().drop_handle(capture_handle);
        server().drop_handle(source.info.video_source.handle.id);
        *server().config.lock() = None;
    }
}
