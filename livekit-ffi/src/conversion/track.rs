use crate::{
    proto,
    server::room::{FfiPublication, FfiTrack},
};
use livekit::prelude::*;

impl From<TrackSource> for proto::TrackSource {
    fn from(source: TrackSource) -> proto::TrackSource {
        match source {
            TrackSource::Unknown => proto::TrackSource::SourceUnknown,
            TrackSource::Camera => proto::TrackSource::SourceCamera,
            TrackSource::Microphone => proto::TrackSource::SourceMicrophone,
            TrackSource::Screenshare => proto::TrackSource::SourceScreenshare,
            TrackSource::ScreenshareAudio => proto::TrackSource::SourceScreenshareAudio,
        }
    }
}

impl proto::TrackPublicationInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, ffi_publication: &FfiPublication) -> Self {
        let publication = &ffi_publication.publication;
        Self {
            handle: Some(handle_id),
            name: publication.name(),
            sid: publication.sid().to_string(),
            kind: proto::TrackKind::from(publication.kind()).into(),
            source: proto::TrackSource::from(publication.source()).into(),
            width: publication.dimension().0,
            height: publication.dimension().1,
            mime_type: publication.mime_type(),
            simulcasted: publication.simulcasted(),
            muted: publication.is_muted(),
            remote: publication.is_remote(),
        }
    }
}

impl proto::TrackInfo {
    pub fn from(handle_id: proto::FfiOwnedHandle, ffi_track: &FfiTrack) -> Self {
        let track = &ffi_track.track;
        Self {
            handle: Some(handle_id),
            name: track.name(),
            stream_state: proto::StreamState::from(track.stream_state()).into(),
            sid: track.sid().to_string(),
            kind: proto::TrackKind::from(track.kind()).into(),
            muted: track.is_muted(),
            remote: track.is_remote(),
        }
    }
}

impl From<TrackKind> for proto::TrackKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => proto::TrackKind::KindAudio,
            TrackKind::Video => proto::TrackKind::KindVideo,
        }
    }
}

impl From<StreamState> for proto::StreamState {
    fn from(state: StreamState) -> Self {
        match state {
            StreamState::Active => Self::StateActive,
            StreamState::Paused => Self::StatePaused,
        }
    }
}

impl From<proto::TrackSource> for TrackSource {
    fn from(source: proto::TrackSource) -> Self {
        match source {
            proto::TrackSource::SourceUnknown => TrackSource::Unknown,
            proto::TrackSource::SourceCamera => TrackSource::Camera,
            proto::TrackSource::SourceMicrophone => TrackSource::Microphone,
            proto::TrackSource::SourceScreenshare => TrackSource::Screenshare,
            proto::TrackSource::SourceScreenshareAudio => TrackSource::ScreenshareAudio,
        }
    }
}
