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

use super::{FfiHandle, FfiServer};
use crate::{proto, FfiHandleId, FfiResult};

/// A capture pump of either kind, boxed at the FFI edge.
#[allow(dead_code)]
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
#[allow(dead_code)]
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
