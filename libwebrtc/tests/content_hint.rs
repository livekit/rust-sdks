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

//! Needs the native libwebrtc, like the other tests that create a `PeerConnectionFactory`.

#![cfg(not(target_arch = "wasm32"))]

use libwebrtc::{
    peer_connection_factory::{native::PeerConnectionFactoryExt, PeerConnectionFactory},
    video_source::{native::NativeVideoSource, VideoResolution},
    video_track::ContentHint,
};

// `NativeVideoSource` spawns onto the ambient Tokio runtime.
#[tokio::test]
async fn content_hint_can_be_set_on_a_video_track() {
    let factory = PeerConnectionFactory::default();
    let source = NativeVideoSource::new(VideoResolution { width: 1280, height: 720 }, true);
    let track = factory.create_video_track("content-hint-test", source);

    assert_eq!(track.content_hint(), ContentHint::None);

    for hint in [ContentHint::Text, ContentHint::Detailed, ContentHint::Fluid, ContentHint::None] {
        track.set_content_hint(hint);
        assert_eq!(track.content_hint(), hint);
    }
}
