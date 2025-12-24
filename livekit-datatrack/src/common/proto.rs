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

use crate::{dtp::TrackHandle, DataTrackInfo, InternalError};
use anyhow::anyhow;
use livekit_protocol as proto;

impl TryInto<DataTrackInfo> for proto::DataTrackInfo {
    type Error = InternalError;

    fn try_into(self) -> Result<DataTrackInfo, Self::Error> {
        let handle: TrackHandle = self.pub_handle.try_into().map_err(anyhow::Error::from)?;
        let uses_e2ee = match self.encryption() {
            proto::encryption::Type::None => false,
            proto::encryption::Type::Gcm => true,
            other => Err(anyhow!("Unsupported E2EE type: {:?}", other))?,
        };
        Ok(DataTrackInfo { handle, sid: self.sid, name: self.name, uses_e2ee })
    }
}
