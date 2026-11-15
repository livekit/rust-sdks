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

//! Everything holding [`crate::server::track::Track`] onto the protobuf path: the handle
//! bridge and the request handlers.
//!
//! This whole file goes when the protobuf path does. What it leaves behind over there is
//! the `handle_id` field on the struct, and the `pub(crate)` these reach through.

use std::sync::Arc;

use crate::server::room::{Track, TrackError};
use crate::{proto, server, FfiError, FfiResult};

crate::migrate_from_ffi!(Track);

impl From<TrackError> for FfiError {
    fn from(err: TrackError) -> Self {
        FfiError::InvalidRequest(err.to_string().into())
    }
}

impl Track {
    /// Resolves a track handle the FFI client minted, as the request handlers see it.
    pub(crate) fn of_handle(
        server: &'static server::FfiServer,
        handle_id: crate::FfiHandleId,
    ) -> FfiResult<Arc<Self>> {
        Ok(server.retrieve_handle::<Arc<Self>>(handle_id)?.clone())
    }

    pub fn create_video_track_ffi(
        create: proto::CreateVideoTrackRequest,
    ) -> FfiResult<proto::CreateVideoTrackResponse> {
        let track = Self::from_video_source(create.name, create.source_handle)?;
        Ok(proto::CreateVideoTrackResponse { track: track.into_owned_ffi() })
    }

    pub fn create_audio_track_ffi(
        create: proto::CreateAudioTrackRequest,
    ) -> FfiResult<proto::CreateAudioTrackResponse> {
        let track = Self::from_audio_source(create.name, create.source_handle)?;
        Ok(proto::CreateAudioTrackResponse { track: track.into_owned_ffi() })
    }

    pub fn mute_ffi(
        server: &'static server::FfiServer,
        request: proto::LocalTrackMuteRequest,
    ) -> FfiResult<proto::LocalTrackMuteResponse> {
        let track = Self::of_handle(server, request.track_handle)?;
        Ok(proto::LocalTrackMuteResponse { muted: track.set_muted(request.mute)? })
    }

    pub fn enable_ffi(
        server: &'static server::FfiServer,
        request: proto::EnableRemoteTrackRequest,
    ) -> FfiResult<proto::EnableRemoteTrackResponse> {
        let track = Self::of_handle(server, request.track_handle)?;
        Ok(proto::EnableRemoteTrackResponse { enabled: track.set_enabled(request.enabled)? })
    }

    /// Still protobuf-only. `RtcStats` is a large tree of records whose uniffi shape is
    /// its own layer's worth of mirroring, and nothing above needs it typed yet, so the
    /// stats keep crossing as protobuf and the track is only resolved from its handle.
    pub fn get_stats_ffi(
        server: &'static server::FfiServer,
        get_stats: proto::GetStatsRequest,
    ) -> FfiResult<proto::GetStatsResponse> {
        let track = Self::of_handle(server, get_stats.track_handle)?;
        let async_id = server.resolve_async_id(get_stats.request_async_id);
        let handle = server.async_runtime.spawn(async move {
            let (error, stats) = match track.inner.get_stats().await {
                Ok(stats) => (None, stats.into_iter().map(Into::into).collect()),
                Err(err) => (Some(err.to_string()), Vec::default()),
            };
            let _ = server.send_event(proto::GetStatsCallback { async_id, error, stats }.into());
        });
        server.watch_panic(handle);
        Ok(proto::GetStatsResponse { async_id })
    }

    /// Publishes the track and describes it the way the protobuf path expects.
    pub(crate) fn into_owned_ffi(self: Arc<Self>) -> proto::OwnedTrack {
        let info = proto::TrackInfo::from(&*self);
        proto::OwnedTrack { handle: proto::FfiOwnedHandle { id: self.ffi_handle_id() }, info }
    }
}

/// Direct-call tests for [`Track`] crossing the FFI seam.
///
/// A track is a thin wrapper over a livekit type that is itself shared, so "one object,
/// two surfaces" cannot be probed through its state the way a stream's queue or a
/// resampler's filter can: muting through either handle would show on both however many
/// objects there were. What there is to pin down is the identity and the handle's
/// lifetime, which is what these do.
#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::server::video_source::FfiVideoSource;
    use crate::FFI_SERVER;
    use livekit::webrtc::video_source::{
        native::NativeVideoSource, RtcVideoSource, VideoResolution,
    };

    /// A video source published as an FFI handle, the way `new_video_source` publishes one.
    fn video_source() -> crate::FfiHandleId {
        let handle_id = FFI_SERVER.next_id();
        FFI_SERVER.store_handle(
            handle_id,
            FfiVideoSource {
                handle_id,
                source_type: proto::VideoSourceType::VideoSourceNative,
                source: RtcVideoSource::Native(NativeVideoSource::new(
                    VideoResolution { width: 64, height: 32 },
                    false,
                )),
            },
        );
        handle_id
    }

    /// A track created over uniffi publishes a handle, the handle resolves back to the
    /// very same object, `take_ffi_handle_id` hands sole ownership back, and publishing
    /// again restores the same id.
    #[serial_test::parallel]
    #[test]
    fn a_track_hands_back_and_forth_across_the_seam() {
        FFI_SERVER.async_runtime.block_on(async {
            let source_handle = video_source();
            let track = Track::from_video_source("probe".into(), source_handle)
                .expect("the handle resolves to a video source");
            assert_eq!(Arc::strong_count(&track), 1, "an unpublished track has no other owner");
            assert_eq!(track.name(), "probe");

            let handle = track.clone().ffi_handle_id();
            let same = Track::from_ffi_handle_id(handle).expect("the published id resolves");
            assert!(Arc::ptr_eq(&same, &track), "the id resolves to the object that published it");

            track.take_ffi_handle_id().expect("the FFI side co-owned the track");
            assert!(
                Track::from_ffi_handle_id(handle).is_err(),
                "a released handle is absent from the map until it is published again"
            );

            assert_eq!(track.clone().ffi_handle_id(), handle, "republishing keeps the id");
            assert!(Arc::ptr_eq(
                &Track::from_ffi_handle_id(handle).expect("the republished handle resolves"),
                &track
            ));

            FFI_SERVER.drop_handle(handle);
            FFI_SERVER.drop_handle(source_handle);
        });
    }

    /// Muting is the publisher's to do and enabling the subscriber's, so a local track
    /// takes one and refuses the other.
    #[serial_test::parallel]
    #[test]
    fn a_local_track_mutes_but_does_not_enable() {
        FFI_SERVER.async_runtime.block_on(async {
            let source_handle = video_source();
            let track = Track::from_video_source("probe".into(), source_handle).unwrap();

            assert!(!track.is_muted());
            assert_eq!(track.set_muted(true).expect("a local track mutes"), true);
            assert!(track.is_muted());
            assert_eq!(track.set_muted(false).expect("and unmutes"), false);

            assert!(
                matches!(track.set_enabled(false), Err(TrackError::WrongTrackKind(_))),
                "enabling is for a track being received, not one being sent"
            );

            FFI_SERVER.drop_handle(source_handle);
        });
    }

    /// A bad source handle is reported, not panicked on.
    #[serial_test::parallel]
    #[test]
    fn an_unknown_source_is_an_error() {
        assert!(matches!(
            Track::from_audio_source("probe".into(), 999_999),
            Err(TrackError::InvalidSource(_))
        ));
    }
}
