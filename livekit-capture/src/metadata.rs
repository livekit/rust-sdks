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

/// Packet-trailer metadata associated with a captured frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameMetadata {
    /// Wall-clock capture timestamp in microseconds.
    pub user_timestamp: Option<u64>,
    /// Monotonically increasing frame identifier.
    pub frame_id: Option<u32>,
}

impl FrameMetadata {
    pub(crate) fn into_rtc(self) -> Option<livekit::webrtc::video_frame::FrameMetadata> {
        (self.user_timestamp.is_some() || self.frame_id.is_some()).then_some(
            livekit::webrtc::video_frame::FrameMetadata {
                user_timestamp: self.user_timestamp,
                frame_id: self.frame_id,
            },
        )
    }
}
