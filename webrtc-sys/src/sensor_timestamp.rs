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

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/sensor_timestamp.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/peer_connection_factory.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type PeerConnectionFactory = crate::peer_connection_factory::ffi::PeerConnectionFactory;

        /// Thread-safe store for mapping capture timestamps to sensor timestamps.
        pub type SensorTimestampStore;

        /// Push a sensor timestamp to the queue.
        fn store(self: &SensorTimestampStore, capture_timestamp_us: i64, sensor_timestamp_us: i64);

        /// Lookup a sensor timestamp by capture timestamp (for debugging).
        /// Returns -1 if not found.
        fn lookup(self: &SensorTimestampStore, capture_timestamp_us: i64) -> i64;

        /// Pop the oldest sensor timestamp from the queue.
        /// Returns -1 if empty.
        fn pop(self: &SensorTimestampStore) -> i64;

        /// Peek at the oldest sensor timestamp without removing it.
        /// Returns -1 if empty.
        fn peek(self: &SensorTimestampStore) -> i64;

        /// Clear old entries.
        fn prune(self: &SensorTimestampStore, max_age_us: i64);

        /// Create a new sensor timestamp store.
        fn new_sensor_timestamp_store() -> SharedPtr<SensorTimestampStore>;
    }

    unsafe extern "C++" {
        include!("livekit/sensor_timestamp.h");

        /// Handler for sensor timestamp embedding/extraction on RTP streams.
        pub type SensorTimestampHandler;

        /// Enable/disable timestamp embedding.
        fn set_enabled(self: &SensorTimestampHandler, enabled: bool);

        /// Check if timestamp embedding is enabled.
        fn enabled(self: &SensorTimestampHandler) -> bool;

        /// Get the last received sensor timestamp (receiver side only).
        /// Returns -1 if no timestamp has been received yet.
        fn last_sensor_timestamp(self: &SensorTimestampHandler) -> i64;

        /// Check if a sensor timestamp has been received.
        fn has_sensor_timestamp(self: &SensorTimestampHandler) -> bool;

        /// Create a new sensor timestamp handler for a sender.
        fn new_sensor_timestamp_sender(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            store: SharedPtr<SensorTimestampStore>,
            sender: SharedPtr<RtpSender>,
        ) -> SharedPtr<SensorTimestampHandler>;

        /// Create a new sensor timestamp handler for a receiver.
        fn new_sensor_timestamp_receiver(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            store: SharedPtr<SensorTimestampStore>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<SensorTimestampHandler>;
    }
}

impl_thread_safety!(ffi::SensorTimestampStore, Send + Sync);
impl_thread_safety!(ffi::SensorTimestampHandler, Send + Sync);

#[cfg(test)]
mod tests {
    #[test]
    fn test_sensor_timestamp_store_creation() {
        // Basic test to ensure the store can be created
        // Full testing requires a running WebRTC context
    }
}

