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

use cxx::{SharedPtr, UniquePtr};
use livekit_runtime::Stream;
use tokio::sync::mpsc;
use webrtc_sys::video_track as sys_vt;

use super::video_frame::new_video_frame_buffer;
use crate::{
    native::packet_trailer::PacketTrailerHandler,
    video_frame::{BoxVideoFrame, VideoFrame},
    video_track::RtcVideoTrack,
};

pub struct NativeVideoStream {
    native_sink: SharedPtr<sys_vt::ffi::NativeVideoSink>,
    observer: Arc<VideoTrackObserver>,
    video_track: RtcVideoTrack,
    frame_rx: mpsc::UnboundedReceiver<BoxVideoFrame>,
}

impl NativeVideoStream {
    pub fn new(video_track: RtcVideoTrack) -> Self {
        let (frame_tx, frame_rx) = mpsc::unbounded_channel();

        // Auto-wire the packet trailer handler from the track if one is set.
        let handler = video_track.handle.packet_trailer_handler();

        let observer = Arc::new(VideoTrackObserver {
            frame_tx,
            packet_trailer_handler: parking_lot::Mutex::new(handler),
        });
        let native_sink = sys_vt::ffi::new_native_video_sink(Box::new(
            sys_vt::VideoSinkWrapper::new(observer.clone()),
        ));

        let video = unsafe { sys_vt::ffi::media_to_video(video_track.sys_handle()) };
        video.add_sink(&native_sink);

        Self { native_sink, observer, video_track, frame_rx }
    }

    /// Set the packet trailer handler for this stream.
    ///
    /// When set, each frame produced by this stream will have its
    /// `user_timestamp_us` field populated from the handler's receive
    /// map (looked up by RTP timestamp).
    ///
    /// Note: If the handler was already set on the `RtcVideoTrack` before
    /// creating this stream, it is automatically wired up. This method is
    /// only needed if you want to override or set the handler after
    /// construction.
    pub fn set_packet_trailer_handler(&self, handler: PacketTrailerHandler) {
        *self.observer.packet_trailer_handler.lock() = Some(handler);
    }

    pub fn track(&self) -> RtcVideoTrack {
        self.video_track.clone()
    }

    pub fn close(&mut self) {
        let video = unsafe { sys_vt::ffi::media_to_video(self.video_track.sys_handle()) };
        video.remove_sink(&self.native_sink);

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

struct VideoTrackObserver {
    frame_tx: mpsc::UnboundedSender<BoxVideoFrame>,
    packet_trailer_handler: parking_lot::Mutex<Option<PacketTrailerHandler>>,
}

impl sys_vt::VideoSink for VideoTrackObserver {
    fn on_frame(&self, frame: UniquePtr<webrtc_sys::video_frame::ffi::VideoFrame>) {
        let rtp_timestamp = frame.timestamp();
        let meta = self
            .packet_trailer_handler
            .lock()
            .as_ref()
            .and_then(|h| h.lookup_frame_metadata(rtp_timestamp));

        let (user_timestamp_us, frame_id) = match meta {
            Some((ts, fid)) => (Some(ts), Some(fid)),
            None => (None, None),
        };

        let _ = self.frame_tx.send(VideoFrame {
            rotation: frame.rotation().into(),
            timestamp_us: frame.timestamp_us(),
            user_timestamp_us,
            frame_id,
            buffer: new_video_frame_buffer(unsafe { frame.video_frame_buffer() }),
        });
    }

    fn on_discarded_frame(&self) {}

    fn on_constraints_changed(&self, _constraints: sys_vt::ffi::VideoTrackSourceConstraints) {}
}
