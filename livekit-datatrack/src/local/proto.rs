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

use super::manager::{PublishRequestEvent, UnpublishRequestEvent};
use crate::{
    dtp::TrackHandle, local::manager::PublishResultEvent, DataTrackInfo, InternalError,
    PublishError,
};
use anyhow::{anyhow, Context};
use livekit_protocol as proto;
use std::mem;

// MARK: - Output event -> protocol

impl From<PublishRequestEvent> for proto::PublishDataTrackRequest {
    fn from(event: PublishRequestEvent) -> Self {
        use proto::encryption::Type;
        let encryption = if event.uses_e2ee { Type::Gcm } else { Type::None }.into();
        Self { pub_handle: event.handle.into(), name: event.name, encryption }
    }
}

impl From<UnpublishRequestEvent> for proto::UnpublishDataTrackRequest {
    fn from(event: UnpublishRequestEvent) -> Self {
        Self { pub_handle: event.handle.into() }
    }
}

// MARK: - Protocol -> input event

impl TryFrom<proto::PublishDataTrackResponse> for PublishResultEvent {
    type Error = InternalError;

    fn try_from(msg: proto::PublishDataTrackResponse) -> Result<Self, Self::Error> {
        let info: DataTrackInfo = msg.info.context("Missing info")?.try_into()?;
        Ok(Self { handle: info.handle, result: Ok(info) })
    }
}

impl TryFrom<proto::DataTrackInfo> for DataTrackInfo {
    type Error = InternalError;

    fn try_from(msg: proto::DataTrackInfo) -> Result<Self, Self::Error> {
        let handle: TrackHandle = msg.pub_handle.try_into().map_err(anyhow::Error::from)?;
        let uses_e2ee = match msg.encryption() {
            proto::encryption::Type::None => false,
            proto::encryption::Type::Gcm => true,
            other => Err(anyhow!("Unsupported E2EE type: {:?}", other))?,
        };
        Ok(Self { handle, sid: msg.sid, name: msg.name, uses_e2ee })
    }
}

fn publish_result_from_request_response(
    msg: &proto::RequestResponse,
) -> Option<PublishResultEvent> {
    use proto::request_response::{Reason, Request};
    let Some(request) = &msg.request else { return None };
    let Request::PublishDataTrack(request) = request else { return None };
    let Ok(handle) = TryInto::<TrackHandle>::try_into(request.pub_handle) else { return None };
    let error = match msg.reason() {
        // If new error reasons are introduced in the future, consider adding them
        // to the public error enum if they are useful to the user.
        Reason::NotAllowed => PublishError::NotAllowed,
        Reason::DuplicateName => PublishError::DuplicateName,
        _ => PublishError::Internal(anyhow!("SFU rejected: {}", msg.message).into()),
    };
    let event = PublishResultEvent { handle, result: Err(error) };
    Some(event)
}

fn publish_results_from_sync_state(
    msg: &mut proto::SyncState,
) -> Result<Vec<PublishResultEvent>, InternalError> {
    mem::take(&mut msg.publish_data_tracks)
        .into_iter()
        .map(TryInto::<PublishResultEvent>::try_into)
        .collect::<Result<Vec<_>, InternalError>>()
}
