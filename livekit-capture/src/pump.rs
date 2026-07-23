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

//! Pumps frames from a capture source into an RTC video source.
//!
//! [`VideoPump`] is the bridge between the libwebrtc-free source traits in
//! [`source`](crate::source) and a publishable RTC track: it builds the
//! matching [`NativeVideoSource`], converts crate-owned frame types at the
//! boundary, and forwards downstream keyframe and rate-control requests back
//! to encoded sources.

use std::{
    any::Any,
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use livekit::{
    options::{TrackPublishOptions, VideoEncoderBackend},
    webrtc::{
        video_frame::{EncodedVideoFrame, I420Buffer, VideoFrame, VideoRotation},
        video_source::{
            native::NativeVideoSource, EncodedRateControl, RtcVideoSource,
            VideoResolution as RtcVideoResolution,
        },
    },
};
use thiserror::Error;

use crate::{
    encoded::{CodecSpecific, EncodedFrameType, EncodedLayerInfo, OwnedEncodedAccessUnit},
    error::CaptureError,
    source::{
        EncodedVideoSource, PixelVideoData, PixelVideoFrame, PixelVideoSource, RateControl,
        SourceError, VideoResolution, VideoSource,
    },
};

impl From<EncodedRateControl> for RateControl {
    fn from(target: EncodedRateControl) -> Self {
        Self { target_bitrate_bps: target.target_bitrate_bps, framerate_fps: target.framerate_fps }
    }
}

impl From<VideoResolution> for RtcVideoResolution {
    fn from(resolution: VideoResolution) -> Self {
        Self { width: resolution.width, height: resolution.height }
    }
}

/// Error returned by a pump run.
#[derive(Debug, Error)]
pub enum VideoPumpError {
    /// The capture source failed.
    #[error("capture source failed")]
    Source(#[from] SourceError),
    /// The RTC source rejected a frame.
    #[error("frame capture failed")]
    Capture(#[from] CaptureError),
    /// The pump thread panicked.
    #[error("pump panicked: {0}")]
    Panicked(String),
}

/// Why a pump run ended successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoPumpExit {
    /// The stop handle was fired.
    Stopped,
    /// The source reached the end of its stream.
    EndOfStream,
}

/// Statistics returned when a pump run ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct VideoPumpStats {
    /// Number of frames or access units captured.
    pub frames_captured: u64,
    /// Why the run ended.
    pub exit: VideoPumpExit,
}

/// Cancellation handle for a [`VideoPump`].
///
/// Cheap to clone; wire it to a shutdown signal and call
/// [`VideoPumpStop::stop`] from any thread to make the pump return after the
/// frame in flight.
#[derive(Debug, Clone, Default)]
pub struct VideoPumpStop(Arc<AtomicBool>);

impl VideoPumpStop {
    /// Creates an un-stopped handle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals the pump to stop.
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns true once [`VideoPumpStop::stop`] has been called.
    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Pumps a [`VideoSource`] into an RTC video source.
///
/// The pump owns every libwebrtc interaction: it builds the RTC source
/// appropriate for the capture source kind, derives the matching publish
/// options, converts frames at the boundary, and polls downstream keyframe
/// and rate-control requests, forwarding them to encoded sources as
/// crate-owned types.
#[derive(Debug)]
pub struct VideoPump {
    source: VideoSource,
    rtc_source: NativeVideoSource,
    stop: VideoPumpStop,
}

impl VideoPump {
    /// Creates a pump for a capture source, building the matching RTC source.
    ///
    /// For pixel sources this must be called from the context of the async
    /// runtime driving the SDK, because the RTC source spawns its keepalive
    /// task at construction. The pump itself runs on plain threads.
    pub fn new(source: impl Into<VideoSource>) -> Self {
        let source = source.into();
        let resolution = source.resolution().into();
        let rtc_source = match &source {
            VideoSource::Pixel(_) => NativeVideoSource::new(resolution, false),
            VideoSource::Encoded(_) => NativeVideoSource::new_encoded(resolution),
        };
        Self { source, rtc_source, stop: VideoPumpStop::new() }
    }

    /// Returns the RTC source to create the local track with.
    pub fn rtc_source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.rtc_source.clone())
    }

    /// Returns publish options appropriate for the source kind.
    pub fn publish_options(&self) -> TrackPublishOptions {
        match &self.source {
            VideoSource::Pixel(_) => TrackPublishOptions::default(),
            VideoSource::Encoded(source) => TrackPublishOptions {
                video_codec: source.codec().into(),
                video_encoder: VideoEncoderBackend::PreEncoded,
                simulcast: false,
                ..Default::default()
            },
        }
    }

    /// Returns a cancellation handle for this pump.
    pub fn stop_handle(&self) -> VideoPumpStop {
        self.stop.clone()
    }

    /// Runs the pump on the calling thread until the source ends, a failure,
    /// or the stop handle fires.
    ///
    /// Sources block, so callers on an async runtime should run this on a
    /// dedicated thread (see [`VideoPump::spawn`]) or a blocking pool.
    pub fn run(self) -> Result<VideoPumpStats, VideoPumpError> {
        match self.source {
            VideoSource::Pixel(source) => run_pixel(source, &self.rtc_source, &self.stop),
            VideoSource::Encoded(source) => run_encoded(source, &self.rtc_source, &self.stop),
        }
    }

    /// Runs the pump on a dedicated thread.
    ///
    /// Panics on the pump thread are caught and reported as
    /// [`VideoPumpError::Panicked`] when the pump is joined.
    pub fn spawn(self) -> io::Result<RunningVideoPump> {
        let stop = self.stop_handle();
        let (finished_tx, finished_rx) = tokio::sync::watch::channel(false);

        let thread = thread::Builder::new().name("lk-video-pump".to_owned()).spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| self.run()))
                .unwrap_or_else(|panic| Err(VideoPumpError::Panicked(panic_message(&*panic))));
            let _ = finished_tx.send(true);
            result
        })?;

        Ok(RunningVideoPump { stop, thread, finished: finished_rx })
    }
}

/// Renders a panic payload for [`VideoPumpError::Panicked`].
fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "opaque panic payload".to_owned()
    }
}

/// A [`VideoPump`] running on a dedicated thread.
///
/// Stopping takes effect between frames: a source blocked waiting for its
/// next frame finishes that wait before the pump observes the signal.
#[derive(Debug)]
pub struct RunningVideoPump {
    stop: VideoPumpStop,
    thread: thread::JoinHandle<Result<VideoPumpStats, VideoPumpError>>,
    /// Flipped to true by the pump thread just before it exits.
    finished: tokio::sync::watch::Receiver<bool>,
}

impl RunningVideoPump {
    /// Returns a cancellation handle for the pump.
    pub fn stop_handle(&self) -> VideoPumpStop {
        self.stop.clone()
    }

    /// Signals the pump to stop after the frame in flight.
    pub fn stop(&self) {
        self.stop.stop();
    }

    /// Returns true once the pump thread has exited.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Waits for the pump thread to exit.
    ///
    /// Panics on the pump thread are reported as
    /// [`VideoPumpError::Panicked`].
    pub fn join(self) -> Result<VideoPumpStats, VideoPumpError> {
        self.thread
            .join()
            .unwrap_or_else(|panic| Err(VideoPumpError::Panicked(panic_message(&*panic))))
    }

    /// Signals the pump to stop and waits for its thread to exit.
    pub fn stop_and_join(self) -> Result<VideoPumpStats, VideoPumpError> {
        self.stop();
        self.join()
    }

    /// Waits for the pump thread to exit without blocking the async runtime.
    ///
    /// This awaits a completion signal rather than parking a thread, so it is
    /// safe to hold across long stretches — for example in a `select!` that
    /// supervises every running pump — and works under any async runtime,
    /// not just tokio. Panics on the pump thread are reported as
    /// [`VideoPumpError::Panicked`].
    pub async fn join_async(mut self) -> Result<VideoPumpStats, VideoPumpError> {
        // An error means the sender dropped, which also implies the pump
        // thread is done; either way the join below returns promptly.
        let _ = self.finished.wait_for(|finished| *finished).await;
        self.join()
    }

    /// Signals the pump to stop and waits for its thread to exit without
    /// blocking the async runtime.
    pub async fn stop_and_join_async(self) -> Result<VideoPumpStats, VideoPumpError> {
        self.stop();
        self.join_async().await
    }
}

fn run_pixel(
    mut source: Box<dyn PixelVideoSource>,
    rtc_source: &NativeVideoSource,
    stop: &VideoPumpStop,
) -> Result<VideoPumpStats, VideoPumpError> {
    let mut frames_captured = 0;
    let exit = loop {
        if stop.is_stopped() {
            break VideoPumpExit::Stopped;
        }
        let Some(frame) = source.next_frame()? else {
            break VideoPumpExit::EndOfStream;
        };
        capture_pixel_frame(rtc_source, &frame)?;
        frames_captured += 1;
    };
    Ok(VideoPumpStats { frames_captured, exit })
}

fn run_encoded(
    mut source: Box<dyn EncodedVideoSource>,
    rtc_source: &NativeVideoSource,
    stop: &VideoPumpStop,
) -> Result<VideoPumpStats, VideoPumpError> {
    let mut frames_captured = 0;
    let mut awaiting_initial_keyframe = true;
    let exit = loop {
        if stop.is_stopped() {
            break VideoPumpExit::Stopped;
        }
        if let Some(target) = rtc_source.take_rate_control_request() {
            source.update_rate_control(target.into());
        }
        if rtc_source.take_keyframe_request() {
            source.request_keyframe();
        }

        let Some(access_unit) = source.next_access_unit()? else {
            break VideoPumpExit::EndOfStream;
        };

        // Drop pre-roll deltas: decoding can only start at a keyframe.
        if awaiting_initial_keyframe && access_unit.frame_type != EncodedFrameType::Key {
            continue;
        }
        awaiting_initial_keyframe = false;

        capture_access_unit(rtc_source, &access_unit)?;
        frames_captured += 1;
    };
    Ok(VideoPumpStats { frames_captured, exit })
}

fn capture_pixel_frame(
    rtc_source: &NativeVideoSource,
    frame: &PixelVideoFrame,
) -> Result<(), CaptureError> {
    let buffer = i420_buffer(frame)?;
    rtc_source.capture_frame(&VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: frame.timestamp_us,
        frame_metadata: None,
        buffer,
    });
    Ok(())
}

fn i420_buffer(frame: &PixelVideoFrame) -> Result<I420Buffer, CaptureError> {
    let PixelVideoData::I420 { y, u, v, stride_y, stride_u, stride_v } = &frame.data;

    let mut buffer = I420Buffer::new(frame.width, frame.height);
    let chroma_width = frame.width.div_ceil(2);
    let chroma_height = frame.height.div_ceil(2);
    let (dst_stride_y, dst_stride_u, dst_stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();

    copy_plane(y, *stride_y, dst_y, dst_stride_y, frame.width, frame.height)?;
    copy_plane(u, *stride_u, dst_u, dst_stride_u, chroma_width, chroma_height)?;
    copy_plane(v, *stride_v, dst_v, dst_stride_v, chroma_width, chroma_height)?;
    Ok(buffer)
}

fn copy_plane(
    src: &[u8],
    src_stride: u32,
    dst: &mut [u8],
    dst_stride: u32,
    width: u32,
    height: u32,
) -> Result<(), CaptureError> {
    let (width, height) = (width as usize, height as usize);
    let (src_stride, dst_stride) = (src_stride as usize, dst_stride as usize);
    if src_stride < width {
        return Err(CaptureError::InvalidPixelFrame("plane stride is smaller than its width"));
    }
    // The final row may be unpadded.
    let min_len = (height - 1).saturating_mul(src_stride) + width;
    if src.len() < min_len {
        return Err(CaptureError::InvalidPixelFrame("plane data is shorter than its dimensions"));
    }

    for row in 0..height {
        let src_row = &src[row * src_stride..][..width];
        dst[row * dst_stride..][..width].copy_from_slice(src_row);
    }
    Ok(())
}

fn capture_access_unit(
    rtc_source: &NativeVideoSource,
    access_unit: &OwnedEncodedAccessUnit,
) -> Result<(), CaptureError> {
    validate_access_unit(access_unit)?;

    let frame = EncodedVideoFrame {
        codec: access_unit.codec.into(),
        payload: &access_unit.payload,
        timestamp_us: access_unit.timestamp_us,
        frame_type: access_unit.frame_type.into(),
        resolution: RtcVideoResolution { width: access_unit.width, height: access_unit.height },
        frame_metadata: None,
    };
    rtc_source.capture_encoded_frame(&frame).then_some(()).ok_or(CaptureError::CaptureFailed)
}

/// The passthrough path forwards single-layer streams: access units carrying
/// temporal/spatial layer ids or layering metadata are rejected so callers
/// are not misled into thinking that metadata reaches the wire.
fn validate_access_unit(access_unit: &OwnedEncodedAccessUnit) -> Result<(), CaptureError> {
    if access_unit.payload.is_empty() {
        return Err(CaptureError::EmptyPayload);
    }
    if access_unit.layers != EncodedLayerInfo::default() {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "temporal/spatial layer ids are not forwarded by the passthrough encoder",
        ));
    }
    if access_unit.codec_specific != CodecSpecific::None
        && access_unit.codec_specific != CodecSpecific::default_for(access_unit.codec)
    {
        return Err(CaptureError::UnsupportedLayeredEncoding(
            "codec-specific layering metadata is not forwarded by the passthrough encoder",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use bytes::Bytes;

    use super::*;
    use crate::encoded::EncodedVideoCodec;

    const RESOLUTION: VideoResolution = VideoResolution { width: 64, height: 36 };

    /// Pixel RTC sources spawn their keepalive task at construction; give the
    /// tests the runtime context an SDK application would have.
    fn runtime_context() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("failed to build test runtime")
    }

    fn pixel_frame(timestamp_us: i64) -> PixelVideoFrame {
        let chroma_width = RESOLUTION.width.div_ceil(2);
        let chroma_height = RESOLUTION.height.div_ceil(2);
        PixelVideoFrame {
            width: RESOLUTION.width,
            height: RESOLUTION.height,
            timestamp_us,
            data: PixelVideoData::I420 {
                y: Bytes::from(vec![128; (RESOLUTION.width * RESOLUTION.height) as usize]),
                u: Bytes::from(vec![128; (chroma_width * chroma_height) as usize]),
                v: Bytes::from(vec![128; (chroma_width * chroma_height) as usize]),
                stride_y: RESOLUTION.width,
                stride_u: chroma_width,
                stride_v: chroma_width,
            },
        }
    }

    struct FakePixelSource {
        frames: VecDeque<PixelVideoFrame>,
    }

    impl FakePixelSource {
        fn new(frames: impl IntoIterator<Item = PixelVideoFrame>) -> Self {
            Self { frames: frames.into_iter().collect() }
        }
    }

    impl PixelVideoSource for FakePixelSource {
        fn resolution(&self) -> VideoResolution {
            RESOLUTION
        }

        fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
            Ok(self.frames.pop_front())
        }
    }

    struct FakeEncodedSource {
        access_units: VecDeque<OwnedEncodedAccessUnit>,
    }

    impl FakeEncodedSource {
        fn new(access_units: impl IntoIterator<Item = OwnedEncodedAccessUnit>) -> Self {
            Self { access_units: access_units.into_iter().collect() }
        }
    }

    impl EncodedVideoSource for FakeEncodedSource {
        fn resolution(&self) -> VideoResolution {
            RESOLUTION
        }

        fn codec(&self) -> EncodedVideoCodec {
            EncodedVideoCodec::VP8
        }

        fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
            Ok(self.access_units.pop_front())
        }
    }

    fn access_unit(timestamp_us: i64, frame_type: EncodedFrameType) -> OwnedEncodedAccessUnit {
        OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::VP8,
            vec![1, 2, 3],
            timestamp_us,
            frame_type,
            RESOLUTION.width,
            RESOLUTION.height,
        )
    }

    #[test]
    fn pixel_pump_captures_all_frames_until_eof() {
        let runtime = runtime_context();
        let _guard = runtime.enter();

        let source = FakePixelSource::new([pixel_frame(1), pixel_frame(2), pixel_frame(3)]);
        let stats = VideoPump::new(VideoSource::pixel(source)).run().unwrap();
        assert_eq!(stats.frames_captured, 3);
        assert_eq!(stats.exit, VideoPumpExit::EndOfStream);
    }

    #[test]
    fn pump_panics_become_errors() {
        struct PanickingSource;

        impl PixelVideoSource for PanickingSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
                panic!("source exploded");
            }
        }

        let runtime = runtime_context();
        let _guard = runtime.enter();

        let running = VideoPump::new(VideoSource::pixel(PanickingSource)).spawn().unwrap();
        let error = running.join().unwrap_err();
        assert!(
            matches!(&error, VideoPumpError::Panicked(message) if message.contains("source exploded"))
        );
    }

    #[test]
    fn running_pump_stops_on_signal() {
        struct EndlessSource;

        impl PixelVideoSource for EndlessSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
                std::thread::sleep(std::time::Duration::from_millis(1));
                Ok(Some(pixel_frame(0)))
            }
        }

        let runtime = runtime_context();
        let _guard = runtime.enter();

        let running = VideoPump::new(VideoSource::pixel(EndlessSource)).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let stats = running.stop_and_join().unwrap();
        assert!(stats.frames_captured > 0);
        assert_eq!(stats.exit, VideoPumpExit::Stopped);
    }

    #[test]
    fn pixel_pump_rejects_short_planes() {
        let runtime = runtime_context();
        let _guard = runtime.enter();

        let mut frame = pixel_frame(1);
        let PixelVideoData::I420 { y, .. } = &mut frame.data;
        *y = Bytes::from(vec![128; 8]);

        let result = VideoPump::new(VideoSource::pixel(FakePixelSource::new([frame]))).run();
        assert!(matches!(result, Err(VideoPumpError::Capture(CaptureError::InvalidPixelFrame(_)))));
    }

    #[tokio::test]
    async fn pump_stops_and_joins_async() {
        struct EndlessSource;

        impl PixelVideoSource for EndlessSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(&mut self) -> Result<Option<PixelVideoFrame>, SourceError> {
                std::thread::sleep(std::time::Duration::from_millis(1));
                Ok(Some(pixel_frame(0)))
            }
        }

        let running = VideoPump::new(VideoSource::pixel(EndlessSource)).spawn().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let stats = running.stop_and_join_async().await.unwrap();
        assert!(stats.frames_captured > 0);
    }

    #[test]
    fn encoded_pump_starts_at_initial_keyframe() {
        let source = FakeEncodedSource::new([
            access_unit(1, EncodedFrameType::Delta),
            access_unit(2, EncodedFrameType::Delta),
            access_unit(3, EncodedFrameType::Key),
            access_unit(4, EncodedFrameType::Delta),
        ]);
        let stats = VideoPump::new(VideoSource::encoded(source)).run().unwrap();
        assert_eq!(stats.frames_captured, 2);
    }

    #[test]
    fn encoded_pump_rejects_empty_payloads() {
        let mut unit = access_unit(1, EncodedFrameType::Key);
        unit.payload = Bytes::new();

        let result = VideoPump::new(VideoSource::encoded(FakeEncodedSource::new([unit]))).run();
        assert!(matches!(result, Err(VideoPumpError::Capture(CaptureError::EmptyPayload))));
    }

    #[test]
    fn encoded_publish_options_use_passthrough() {
        let pump = VideoPump::new(VideoSource::encoded(FakeEncodedSource::new([])));
        let options = pump.publish_options();
        assert_eq!(options.video_encoder, VideoEncoderBackend::PreEncoded);
        assert!(!options.simulcast);
    }
}
