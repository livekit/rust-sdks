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

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    unsafe extern "C++" {
        include!("livekit/packet_trailer.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/peer_connection_factory.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type PeerConnectionFactory = crate::peer_connection_factory::ffi::PeerConnectionFactory;

        /// Handler for packet trailer embedding/extraction on RTP streams.
        pub type PacketTrailerHandler;

        /// Enable/disable timestamp embedding.
        fn set_enabled(self: &PacketTrailerHandler, enabled: bool);

        /// Check if timestamp embedding is enabled.
        fn enabled(self: &PacketTrailerHandler) -> bool;

        /// Lookup the user timestamp for a given RTP timestamp (receiver side).
        /// Returns -1 if not found. The entry is removed after lookup.
        /// Also caches the frame_id for retrieval via last_lookup_frame_id().
        fn lookup_timestamp(self: &PacketTrailerHandler, rtp_timestamp: u32) -> u64;

        /// Returns the frame_id from the most recent successful
        /// lookup_timestamp() call.
        fn last_lookup_frame_id(self: &PacketTrailerHandler) -> u32;

        /// Store frame metadata for a given capture timestamp (sender side).
        fn store_frame_metadata(
            self: &PacketTrailerHandler,
            capture_timestamp_us: i64,
            user_timestamp: u64,
            frame_id: u32,
        );

        /// Create a new packet trailer handler for a sender.
        fn new_packet_trailer_sender(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            sender: SharedPtr<RtpSender>,
        ) -> SharedPtr<PacketTrailerHandler>;

        /// Create a new packet trailer handler for a receiver.
        fn new_packet_trailer_receiver(
            peer_factory: SharedPtr<PeerConnectionFactory>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<PacketTrailerHandler>;
    }
}

impl_thread_safety!(ffi::PacketTrailerHandler, Send + Sync);
