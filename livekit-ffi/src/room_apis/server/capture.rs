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
                })
            }
            Err(err) => {
                let kind = match err {
                    PumpError::Source(_) => proto::CaptureErrorKind::Source,
                    PumpError::CaptureFailed => proto::CaptureErrorKind::Rtc,
                    PumpError::Panicked(_) => proto::CaptureErrorKind::Panicked,
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
        let running = server.retrieve_handle::<FfiCaptureSource>(capture_handle).ok().and_then(
            |ffi_capture| match std::mem::replace(
                &mut *ffi_capture.state.lock(),
                CaptureState::Finished,
            ) {
                CaptureState::Running(running) => Some(running),
                _ => None,
            },
        );
        let message = match running {
            Some(running) => running.join().await.into(),
            None => proto::join_capture_callback::Message::Error(proto::CaptureError {
                error: "capture was already joined or disposed".into(),
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
    use livekit_capture::{error::SourceError, primitive::VideoResolution};
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
