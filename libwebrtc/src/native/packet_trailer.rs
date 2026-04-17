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

use cxx::SharedPtr;
use webrtc_sys::packet_trailer::ffi as sys_pt;

use crate::{
    peer_connection_factory::PeerConnectionFactory, rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

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

    /// Queue frame metadata for ordered send-side propagation.
    pub fn enqueue_frame_metadata(&self, user_timestamp: u64, frame_id: u32) {
        self.sys_handle.enqueue_frame_metadata(user_timestamp, frame_id);
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

    #[cfg(test)]
    pub(crate) fn null_for_test() -> Self {
        Self { sys_handle: SharedPtr::null() }
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
