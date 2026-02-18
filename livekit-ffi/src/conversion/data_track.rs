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

use crate::proto;
use livekit::data_track::{DataTrackFrame, DataTrackInfo, DataTrackOptions};

impl From<proto::DataTrackOptions> for DataTrackOptions {
    fn from(options: proto::DataTrackOptions) -> Self {
        Self::new(options.name)
    }
}

impl From<DataTrackInfo> for proto::DataTrackInfo {
    fn from(info: DataTrackInfo) -> Self {
        Self {
            name: info.name().to_string(),
            sid: info.sid().to_string(),
            uses_e2ee: info.uses_e2ee(),
        }
    }
}

impl From<DataTrackFrame> for proto::DataTrackFrame {
    fn from(frame: DataTrackFrame) -> Self {
        Self { payload: frame.payload().into(), user_timestamp: frame.user_timestamp() }
    }
}

impl From<proto::DataTrackFrame> for DataTrackFrame {
    fn from(frame: proto::DataTrackFrame) -> Self {
        let mut frame = Self::new(frame.payload);
        if let Some(timestamp) = frame.user_timestamp() {
            frame = frame.with_user_timestamp(timestamp);
        }
        frame
    }
}
