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

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    /// Aggregated send side FEC rates as reported by the RTP modules of all
    /// live video send streams.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct FecSenderMetrics {
        pub sent_video_rate_bps: u32,
        pub sent_fec_rate_bps: u32,
        pub sent_nack_rate_bps: u32,
        pub active_streams: u32,
    }

    unsafe extern "C++" {
        include!("livekit/fec_controller.h");

        /// Snapshot of the aggregated send side FEC rates.
        fn fec_sender_metrics() -> FecSenderMetrics;

        /// Sets the WebRTC field trials applied when the peer connection
        /// factory is created. Returns false when the factory already exists
        /// and the trials cannot take effect.
        fn set_field_trials(field_trials: String) -> bool;
    }
}
