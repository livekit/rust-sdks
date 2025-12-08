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

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use livekit_runtime::Stream;
use tokio::sync::mpsc;

use crate::{
    video_frame::{BoxVideoFrame, VideoFrame},
    video_source::{native::NativeVideoSink, native::VideoSink, VideoTrackSourceConstraints},
    video_track::RtcVideoTrack,
};

pub struct NativeVideoStream {
    native_sink: Arc<NativeVideoSink>,
    pub video_track: RtcVideoTrack,
    pub frame_rx: mpsc::UnboundedReceiver<BoxVideoFrame>,
}

impl NativeVideoStream {
    pub fn new(video_track: RtcVideoTrack) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();
        let native_sink = Arc::new(NativeVideoSink::new(Arc::new(VideoTrackObserver { frame_tx })));
        video_track.add_sink(native_sink.clone());
        Self { native_sink, video_track, frame_rx }
    }

    pub fn track(&self) -> RtcVideoTrack {
        self.video_track.clone()
    }

    pub fn close(&mut self) {
        self.video_track.remove_sink(self.native_sink.clone());

        self.frame_rx.close();
    }
}

impl Drop for NativeVideoStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeVideoStream {
    type Item = BoxVideoFrame;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_rx.poll_recv(cx)
    }
}

pub struct VideoTrackObserver {
    pub frame_tx: mpsc::UnboundedSender<BoxVideoFrame>,
}

impl VideoSink for VideoTrackObserver {
    fn on_frame(&self, frame: VideoFrame) {
        let _ = self.frame_tx.send(Box::new(VideoFrame {
            rotation: frame.rotation.into(),
            timestamp_us: frame.timestamp_us,
            buffer: frame.buffer,
        }));
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: VideoTrackSourceConstraints) {}
}
