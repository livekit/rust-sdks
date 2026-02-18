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

//! User timestamp support for end-to-end timestamp propagation.
//!
//! This module provides functionality to embed user-supplied timestamps
//! in encoded video frames as trailers. The timestamps are preserved
//! through the WebRTC pipeline and can be extracted on the receiver side.
//!
//! On the send side, user timestamps are stored in the handler's internal
//! map keyed by capture timestamp. When the encoder produces a frame,
//! the transformer looks up the user timestamp via the frame's CaptureTime().
//!
//! On the receive side, extracted user timestamps are stored in an
//! internal map keyed by RTP timestamp. Decoded frames look up their
//! user timestamp via lookup_user_timestamp(rtp_timestamp).

use cxx::SharedPtr;
use webrtc_sys::user_timestamp::ffi as sys_ut;

use crate::{
    peer_connection_factory::PeerConnectionFactory, rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

/// Handler for user timestamp embedding/extraction on RTP streams.
///
/// For sender side: Stores user timestamps keyed by capture timestamp
/// and embeds them as 12-byte trailers on encoded frames before they
/// are sent. Use `store_user_timestamp()` to associate a user timestamp
/// with a captured frame.
///
/// For receiver side: Extracts user timestamps from received frames
/// and makes them available for retrieval via `lookup_user_timestamp()`.
#[derive(Clone)]
pub struct UserTimestampHandler {
    sys_handle: SharedPtr<sys_ut::UserTimestampHandler>,
}

impl UserTimestampHandler {
    /// Enable or disable timestamp embedding/extraction.
    pub fn set_enabled(&self, enabled: bool) {
        self.sys_handle.set_enabled(enabled);
    }

    /// Check if timestamp embedding/extraction is enabled.
    pub fn enabled(&self) -> bool {
        self.sys_handle.enabled()
    }

    /// Lookup the user timestamp for a given RTP timestamp (receiver side).
    /// Returns None if no timestamp was found for this RTP timestamp.
    /// The entry is removed from the map after a successful lookup.
    ///
    /// Use the RTP timestamp from the decoded video frame to correlate
    /// it with the user timestamp that was embedded in the encoded frame.
    pub fn lookup_user_timestamp(&self, rtp_timestamp: u32) -> Option<i64> {
        let ts = self.sys_handle.lookup_user_timestamp(rtp_timestamp);
        if ts >= 0 {
            Some(ts)
        } else {
            None
        }
    }

    /// Store a user timestamp for a given capture timestamp (sender side).
    ///
    /// The `capture_timestamp_us` must be the TimestampAligner-adjusted
    /// timestamp (as produced by `VideoTrackSource::on_captured_frame`),
    /// NOT the original `timestamp_us` from the VideoFrame. The transformer
    /// looks up the user timestamp by the frame's `CaptureTime()` which is
    /// derived from the aligned value.
    ///
    /// In normal usage this is called automatically by the C++ layer —
    /// callers should set `user_timestamp_us` on the `VideoFrame` and let
    /// `capture_frame` / `on_captured_frame` handle the rest.
    pub fn store_user_timestamp(&self, capture_timestamp_us: i64, user_timestamp_us: i64) {
        log::info!(
            target: "user_timestamp",
            "store: capture_ts_us={}, user_ts_us={}",
            capture_timestamp_us,
            user_timestamp_us
        );
        self.sys_handle.store_user_timestamp(capture_timestamp_us, user_timestamp_us);
    }

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_ut::UserTimestampHandler> {
        self.sys_handle.clone()
    }
}

/// Create a sender-side user timestamp handler.
///
/// This handler will embed user timestamps into encoded frames before
/// they are packetized and sent. Use `store_user_timestamp()` to
/// associate a user timestamp with a captured frame's capture timestamp.
pub fn create_sender_handler(
    peer_factory: &PeerConnectionFactory,
    sender: &RtpSender,
) -> UserTimestampHandler {
    UserTimestampHandler {
        sys_handle: sys_ut::new_user_timestamp_sender(
            peer_factory.handle.sys_handle.clone(),
            sender.handle.sys_handle.clone(),
        ),
    }
}

/// Create a receiver-side user timestamp handler.
///
/// This handler will extract user timestamps from received frames
/// and store them in a map keyed by RTP timestamp. Use
/// `lookup_user_timestamp(rtp_timestamp)` to retrieve the user
/// timestamp for a specific decoded frame.
pub fn create_receiver_handler(
    peer_factory: &PeerConnectionFactory,
    receiver: &RtpReceiver,
) -> UserTimestampHandler {
    UserTimestampHandler {
        sys_handle: sys_ut::new_user_timestamp_receiver(
            peer_factory.handle.sys_handle.clone(),
            receiver.handle.sys_handle.clone(),
        ),
    }
}
