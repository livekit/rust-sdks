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

use super::TrackHandle;
use crate::api::{DataTrack, Local, PublishError, PublishOptions};
use dashmap::DashMap;
use livekit_protocol as proto;

// Question: mechanism for signaling tx/rx. Options:
// 1.
// 2.

#[derive(Debug)]
pub struct PubManagerOptions {
    // Dependencies:
    // - E2EE
    // - Signaling
    //   - Tx AddTrackRequest, UnpublishDataTrackRequest
    //   - Rx DataTrackPublishedResponse, DataTrackUnpublishedResponse, RequestResponse
    // - Data track channel
    //   - Tx
}

#[derive(Debug)]
pub struct PubManager {
    options: PubManagerOptions,
    pub_tracks: DashMap<TrackHandle, Descriptor>,
}

impl PubManager {
    pub fn new(options: PubManagerOptions) -> Self {
        Self { options, pub_tracks: DashMap::default() }
    }

    // from track published, participant update?
    pub fn update_state() {
        todo!()
    }

    pub async fn publish(options: PublishOptions) -> Result<DataTrack<Local>, PublishError> {
        let request = options.into_add_track_request(false); // set based on E2EE options

        // TODO: send request, await response
        let response = proto::DataTrackPublishedResponse::default();


        todo!()
    }

    // handle update
}

#[derive(Debug)]
struct Descriptor {
    // minimal info
    // stats (fps calculation, total sent, etc.)
    // rx
    // publish -> dtp encode -> data channel
}

impl Into<proto::AddDataTrackRequest> for PublishOptions {
    fn into(self) -> proto::AddDataTrackRequest {
        todo!()
    }
}

impl PublishOptions {
    fn into_add_track_request(self, use_e2ee: bool) -> proto::AddDataTrackRequest {
        let encryption = if self.disable_e2ee || !use_e2ee {
            proto::encryption::Type::None
        } else {
            proto::encryption::Type::Gcm
        };
        proto::AddDataTrackRequest {
            name: self.name,
            mime_type: self.mime.to_string(),
            encryption: encryption.into(),
        }
    }
}
