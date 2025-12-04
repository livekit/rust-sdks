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

//! Sensor timestamp support for end-to-end timestamp propagation.
//!
//! This module provides functionality to embed sensor/hardware timestamps
//! in encoded video frames as trailers. The timestamps are preserved
//! through the WebRTC pipeline and can be extracted on the receiver side.
//!
//! This works independently of e2ee encryption - timestamps can be
//! embedded even when encryption is disabled.

use cxx::SharedPtr;
use webrtc_sys::sensor_timestamp::ffi as sys_st;

use crate::{
    peer_connection_factory::PeerConnectionFactory,
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

/// Thread-safe store for mapping capture timestamps to sensor timestamps.
///
/// Used on the sender side to correlate video frame capture time with
/// the sensor timestamp that should be embedded in the encoded frame.
#[derive(Clone)]
pub struct SensorTimestampStore {
    sys_handle: SharedPtr<sys_st::SensorTimestampStore>,
}

impl SensorTimestampStore {
    /// Create a new sensor timestamp store.
    pub fn new() -> Self {
        Self {
            sys_handle: sys_st::new_sensor_timestamp_store(),
        }
    }

    /// Store a sensor timestamp associated with a capture timestamp.
    ///
    /// Call this when capturing a video frame with a sensor timestamp.
    /// The `capture_timestamp_us` should match the `timestamp_us` field
    /// of the VideoFrame.
    pub fn store(&self, capture_timestamp_us: i64, sensor_timestamp_us: i64) {
        log::info!(
            target: "sensor_timestamp",
            "store: capture_ts_us={}, sensor_ts_us={}",
            capture_timestamp_us,
            sensor_timestamp_us
        );
        self.sys_handle.store(capture_timestamp_us, sensor_timestamp_us);
    }

    /// Lookup a sensor timestamp by capture timestamp (for debugging).
    /// Returns None if not found.
    pub fn lookup(&self, capture_timestamp_us: i64) -> Option<i64> {
        let result = self.sys_handle.lookup(capture_timestamp_us);
        if result < 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Pop the oldest sensor timestamp from the queue.
    /// Returns None if the queue is empty.
    pub fn pop(&self) -> Option<i64> {
        let result = self.sys_handle.pop();
        if result < 0 {
            None
        } else {
            Some(result)
        }
    }

    /// Peek at the oldest sensor timestamp without removing it.
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

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_st::SensorTimestampStore> {
        self.sys_handle.clone()
    }
}

impl Default for SensorTimestampStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler for sensor timestamp embedding/extraction on RTP streams.
///
/// For sender side: Embeds sensor timestamps as 12-byte trailers on
/// encoded frames before they are sent.
///
/// For receiver side: Extracts sensor timestamps from received frames
/// and makes them available for retrieval.
#[derive(Clone)]
pub struct SensorTimestampHandler {
    sys_handle: SharedPtr<sys_st::SensorTimestampHandler>,
}

impl SensorTimestampHandler {
    /// Enable or disable timestamp embedding/extraction.
    pub fn set_enabled(&self, enabled: bool) {
        self.sys_handle.set_enabled(enabled);
    }

    /// Check if timestamp embedding/extraction is enabled.
    pub fn enabled(&self) -> bool {
        self.sys_handle.enabled()
    }

    /// Get the last received sensor timestamp (receiver side only).
    /// Returns None if no timestamp has been received yet.
    pub fn last_sensor_timestamp(&self) -> Option<i64> {
        if self.sys_handle.has_sensor_timestamp() {
            let ts = self.sys_handle.last_sensor_timestamp();
            if ts >= 0 {
                Some(ts)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn sys_handle(&self) -> SharedPtr<sys_st::SensorTimestampHandler> {
        self.sys_handle.clone()
    }
}

/// Create a sender-side sensor timestamp handler.
///
/// This handler will embed sensor timestamps from the provided store
/// into encoded frames before they are packetized and sent.
pub fn create_sender_handler(
    peer_factory: &PeerConnectionFactory,
    store: &SensorTimestampStore,
    sender: &RtpSender,
) -> SensorTimestampHandler {
    SensorTimestampHandler {
        sys_handle: sys_st::new_sensor_timestamp_sender(
            peer_factory.handle.sys_handle.clone(),
            store.sys_handle(),
            sender.handle.sys_handle.clone(),
        ),
    }
}

/// Create a receiver-side sensor timestamp handler.
///
/// This handler will extract sensor timestamps from received frames
/// and make them available via `last_sensor_timestamp()`.
pub fn create_receiver_handler(
    peer_factory: &PeerConnectionFactory,
    store: &SensorTimestampStore,
    receiver: &RtpReceiver,
) -> SensorTimestampHandler {
    SensorTimestampHandler {
        sys_handle: sys_st::new_sensor_timestamp_receiver(
            peer_factory.handle.sys_handle.clone(),
            store.sys_handle(),
            receiver.handle.sys_handle.clone(),
        ),
    }
}

