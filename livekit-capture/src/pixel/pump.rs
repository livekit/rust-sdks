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

//! Pumps pixel frames from a capture source into an RTC video source.

use crate::{
    pixel::PixelVideoSource,
    pump::{spawn_pump, PumpError, PumpExit, PumpStats, PumpStop, RunningPump},
};
use livekit::{
    options::TrackPublishOptions,
    webrtc::{
        video_frame::{BoxVideoFrame, FrameMetadata},
        video_source::{native::NativeVideoSource, RtcVideoSource},
    },
};
use std::{fmt, io};

/// Callback that supplies packet-trailer metadata for a pixel frame.
type FrameMetadataFn = Box<dyn FnMut(&BoxVideoFrame) -> Option<FrameMetadata> + Send>;

/// Pumps a [`PixelVideoSource`] into an RTC video source and publishes its
/// frames through the WebRTC encoder.
pub struct PixelVideoPump<S: PixelVideoSource> {
    source: S,
    rtc_source: NativeVideoSource,
    stop: PumpStop,
    frame_metadata: Option<FrameMetadataFn>,
}

impl<S: PixelVideoSource> PixelVideoPump<S> {
    /// Creates a pump for a pixel source and builds the matching RTC source.
    pub fn new(source: S) -> Self {
        let rtc_source = NativeVideoSource::new(source.resolution().into(), false);
        Self { source, rtc_source, stop: PumpStop::new(), frame_metadata: None }
    }

    /// Sets a callback that supplies packet-trailer metadata for each frame.
    ///
    /// When the callback returns `Some`, it overrides metadata the source
    /// pre-filled on the frame. Subscribers receive metadata only when the
    /// matching [`TrackPublishOptions::frame_metadata_features`] are enabled
    /// on the published track.
    pub fn with_frame_metadata(
        mut self,
        frame_metadata: impl FnMut(&BoxVideoFrame) -> Option<FrameMetadata> + Send + 'static,
    ) -> Self {
        self.frame_metadata = Some(Box::new(frame_metadata));
        self
    }

    /// Returns the RTC source to create the local track with.
    pub fn rtc_source(&self) -> RtcVideoSource {
        RtcVideoSource::Native(self.rtc_source.clone())
    }

    /// Returns publish options for a pixel source.
    pub fn publish_options(&self) -> TrackPublishOptions {
        TrackPublishOptions::default()
    }

    /// Returns a cancellation handle for this pump.
    pub fn stop_handle(&self) -> PumpStop {
        self.stop.clone()
    }

    /// Returns the underlying capture source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Runs the pump on the calling thread until the source ends, an error
    /// occurs, or the stop handle fires.
    ///
    /// Sources block. On an async runtime, run this on a dedicated thread
    /// (see [`PixelVideoPump::spawn`]) or a blocking pool.
    pub fn run(mut self) -> Result<PumpStats, PumpError> {
        let mut frames_captured = 0;
        let exit = loop {
            if self.stop.is_stopped() {
                break PumpExit::Stopped;
            }
            let Some(mut frame) = self.source.next_frame(&self.stop)? else {
                // `None` is end of stream, unless the source returned early
                // because the stop handle fired mid-wait.
                break if self.stop.is_stopped() {
                    PumpExit::Stopped
                } else {
                    PumpExit::EndOfStream
                };
            };
            if let Some(metadata) =
                self.frame_metadata.as_mut().and_then(|callback| callback(&frame))
            {
                frame.frame_metadata = Some(metadata);
            }
            self.rtc_source.capture_frame(&frame);
            frames_captured += 1;
        };
        Ok(PumpStats { frames_captured, exit })
    }

    /// Runs the pump on a dedicated thread.
    ///
    /// A panic on the pump thread is reported as [`PumpError::Panicked`]
    /// when the pump is joined.
    pub fn spawn(self) -> io::Result<RunningPump>
    where
        S: 'static,
    {
        let stop = self.stop_handle();
        spawn_pump(stop, move || self.run())
    }
}

impl<S: PixelVideoSource> fmt::Debug for PixelVideoPump<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixelVideoPump")
            .field("rtc_source", &self.rtc_source)
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};

    use super::*;
    use crate::{error::SourceError, primitive::VideoResolution};

    const RESOLUTION: VideoResolution = VideoResolution { width: 64, height: 36 };

    /// Pixel RTC sources spawn their keepalive task at construction; give the
    /// tests the runtime context an SDK application would have.
    fn runtime_context() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("failed to build test runtime")
    }

    fn pixel_frame(timestamp_us: i64) -> BoxVideoFrame {
        VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            frame_metadata: None,
            buffer: Box::new(I420Buffer::new(RESOLUTION.width, RESOLUTION.height)),
        }
    }

    struct FakePixelSource {
        frames: VecDeque<BoxVideoFrame>,
    }

    impl FakePixelSource {
        fn new(frames: impl IntoIterator<Item = BoxVideoFrame>) -> Self {
            Self { frames: frames.into_iter().collect() }
        }
    }

    impl PixelVideoSource for FakePixelSource {
        fn resolution(&self) -> VideoResolution {
            RESOLUTION
        }

        fn next_frame(&mut self, _stop: &PumpStop) -> Result<Option<BoxVideoFrame>, SourceError> {
            Ok(self.frames.pop_front())
        }
    }

    #[test]
    fn pixel_pump_captures_all_frames_until_eof() {
        let runtime = runtime_context();
        let _guard = runtime.enter();

        let source = FakePixelSource::new([pixel_frame(1), pixel_frame(2), pixel_frame(3)]);
        let stats = PixelVideoPump::new(source).run().unwrap();
        assert_eq!(stats.frames_captured, 3);
        assert_eq!(stats.exit, PumpExit::EndOfStream);
    }

    #[test]
    fn boxed_source_drives_generic_pump() {
        let runtime = runtime_context();
        let _guard = runtime.enter();

        // The dynamic-instantiation pattern: box at the edge, same pump.
        let source: Box<dyn PixelVideoSource> =
            Box::new(FakePixelSource::new([pixel_frame(1), pixel_frame(2)]));
        let stats = PixelVideoPump::new(source).run().unwrap();
        assert_eq!(stats.frames_captured, 2);
    }

    #[test]
    fn metadata_callback_runs_per_frame() {
        let runtime = runtime_context();
        let _guard = runtime.enter();

        let source = FakePixelSource::new([pixel_frame(1), pixel_frame(2), pixel_frame(3)]);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let calls_in_callback = calls.clone();
        let stats = PixelVideoPump::new(source)
            .with_frame_metadata(move |frame| {
                assert!(frame.timestamp_us > 0);
                calls_in_callback.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                None
            })
            .run()
            .unwrap();
        assert_eq!(stats.frames_captured, 3);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 3);
    }

    #[test]
    fn pump_panics_become_errors() {
        struct PanickingSource;

        impl PixelVideoSource for PanickingSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(
                &mut self,
                _stop: &PumpStop,
            ) -> Result<Option<BoxVideoFrame>, SourceError> {
                panic!("source exploded");
            }
        }

        let runtime = runtime_context();
        let _guard = runtime.enter();

        let running = PixelVideoPump::new(PanickingSource).spawn().unwrap();
        let error = running.join().unwrap_err();
        assert!(
            matches!(&error, PumpError::Panicked(message) if message.contains("source exploded"))
        );
    }

    #[test]
    fn running_pump_stops_on_signal() {
        struct EndlessSource;

        impl PixelVideoSource for EndlessSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(
                &mut self,
                _stop: &PumpStop,
            ) -> Result<Option<BoxVideoFrame>, SourceError> {
                std::thread::sleep(std::time::Duration::from_millis(1));
                Ok(Some(pixel_frame(0)))
            }
        }

        let runtime = runtime_context();
        let _guard = runtime.enter();

        let running = PixelVideoPump::new(EndlessSource).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let stats = running.stop_and_join().unwrap();
        assert!(stats.frames_captured > 0);
        assert_eq!(stats.exit, PumpExit::Stopped);
    }

    #[tokio::test]
    async fn pump_stops_and_joins_async() {
        struct EndlessSource;

        impl PixelVideoSource for EndlessSource {
            fn resolution(&self) -> VideoResolution {
                RESOLUTION
            }

            fn next_frame(
                &mut self,
                _stop: &PumpStop,
            ) -> Result<Option<BoxVideoFrame>, SourceError> {
                std::thread::sleep(std::time::Duration::from_millis(1));
                Ok(Some(pixel_frame(0)))
            }
        }

        let running = PixelVideoPump::new(EndlessSource).spawn().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let stats = running.stop_and_join_async().await.unwrap();
        assert!(stats.frames_captured > 0);
    }
}
