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

//! Direct core objects. UDL declares existing methods; extension traits only adapt owned returns.
pub use livekit::publication::{
    LocalTrackPublication as FfiLocalTrackPublication,
    RemoteTrackPublication as FfiRemoteTrackPublication,
};
pub use livekit::track::TrackDimension as FfiTrackDimension;
pub use livekit::{
    participant::Participant as FfiParticipant,
    publication::TrackPublication as FfiTrackPublication, track::Track as FfiTrack,
};
use livekit_common::ClientCapability;
use std::{collections::HashMap, sync::Arc};

pub(crate) trait ParticipantView {
    fn is_local(&self) -> bool;
    fn capabilities(&self) -> Option<Vec<ClientCapability>>;
    fn publications(&self) -> HashMap<String, Arc<FfiTrackPublication>>;
}
impl ParticipantView for FfiParticipant {
    fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }
    fn capabilities(&self) -> Option<Vec<ClientCapability>> {
        match self {
            Self::Remote(p) => Some(p.capabilities()),
            Self::Local(_) => None,
        }
    }
    fn publications(&self) -> HashMap<String, Arc<FfiTrackPublication>> {
        self.track_publications().into_iter().map(|(sid, p)| (sid.into(), Arc::new(p))).collect()
    }
}

pub(crate) trait PublicationView {
    fn dimensions(&self) -> Arc<FfiTrackDimension>;
    fn media_track(&self) -> Option<Arc<FfiTrack>>;
    fn remote(&self) -> Option<Arc<FfiRemoteTrackPublication>>;
    fn local(&self) -> Option<Arc<FfiLocalTrackPublication>>;
}
impl PublicationView for FfiTrackPublication {
    fn dimensions(&self) -> Arc<FfiTrackDimension> {
        Arc::new(self.dimension())
    }
    fn media_track(&self) -> Option<Arc<FfiTrack>> {
        self.track().map(Arc::new)
    }
    fn remote(&self) -> Option<Arc<FfiRemoteTrackPublication>> {
        match self {
            Self::Remote(p) => Some(Arc::new(p.clone())),
            _ => None,
        }
    }
    fn local(&self) -> Option<Arc<FfiLocalTrackPublication>> {
        match self {
            Self::Local(p) => Some(Arc::new(p.clone())),
            _ => None,
        }
    }
}

pub(crate) trait DimensionView {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}
impl DimensionView for FfiTrackDimension {
    fn width(&self) -> u32 {
        self.0
    }
    fn height(&self) -> u32 {
        self.1
    }
}

pub(crate) trait RemotePublicationView {
    fn media_track(&self) -> Option<Arc<FfiTrack>>;
}
impl RemotePublicationView for FfiRemoteTrackPublication {
    fn media_track(&self) -> Option<Arc<FfiTrack>> {
        self.track().map(|t| Arc::new(t.into()))
    }
}
