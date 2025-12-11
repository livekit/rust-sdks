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
//! This works independently of e2ee encryption - timestamps can be
//! embedded even when encryption is disabled.

use cxx::SharedPtr;
use webrtc_sys::user_timestamp::ffi as sys_ut;

use crate::{
    peer_connection_factory::PeerConnectionFactory,
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

/// Thread-safe store for mapping capture timestamps to user timestamps.
///
/// Used on the sender side to correlate video frame capture time with
/// the user timestamp that should be embedded in the encoded frame.
#[derive(Clone)]
pub struct UserTimestampStore {
    sys_handle: SharedPtr<sys_ut::UserTimestampStore>,
}

impl UserTimestampStore {
    /// Create a new user timestamp store.
    pub fn new() -> Self {
        Self {
            sys_handle: sys_ut::new_user_timestamp_store(),
        }
    }

    /// Store a user timestamp associated with a capture timestamp.
    ///
    /// Call this when capturing a video frame with a user timestamp.
    /// The `capture_timestamp_us` should match the `timestamp_us` field
    /// of the VideoFrame.
    pub fn store(&self, capture_timestamp_us: i64, user_timestamp_us: i64) {
        log::info!(
            target: "user_timestamp",
            "store: capture_ts_us={}, user_ts_us={}",
            capture_timestamp_us,
            user_timestamp_us
        );
        self.sys_handle.store(capture_timestamp_us, user_timestamp_us);
    }

    /// Lookup a user timestamp by capture timestamp (for debugging).
    /// Returns None if not found.
    pub fn lookup(&self, capture_timestamp_us: i64) -> Option<i64> {
        let result = self.sys_handle.lookup(capture_timestamp_us);
        if result < 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Pop the oldest user timestamp from the queue.
    /// Returns None if the queue is empty.
    pub fn pop(&self) -> Option<i64> {
        let result = self.sys_handle.pop();
        if result < 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Peek at the oldest user timestamp without removing it.
    /// Returns None if the queue is empty.
    pub fn peek(&self) -> Option<i64> {
        let result = self.sys_handle.peek();
        if result < 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Clear old entries (older than the given threshold in microseconds).
    pub fn prune(&self, max_age_us: i64) {
        self.sys_handle.prune(max_age_us);
    }

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_ut::UserTimestampStore> {
        self.sys_handle.clone()
    }
}

impl Default for UserTimestampStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for user timestamp embedding/extraction on RTP streams.
///
/// For sender side: Embeds user timestamps as 12-byte trailers on
/// encoded frames before they are sent.
///
/// For receiver side: Extracts user timestamps from received frames
/// and makes them available for retrieval.
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

    /// Get the last received user timestamp (receiver side only).
    /// Returns None if no timestamp has been received yet.
    pub fn last_user_timestamp(&self) -> Option<i64> {
        if self.sys_handle.has_user_timestamp() {
            let ts = self.sys_handle.last_user_timestamp();
            if ts >= 0 {
                Some(ts)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_ut::UserTimestampHandler> {
        self.sys_handle.clone()
    }
}

/// Create a sender-side user timestamp handler.
///
/// This handler will embed user timestamps from the provided store
/// into encoded frames before they are packetized and sent.
pub fn create_sender_handler(
    peer_factory: &PeerConnectionFactory,
    store: &UserTimestampStore,
    sender: &RtpSender,
) -> UserTimestampHandler {
    UserTimestampHandler {
        sys_handle: sys_ut::new_user_timestamp_sender(
            peer_factory.handle.sys_handle.clone(),
            store.sys_handle(),
            sender.handle.sys_handle.clone(),
        ),
    }
}

/// Create a receiver-side user timestamp handler.
///
/// This handler will extract user timestamps from received frames
/// and make them available via `last_user_timestamp()`.
pub fn create_receiver_handler(
    peer_factory: &PeerConnectionFactory,
    store: &UserTimestampStore,
    receiver: &RtpReceiver,
) -> UserTimestampHandler {
    UserTimestampHandler {
        sys_handle: sys_ut::new_user_timestamp_receiver(
            peer_factory.handle.sys_handle.clone(),
            store.sys_handle(),
            receiver.handle.sys_handle.clone(),
        ),
    }
}


