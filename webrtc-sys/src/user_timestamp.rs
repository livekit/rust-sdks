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

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/user_timestamp.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/peer_connection_factory.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type PeerConnectionFactory = crate::peer_connection_factory::ffi::PeerConnectionFactory;

        /// Thread-safe store for mapping capture timestamps to user timestamps.
        pub type UserTimestampStore;

        /// Push a user timestamp to the queue.
        fn store(self: &UserTimestampStore, capture_timestamp_us: i64, user_timestamp_us: i64);

        /// Lookup a user timestamp by capture timestamp (for debugging).
        /// Returns -1 if not found.
        fn lookup(self: &UserTimestampStore, capture_timestamp_us: i64) -> i64;

        /// Pop the oldest user timestamp from the queue.
        /// Returns -1 if empty.
        fn pop(self: &UserTimestampStore) -> i64;

        /// Peek at the oldest user timestamp without removing it.
        /// Returns -1 if empty.
        fn peek(self: &UserTimestampStore) -> i64;

        /// Clear old entries.
        fn prune(self: &UserTimestampStore, max_age_us: i64);

        /// Create a new user timestamp store.
        fn new_user_timestamp_store() -> SharedPtr<UserTimestampStore>;
    }

    unsafe extern "C++" {
        include!("livekit/user_timestamp.h");

        /// Handler for user timestamp embedding/extraction on RTP streams.
        pub type UserTimestampHandler;

        /// Enable/disable timestamp embedding.
        fn set_enabled(self: &UserTimestampHandler, enabled: bool);

        /// Check if timestamp embedding is enabled.
        fn enabled(self: &UserTimestampHandler) -> bool;

        /// Get the last received user timestamp (receiver side only).
        /// Returns -1 if no timestamp has been received yet.
        fn last_user_timestamp(self: &UserTimestampHandler) -> i64;

        /// Check if a user timestamp has been received.
        fn has_user_timestamp(self: &UserTimestampHandler) -> bool;

        /// Create a new user timestamp handler for a sender.
        fn new_user_timestamp_sender(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            store: SharedPtr<UserTimestampStore>,
            sender: SharedPtr<RtpSender>,
        ) -> SharedPtr<UserTimestampHandler>;

        /// Create a new user timestamp handler for a receiver.
        fn new_user_timestamp_receiver(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            store: SharedPtr<UserTimestampStore>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<UserTimestampHandler>;
    }
}

impl_thread_safety!(ffi::UserTimestampStore, Send + Sync);
impl_thread_safety!(ffi::UserTimestampHandler, Send + Sync);
