// Copyright 2025 LiveKit, Inc.
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

use crate::imp::video_stream as stream_imp;

// There is no shared sink between native and web platforms.
// Each platform requires different configuration (e.g: WebGlContext, ..)

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::{
        fmt::{Debug, Formatter},
        pin::Pin,
        task::{Context, Poll},
    };

    use super::stream_imp;
    use crate::{
        native::packet_trailer::PacketTrailerHandler, video_frame::BoxVideoFrame,
        video_track::RtcVideoTrack,
    };
    use livekit_runtime::Stream;

    const DEFAULT_QUEUE_SIZE_FRAMES: usize = 1;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum NativeVideoStreamQueuePolicy {
        /// FIFO queue that drops the oldest queued frame when full.
        Fifo { capacity: usize },
        /// Keep only the newest decoded frame. Any unconsumed older frame is replaced.
        LatestOnly,
        /// Keep all decoded frames. This can add unbounded latency if the consumer falls behind.
        Unbounded,
    }

    #[derive(Clone, Debug, Default)]
    pub struct NativeVideoStreamOptions {
        /// Maximum number of queued WebRTC sink frames after the video callback.
        ///
        /// `None` uses the default bounded queue size of 1 frame. `Some(0)`
        /// opts into unbounded buffering. Positive values bound the queue, and
        /// the stream drops the oldest queued frames on overflow so render
        /// latency stays bounded.
        ///
        /// If your application consumes both audio and video, keep the queue
        /// sizing strategy coordinated across both streams. Using a much larger
        /// queue, or unbounded buffering, for only one of them can increase
        /// end-to-end latency for that stream and cause audio/video drift.
        pub queue_size_frames: Option<usize>,

        /// Explicit receive queue policy.
        ///
        /// When set, this takes precedence over `queue_size_frames`. Leaving it
        /// as `None` preserves the existing `queue_size_frames` behavior.
        pub queue_policy: Option<NativeVideoStreamQueuePolicy>,
    }

    pub struct NativeVideoStream {
        pub(crate) handle: stream_imp::NativeVideoStream,
    }

    impl Debug for NativeVideoStream {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoStream").field("track", &self.track()).finish()
        }
    }

    impl NativeVideoStream {
        pub fn new(video_track: RtcVideoTrack) -> Self {
            Self {
                handle: stream_imp::NativeVideoStream::new(
                    video_track,
                    NativeVideoStreamQueuePolicy::Fifo { capacity: DEFAULT_QUEUE_SIZE_FRAMES },
                ),
            }
        }

        pub fn latest(video_track: RtcVideoTrack) -> Self {
            Self {
                handle: stream_imp::NativeVideoStream::new(
                    video_track,
                    NativeVideoStreamQueuePolicy::LatestOnly,
                ),
            }
        }

        pub fn with_options(video_track: RtcVideoTrack, options: NativeVideoStreamOptions) -> Self {
            Self {
                handle: stream_imp::NativeVideoStream::new(
                    video_track,
                    normalize_queue_policy(options),
                ),
            }
        }

        /// Set the packet trailer handler for this stream.
        ///
        /// When set, each frame produced by this stream will have its
        /// `user_timestamp` field populated by looking up the user
        /// timestamp for each frame's RTP timestamp.
        ///
        /// Note: If the handler was already set on the `RtcVideoTrack`
        /// before creating this stream, it is automatically wired up.
        /// This method is only needed to override or set the handler
        /// after construction.
        pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
            self.handle.set_packet_trailer_handler(handler);
        }

        pub fn track(&self) -> RtcVideoTrack {
            self.handle.track()
        }

        pub fn close(&mut self) {
            self.handle.close();
        }

        pub fn dropped_frames(&self) -> u64 {
            self.handle.dropped_frames()
        }
    }

    impl Stream for NativeVideoStream {
        type Item = BoxVideoFrame;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
            Pin::new(&mut self.get_mut().handle).poll_next(cx)
        }
    }

    fn normalize_queue_policy(options: NativeVideoStreamOptions) -> NativeVideoStreamQueuePolicy {
        if let Some(policy) = options.queue_policy {
            return policy;
        }

        match options.queue_size_frames {
            None => NativeVideoStreamQueuePolicy::Fifo { capacity: DEFAULT_QUEUE_SIZE_FRAMES },
            Some(0) => NativeVideoStreamQueuePolicy::Unbounded,
            Some(value) => NativeVideoStreamQueuePolicy::Fifo { capacity: value },
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
