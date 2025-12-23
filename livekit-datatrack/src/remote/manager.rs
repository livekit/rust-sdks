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

#[derive(Debug, Clone)]
pub(crate) struct TrackInner {}

// #[derive(Debug)]
// pub struct SubManagerOptions {
//     // Dependencies
//     // - E2EE
//     // - Signaling
//     //   - Tx UpdateDataSubscription
//     //   - Rx DataTrackPublishedResponse, DataTrackUnpublishedResponse,
//     // - Data track channel
//     //   - Rx
// }

// #[derive(Debug)]
// pub struct SubManager {
//     options: SubManagerOptions,
//     sub_tracks: DashMap<TrackHandle, Descriptor>
// }

// impl SubManager {
//     pub fn new(options: SubManagerOptions) -> Self {
//         Self { options, sub_tracks: DashMap::default() }
//     }
// }

// // handles: track published/unpublished
// // maintains state
// // creates DataTrack<Remote> when track published
// // send signal to existing ones when track unpublished

// #[derive(Debug)]
// pub struct Descriptor {
//     // tx
//     // data channel -> dtp decode -> frame -> tx
// }