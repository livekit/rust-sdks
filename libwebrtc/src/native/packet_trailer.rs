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

//! Packet trailer support for end-to-end frame metadata propagation.
//!
//! This module provides functionality to embed user-supplied metadata
//! in encoded video frames as trailers. The timestamps/frameIDs are preserved
//! through the WebRTC pipeline and can be extracted on the receiver side.
//!
//! On the send side, user timestamps/frameIDs are stored in the handler's internal
//! map keyed by RTP timestamp. When the encoder produces a frame,
//! the transformer looks up the metadata via the frame's CaptureTime().
//!
//! On the receive side, extracted frame metadata is stored in an
//! internal map keyed by RTP timestamp. Decoded frames look up their
//! metadata via lookup_frame_metadata(rtp_timestamp).

use std::sync::Arc;

use cxx::SharedPtr;
use webrtc_sys::packet_trailer::ffi as sys_pt;

use crate::{
    peer_connection_factory::PeerConnectionFactory, rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

/// Stage reached by a native local video frame in the publish pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishTimingStage {
    /// The adapted raw frame was handed to WebRTC's encoder path.
    EncoderUpload,
    /// WebRTC produced an encoded frame for packetization.
    EncoderOutput,
    /// The encoded frame was handed back to WebRTC's packetizer.
    WebrtcPacketize,
}

/// Stage reached by a native remote video frame in the subscribe pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscribeTimingStage {
    /// WebRTC produced an encoded frame after RTP depacketization.
    WebrtcReceive,
    /// The encoded frame was handed to WebRTC's decoder.
    DecoderUpload,
    /// WebRTC produced a decoded frame for the native video sink.
    DecoderOutput,
}

/// Timestamped native local video publish pipeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishTimingEvent {
    /// Publish pipeline stage reached by the frame.
    pub stage: PublishTimingStage,
    /// Wall-clock time when this stage was observed, in microseconds since the Unix epoch.
    pub timestamp_us: u64,
    /// User capture timestamp associated with this frame, in microseconds since the Unix epoch.
    pub capture_timestamp_us: u64,
    /// Optional application frame ID associated with this frame.
    pub frame_id: Option<u32>,
}

/// Timestamped native remote video subscribe pipeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeTimingEvent {
    /// Subscribe pipeline stage reached by the frame.
    pub stage: SubscribeTimingStage,
    /// Wall-clock time when this stage was observed, in microseconds since the Unix epoch.
    pub timestamp_us: u64,
    /// User capture timestamp associated with this frame, in microseconds since the Unix epoch.
    pub capture_timestamp_us: u64,
    /// Optional application frame ID associated with this frame.
    pub frame_id: Option<u32>,
}

/// Callback invoked for native local video publish timing events.
pub type PublishTimingObserver = Arc<dyn Fn(PublishTimingEvent) + Send + Sync + 'static>;
/// Callback invoked for native remote video subscribe timing events.
pub type SubscribeTimingObserver = Arc<dyn Fn(SubscribeTimingEvent) + Send + Sync + 'static>;

impl From<sys_pt::VideoPublishTimingStage> for PublishTimingStage {
    fn from(stage: sys_pt::VideoPublishTimingStage) -> Self {
        match stage {
            sys_pt::VideoPublishTimingStage::EncoderUpload => Self::EncoderUpload,
            sys_pt::VideoPublishTimingStage::EncoderOutput => Self::EncoderOutput,
            sys_pt::VideoPublishTimingStage::WebrtcPacketize => Self::WebrtcPacketize,
            _ => Self::WebrtcPacketize,
        }
    }
}

impl From<sys_pt::VideoPublishTimingEvent> for PublishTimingEvent {
    fn from(event: sys_pt::VideoPublishTimingEvent) -> Self {
        Self {
            stage: event.stage.into(),
            timestamp_us: event.timestamp_us,
            capture_timestamp_us: event.capture_timestamp_us,
            frame_id: (event.frame_id != 0).then_some(event.frame_id),
        }
    }
}

impl From<sys_pt::VideoSubscribeTimingStage> for SubscribeTimingStage {
    fn from(stage: sys_pt::VideoSubscribeTimingStage) -> Self {
        match stage {
            sys_pt::VideoSubscribeTimingStage::WebrtcReceive => Self::WebrtcReceive,
            sys_pt::VideoSubscribeTimingStage::DecoderUpload => Self::DecoderUpload,
            sys_pt::VideoSubscribeTimingStage::DecoderOutput => Self::DecoderOutput,
            _ => Self::DecoderOutput,
        }
    }
}

impl From<sys_pt::VideoSubscribeTimingEvent> for SubscribeTimingEvent {
    fn from(event: sys_pt::VideoSubscribeTimingEvent) -> Self {
        Self {
            stage: event.stage.into(),
            timestamp_us: event.timestamp_us,
            capture_timestamp_us: event.capture_timestamp_us,
            frame_id: (event.frame_id != 0).then_some(event.frame_id),
        }
    }
}

/// Handler for packet trailer embedding/extraction on RTP streams.
///
/// For sender side: Stores frame metadata keyed by capture timestamp
/// and embeds them as binary payload trailers on encoded frames before they
/// are sent. Use `store_frame_metadata()` to associate metadata with
/// a captured frame.
///
/// For receiver side: Extracts frame metadata from received frames
/// and makes them available for retrieval via `lookup_frame_metadata()`.
#[derive(Clone)]
pub struct PacketTrailerHandler {
    sys_handle: SharedPtr<sys_pt::PacketTrailerHandler>,
}

impl PacketTrailerHandler {
    /// Enable or disable timestamp embedding/extraction.
    pub fn set_enabled(&self, enabled: bool) {
        self.sys_handle.set_enabled(enabled);
    }

    /// Check if timestamp embedding/extraction is enabled.
    pub fn enabled(&self) -> bool {
        self.sys_handle.enabled()
    }

    /// Lookup the frame metadata for a given RTP timestamp (receiver side).
    /// Returns `Some((user_timestamp, frame_id))` if found, `None` otherwise.
    /// The entry is removed from the map after a successful lookup.
    pub fn lookup_frame_metadata(&self, rtp_timestamp: u32) -> Option<(u64, u32)> {
        let ts = self.sys_handle.lookup_timestamp(rtp_timestamp);
        if ts != u64::MAX {
            let frame_id = self.sys_handle.last_lookup_frame_id();
            Some((ts, frame_id))
        } else {
            None
        }
    }

    /// Store frame metadata for a given capture timestamp (sender side).
    ///
    /// The `capture_timestamp_us` must be the TimestampAligner-adjusted
    /// timestamp (as produced by `VideoTrackSource::on_captured_frame`),
    /// NOT the original `timestamp_us` from the VideoFrame. The transformer
    /// looks up the metadata by the frame's `CaptureTime()` which is
    /// derived from the aligned value.
    ///
    /// In normal usage this is called automatically by the C++ layer --
    /// callers should set `user_timestamp` and `frame_id` on the
    /// `VideoFrame` and let `capture_frame` / `on_captured_frame` handle
    /// the rest.
    pub fn store_frame_metadata(
        &self,
        capture_timestamp_us: i64,
        user_timestamp: u64,
        frame_id: u32,
    ) {
        self.sys_handle.store_frame_metadata(capture_timestamp_us, user_timestamp, frame_id);
    }

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_pt::PacketTrailerHandler> {
        self.sys_handle.clone()
    }

    /// Set the callback receiving sender-side publish timing events.
    pub fn set_publish_timing_observer(&self, observer: Option<PublishTimingObserver>) {
        if let Some(observer) = observer {
            self.sys_handle.set_publish_timing_observer(Box::new(
                webrtc_sys::packet_trailer::VideoPublishTimingObserverWrapper::new(Box::new(
                    move |event| observer(event.into()),
                )),
            ));
        } else {
            self.sys_handle.clear_publish_timing_observer();
        }
    }

    /// Set the callback receiving receiver-side subscribe timing events.
    pub fn set_subscribe_timing_observer(&self, observer: Option<SubscribeTimingObserver>) {
        if let Some(observer) = observer {
            self.sys_handle.set_subscribe_timing_observer(Box::new(
                webrtc_sys::packet_trailer::VideoSubscribeTimingObserverWrapper::new(Box::new(
                    move |event| observer(event.into()),
                )),
            ));
        } else {
            self.sys_handle.clear_subscribe_timing_observer();
        }
    }

    pub(crate) fn emit_subscribe_timing(
        &self,
        stage: SubscribeTimingStage,
        capture_timestamp_us: u64,
        frame_id: u32,
    ) {
        let stage = match stage {
            SubscribeTimingStage::WebrtcReceive => sys_pt::VideoSubscribeTimingStage::WebrtcReceive,
            SubscribeTimingStage::DecoderUpload => sys_pt::VideoSubscribeTimingStage::DecoderUpload,
            SubscribeTimingStage::DecoderOutput => sys_pt::VideoSubscribeTimingStage::DecoderOutput,
        };
        self.sys_handle.emit_subscribe_timing(stage, capture_timestamp_us, frame_id);
    }
}

/// Create a sender-side packet trailer handler.
///
/// This handler will embed frame metadata into encoded frames before
/// they are packetized and sent. Use `store_frame_metadata()` to
/// associate metadata with a captured frame's capture timestamp.
pub fn create_sender_handler(
    peer_factory: &PeerConnectionFactory,
    sender: &RtpSender,
) -> PacketTrailerHandler {
    PacketTrailerHandler {
        sys_handle: sys_pt::new_packet_trailer_sender(
            peer_factory.handle.sys_handle.clone(),
            sender.handle.sys_handle.clone(),
        ),
    }
}

/// Create a receiver-side packet trailer handler.
///
/// This handler will extract frame metadata from received frames
/// and store them in a map keyed by RTP timestamp. Use
/// `lookup_frame_metadata(rtp_timestamp)` to retrieve the metadata
/// for a specific decoded frame.
pub fn create_receiver_handler(
    peer_factory: &PeerConnectionFactory,
    receiver: &RtpReceiver,
) -> PacketTrailerHandler {
    PacketTrailerHandler {
        sys_handle: sys_pt::new_packet_trailer_receiver(
            peer_factory.handle.sys_handle.clone(),
            receiver.handle.sys_handle.clone(),
        ),
    }
}
