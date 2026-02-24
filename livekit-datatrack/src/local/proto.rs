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

use super::events::*;
use crate::{
    api::{DataTrackInfo, DataTrackSid, InternalError, PublishError},
    packet::Handle,
};
use anyhow::{anyhow, Context};
use livekit_protocol as proto;
use std::{borrow::Borrow, sync::RwLock};

// MARK: - Output event -> protocol

impl From<SfuPublishRequest> for proto::PublishDataTrackRequest {
    fn from(event: SfuPublishRequest) -> Self {
        use proto::encryption::Type;
        let encryption = if event.uses_e2ee { Type::Gcm } else { Type::None }.into();
        Self { pub_handle: event.handle.into(), name: event.name, encryption }
    }
}

impl From<SfuUnpublishRequest> for proto::UnpublishDataTrackRequest {
    fn from(event: SfuUnpublishRequest) -> Self {
        Self { pub_handle: event.handle.into() }
    }
}

// MARK: - Protocol -> input event

impl TryFrom<proto::PublishDataTrackResponse> for SfuPublishResponse {
    type Error = InternalError;

    fn try_from(msg: proto::PublishDataTrackResponse) -> Result<Self, Self::Error> {
        let info: DataTrackInfo = msg.info.context("Missing info")?.try_into()?;
        Ok(Self { handle: info.pub_handle, result: Ok(info) })
    }
}

impl TryFrom<proto::UnpublishDataTrackResponse> for SfuUnpublishResponse {
    type Error = InternalError;

    fn try_from(msg: proto::UnpublishDataTrackResponse) -> Result<Self, Self::Error> {
        let handle: Handle =
            msg.info.context("Missing info")?.pub_handle.try_into().map_err(anyhow::Error::from)?;
        Ok(Self { handle })
    }
}

impl TryFrom<proto::DataTrackInfo> for DataTrackInfo {
    type Error = InternalError;

    fn try_from(msg: proto::DataTrackInfo) -> Result<Self, Self::Error> {
        let handle: Handle = msg.pub_handle.try_into().map_err(anyhow::Error::from)?;
        let uses_e2ee = match msg.encryption() {
            proto::encryption::Type::None => false,
            proto::encryption::Type::Gcm => true,
            other => Err(anyhow!("Unsupported E2EE type: {:?}", other))?,
        };
        let sid: DataTrackSid = msg.sid.try_into().map_err(anyhow::Error::from)?;
        Ok(Self { pub_handle: handle, sid: RwLock::new(sid).into(), name: msg.name, uses_e2ee })
    }
}

pub fn publish_result_from_request_response(
    msg: &proto::RequestResponse,
) -> Option<SfuPublishResponse> {
    use proto::request_response::{Reason, Request};
    let Some(request) = &msg.request else { return None };
    let Request::PublishDataTrack(request) = request else { return None };
    let Ok(handle) = TryInto::<Handle>::try_into(request.pub_handle) else { return None };
    let error = match msg.reason() {
        // If new error reasons are introduced in the future, consider adding them
        // to the public error enum if they are useful to the user.
        Reason::NotAllowed => PublishError::NotAllowed,
        Reason::DuplicateName => PublishError::DuplicateName,
        Reason::InvalidName => PublishError::InvalidName,
        _ => PublishError::Internal(anyhow!("SFU rejected: {}", msg.message).into()),
    };
    let event = SfuPublishResponse { handle, result: Err(error) };
    Some(event)
}

// MARK: - Sync state support

impl From<DataTrackInfo> for proto::DataTrackInfo {
    fn from(info: DataTrackInfo) -> Self {
        let encryption = if info.uses_e2ee() {
            proto::encryption::Type::Gcm
        } else {
            proto::encryption::Type::None
        };
        Self {
            pub_handle: info.pub_handle.into(),
            sid: info.sid().to_string(),
            name: info.name,
            encryption: encryption as i32,
        }
    }
}

/// Form publish responses for each publish data track to support sync state.
pub fn publish_responses_for_sync_state(
    published_tracks: impl IntoIterator<Item = impl Borrow<DataTrackInfo>>,
) -> Vec<proto::PublishDataTrackResponse> {
    published_tracks
        .into_iter()
        .map(|info| proto::PublishDataTrackResponse { info: Some(info.borrow().clone().into()) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fake::{Fake, Faker};

    #[test]
    fn test_from_publish_request_event() {
        let event = SfuPublishRequest {
            handle: 1u32.try_into().unwrap(),
            name: "track".into(),
            uses_e2ee: true,
        };
        let request: proto::PublishDataTrackRequest = event.into();
        assert_eq!(request.pub_handle, 1);
        assert_eq!(request.name, "track");
        assert_eq!(request.encryption(), proto::encryption::Type::Gcm);
    }

    #[test]
    fn test_from_unpublish_request_event() {
        let event = SfuUnpublishRequest { handle: 1u32.try_into().unwrap() };
        let request: proto::UnpublishDataTrackRequest = event.into();
        assert_eq!(request.pub_handle, 1);
    }

    #[test]
    fn test_from_publish_response() {
        let response = proto::PublishDataTrackResponse {
            info: proto::DataTrackInfo {
                pub_handle: 1,
                sid: "DTR_1234".into(),
                name: "track".into(),
                encryption: proto::encryption::Type::Gcm.into(),
            }
            .into(),
        };
        let event: SfuPublishResponse = response.try_into().unwrap();
        assert_eq!(event.handle, 1u32.try_into().unwrap());

        let info = event.result.expect("Expected ok result");
        assert_eq!(info.pub_handle, 1u32.try_into().unwrap());
        assert_eq!(*info.sid.read().unwrap(), "DTR_1234".to_string().try_into().unwrap());
        assert_eq!(info.name, "track");
        assert!(info.uses_e2ee);
    }

    #[test]
    fn test_from_request_response() {
        use proto::request_response::{Reason, Request};
        let response = proto::RequestResponse {
            request: Request::PublishDataTrack(proto::PublishDataTrackRequest {
                pub_handle: 1,
                ..Default::default()
            })
            .into(),
            reason: Reason::NotAllowed.into(),
            ..Default::default()
        };

        let event = publish_result_from_request_response(&response).expect("Expected event");
        assert_eq!(event.handle, 1u32.try_into().unwrap());
        assert!(matches!(event.result, Err(PublishError::NotAllowed)));
    }

    #[test]
    fn test_publish_responses_for_sync_state() {
        let mut first: DataTrackInfo = Faker.fake();
        first.uses_e2ee = true;

        let mut second: DataTrackInfo = Faker.fake();
        second.uses_e2ee = false;

        let publish_responses = publish_responses_for_sync_state(vec![first, second]);
        assert_eq!(
            publish_responses[0].info.as_ref().unwrap().encryption(),
            proto::encryption::Type::Gcm
        );
        assert_eq!(
            publish_responses[1].info.as_ref().unwrap().encryption(),
            proto::encryption::Type::None
        );
    }
}
