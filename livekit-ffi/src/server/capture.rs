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

use livekit_capture::{
    encoded::{EncodedVideoPump, EncodedVideoSource},
    pixel::{PixelVideoPump, PixelVideoSource},
    pump::{PumpError, PumpExit, PumpStats, PumpStop, RunningPump},
};
use parking_lot::Mutex;

#[cfg(feature = "capture-clock")]
use livekit_capture::sources::clock::ClockVideoSource;
#[cfg(feature = "capture-gstreamer")]
use livekit_capture::sources::gstreamer::GStreamerVideoSource;
#[cfg(feature = "capture-pattern")]
use livekit_capture::sources::pattern::PatternVideoSource;

use super::{video_source::FfiVideoSource, FfiHandle, FfiServer};
#[cfg(feature = "capture-gstreamer")]
use crate::conversion::capture::gstreamer_config_from_proto;
#[cfg(feature = "capture-pattern")]
use crate::conversion::capture::pattern_config_from_proto;
use crate::{conversion::capture::video_codec_to_proto, proto, FfiError, FfiHandleId, FfiResult};

/// A capture pump of either kind, boxed at the FFI edge.
enum CapturePump {
    Pixel(PixelVideoPump<Box<dyn PixelVideoSource>>),
    Encoded(EncodedVideoPump<Box<dyn EncodedVideoSource>>),
}

impl CapturePump {
    fn spawn(self) -> std::io::Result<RunningPump> {
        match self {
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
    /// Started; the watcher task owns the running pump.
    Running,
    /// The capture ended; the terminal event has been dispatched.
    Finished,
}

pub struct FfiCaptureSource {
    pub handle_id: FfiHandleId,
    /// Cancellation handle, usable in every state.
    stop: PumpStop,
    state: Mutex<CaptureState>,
}

impl FfiHandle for FfiCaptureSource {}

impl Drop for FfiCaptureSource {
    fn drop(&mut self) {
        // Disposing a running capture stops it; the pump thread observes the
        // signal within one bounded wait and drops the source (stopping the
        // producer). The watcher task delivers the terminal event, which may
        // trail the disposal.
        self.stop.stop();
    }
}

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

    let (kind, resolution, codec, publish_options, rtc_source, stop) = match &pump {
        CapturePump::Pixel(pump) => (
            proto::CaptureSourceKind::CaptureSourcePixel,
            pump.source().resolution(),
            None,
            pump.publish_options(),
            pump.rtc_source(),
            pump.stop_handle(),
        ),
        CapturePump::Encoded(pump) => (
            proto::CaptureSourceKind::CaptureSourceEncoded,
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
    let video_source = FfiVideoSource {
        handle_id: source_handle_id,
        source_type: proto::VideoSourceType::VideoSourceNative,
        source: rtc_source,
    };
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

    Ok(proto::OwnedCaptureSource { handle: proto::FfiOwnedHandle { id: capture_handle_id }, info })
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
    let pump = match std::mem::replace(&mut *state, CaptureState::Running) {
        CaptureState::Idle(pump) => pump,
        other => {
            let error = match &other {
                CaptureState::Running => "capture is already started",
                _ => "capture has already finished",
            };
            *state = other;
            return Ok(proto::StartCaptureResponse { error: Some(error.to_owned()) });
        }
    };

    let running = match pump.spawn() {
        Ok(running) => running,
        Err(err) => {
            *state = CaptureState::Finished;
            return Ok(proto::StartCaptureResponse {
                error: Some(format!("failed to start capture: {err}")),
            });
        }
    };
    drop(state);
    drop(ffi_capture);

    // The watcher owns the running pump and delivers the terminal event
    // exactly once, whether the capture is stopped, ends, or fails.
    server.async_runtime.spawn(async move {
        let result = running.join_async().await;
        if let Ok(ffi_capture) = server.retrieve_handle::<FfiCaptureSource>(capture_handle) {
            *ffi_capture.state.lock() = CaptureState::Finished;
        }
        let _ = server.send_event(proto::ffi_event::Message::CaptureSourceEvent(
            proto::CaptureSourceEvent {
                capture_handle,
                message: Some(capture_result_to_proto(result)),
            },
        ));
    });

    Ok(proto::StartCaptureResponse { error: None })
}

fn capture_result_to_proto(
    result: Result<PumpStats, PumpError>,
) -> proto::capture_source_event::Message {
    match result {
        Ok(stats) => {
            let exit = match stats.exit {
                PumpExit::Stopped => proto::CaptureExit::Stopped,
                PumpExit::EndOfStream => proto::CaptureExit::EndOfStream,
            };
            proto::capture_source_event::Message::Finished(proto::CaptureFinished {
                frames_captured: stats.frames_captured,
                exit: exit.into(),
            })
        }
        Err(err) => proto::capture_source_event::Message::Error(proto::CaptureError {
            error: err.to_string(),
        }),
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

#[cfg(all(test, feature = "capture-pattern"))]
mod tests {
    use super::*;
    use crate::FFI_SERVER;
    use std::time::Duration;

    fn server() -> &'static FfiServer {
        &FFI_SERVER
    }

    #[test]
    fn pattern_capture_lifecycle() {
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
        // immediately once started, and the watcher marks it finished.
        let response =
            on_stop_capture(server(), proto::StopCaptureRequest { capture_handle }).unwrap();
        assert_eq!(response.error, None);

        let response =
            on_start_capture(server(), proto::StartCaptureRequest { capture_handle }).unwrap();
        assert_eq!(response.error, None);

        let response =
            on_start_capture(server(), proto::StartCaptureRequest { capture_handle }).unwrap();
        assert!(response.error.is_some(), "double start must be rejected");

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            {
                let ffi_capture =
                    server().retrieve_handle::<FfiCaptureSource>(capture_handle).unwrap();
                if matches!(*ffi_capture.state.lock(), CaptureState::Finished) {
                    break;
                }
            }
            assert!(std::time::Instant::now() < deadline, "capture did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }

        server().drop_handle(capture_handle);
        server().drop_handle(source.info.video_source.handle.id);
    }
}
