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

use super::{FfiServer, FfiHandle};
use crate::{proto, FfiResult, FfiHandleId};
use livekit::data_track::{DataTrackFrame, LocalDataTrack, RemoteDataTrack};

/// FFI wrapper around [`LocalDataTrack`].
#[derive(Clone)]
pub struct FfiLocalDataTrack {
    pub handle_id: FfiHandleId,
    pub inner: LocalDataTrack,
}

/// FFI wrapper around [`RemoteDataTrack`].
#[derive(Clone)]
pub struct FfiRemoteDataTrack {
    pub handle_id: FfiHandleId,
    pub inner: RemoteDataTrack,
}

impl FfiHandle for FfiLocalDataTrack {}
impl FfiHandle for FfiRemoteDataTrack {}

impl FfiLocalDataTrack {
    pub fn from_track(
        server: &'static FfiServer,
        track: LocalDataTrack,
    ) -> proto::OwnedLocalDataTrack {
        let handle_id = server.next_id();
        let info = track.info().clone();
        let track = Self { handle_id, inner: track };
        server.store_handle(handle_id, track);
        proto::OwnedLocalDataTrack {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: info.into()
        }
    }

    pub fn is_published(
        self,
        _server: &'static FfiServer,
        _request: proto::LocalDataTrackIsPublishedRequest,
    ) -> FfiResult<proto::LocalDataTrackIsPublishedResponse> {
        let is_published = self.inner.is_published();
        Ok(proto::LocalDataTrackIsPublishedResponse { is_published })
    }

    pub fn unpublish(
        self,
        _server: &'static FfiServer,
        _request: proto::LocalDataTrackUnpublishRequest,
    ) -> FfiResult<proto::LocalDataTrackUnpublishResponse> {
        self.inner.unpublish();
        Ok(proto::LocalDataTrackUnpublishResponse::default())
    }

    pub fn try_push(
        self,
        _server: &'static FfiServer,
        request: proto::LocalDataTrackTryPushRequest,
    ) -> FfiResult<proto::LocalDataTrackTryPushResponse> {
        let frame: DataTrackFrame = request.frame.into();
        let error: Option<String> = self.inner.try_push(frame).err().map(|err| err.to_string());
        Ok(proto::LocalDataTrackTryPushResponse { error })
    }
}