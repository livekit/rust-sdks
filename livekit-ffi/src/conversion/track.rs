use crate::{proto, FFIHandleId};
use livekit::options::{AudioCaptureOptions, VideoCaptureOptions};
use livekit::prelude::*;

impl From<proto::VideoCaptureOptions> for VideoCaptureOptions {
    fn from(opts: proto::VideoCaptureOptions) -> Self {
        Self {
            resolution: opts.resolution.unwrap_or_default().into(),
        }
    }
}

impl From<proto::AudioCaptureOptions> for AudioCaptureOptions {
    fn from(opts: proto::AudioCaptureOptions) -> Self {
        Self {
            echo_cancellation: opts.echo_cancellation,
            auto_gain_control: opts.auto_gain_control,
            noise_suppression: opts.noise_suppression,
        }
    }
}

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

macro_rules! impl_publication_into {
    ($p:ty) => {
        impl From<$p> for proto::TrackPublicationInfo {
            fn from(p: $p) -> Self {
                Self {
                    name: p.name(),
                    sid: p.sid().to_string(),
                    kind: proto::TrackKind::from(p.kind()).into(),
                    source: proto::TrackSource::from(p.source()).into(),
                    width: p.dimension().0,
                    height: p.dimension().1,
                    mime_type: p.mime_type(),
                    simulcasted: p.simulcasted(),
                    muted: p.is_muted(),
                    remote: p.is_remote(),
                }
            }
        }
    };
}

impl_publication_into!(&LocalTrackPublication);
impl_publication_into!(&RemoteTrackPublication);
impl_publication_into!(&TrackPublication);

macro_rules! impl_track_into {
    ($fnc:ident, $t:ty) => {
        impl proto::TrackInfo {
            pub fn $fnc(handle_id: FFIHandleId, track: $t) -> Self {
                Self {
                    opt_handle: Some(handle_id.into()),
                    name: track.name(),
                    stream_state: proto::StreamState::from(track.stream_state()).into(),
                    sid: track.sid().to_string(),
                    kind: proto::TrackKind::from(track.kind()).into(),
                    muted: track.is_muted(),
                    remote: track.is_remote(),
                }
            }
        }
    };
}

impl_track_into!(from_local_audio_track, &LocalAudioTrack);
impl_track_into!(from_local_video_track, &LocalVideoTrack);
impl_track_into!(from_remote_audio_track, &RemoteAudioTrack);
impl_track_into!(from_remote_video_track, &RemoteVideoTrack);
impl_track_into!(from_track, &Track);
impl_track_into!(from_local_track, &LocalTrack);
impl_track_into!(from_remote_track, &RemoteTrack);

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
