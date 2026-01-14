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

use super::manager::{PublicationUpdatesEvent, SubscriberHandlesEvent, SubscriptionUpdatedEvent};
use crate::{
    api::{DataTrackInfo, DataTrackSid, InternalError},
    dtp::Handle,
};
use livekit_protocol::{self as proto, ParticipantInfo};
use std::{collections::HashMap, mem};

// MARK: - Protocol -> input event

impl TryFrom<proto::DataTrackSubscriberHandles> for SubscriberHandlesEvent {
    type Error = InternalError;

    fn try_from(msg: proto::DataTrackSubscriberHandles) -> Result<Self, Self::Error> {
        let mapping = msg
            .sub_handles
            .into_iter()
            .map(|(handle, info)| -> Result<_, InternalError> {
                let handle: Handle = handle.try_into().map_err(anyhow::Error::from)?;
                let sid: DataTrackSid = info.track_sid.try_into().map_err(anyhow::Error::from)?;
                Ok((handle, sid))
            })
            .collect::<Result<HashMap<Handle, DataTrackSid>, _>>()?;
        Ok(SubscriberHandlesEvent { mapping })
    }
}

/// Extracts a [`PublicationsUpdatedEvent`] from a join response.
///
/// This takes ownership of the `data_tracks` vector for each participant
/// (except for the local participant), leaving an empty vector in its place.
///
pub fn event_from_join(
    msg: &mut proto::JoinResponse,
) -> Result<PublicationUpdatesEvent, InternalError> {
    event_from_participant_info(&mut msg.other_participants, None)
}

/// Extracts a [`PublicationsUpdatedEvent`] from a participant update.
///
/// This takes ownership of the `data_tracks` vector for each participant in
/// the update, leaving an empty vector in its place.
///
pub fn event_from_participant_update(
    msg: &mut proto::ParticipantUpdate,
    local_participant_identity: &str,
) -> Result<PublicationUpdatesEvent, InternalError> {
    // TODO: is there a better way to exclude the local participant?
    event_from_participant_info(&mut msg.participants, local_participant_identity.into())
}

/// Extracts a [`PublicationsUpdatedEvent`] from a participant info list.
///
/// Tracks published by the local participant will be filtered out if the local
/// participant identity is set.
///
fn event_from_participant_info(
    msg: &mut Vec<ParticipantInfo>,
    local_participant_identity: Option<&str>,
) -> Result<PublicationUpdatesEvent, InternalError> {
    let updates = msg
        .iter_mut()
        .filter(|participant| {
            local_participant_identity.map_or(true, |identity| participant.identity != identity)
        })
        .map(|participant| -> Result<_, InternalError> {
            Ok((participant.identity.clone(), extract_track_info(participant)?))
        })
        .collect::<Result<HashMap<String, Vec<DataTrackInfo>>, _>>()?;
    Ok(PublicationUpdatesEvent { updates })
}

fn extract_track_info(msg: &mut ParticipantInfo) -> Result<Vec<DataTrackInfo>, InternalError> {
    mem::take(&mut msg.data_tracks)
        .into_iter()
        .map(TryInto::<DataTrackInfo>::try_into)
        .collect::<Result<Vec<_>, InternalError>>()
}

// MARK: - Output event -> protocol

impl From<SubscriptionUpdatedEvent> for proto::UpdateDataSubscription {
    fn from(event: SubscriptionUpdatedEvent) -> Self {
        let update = proto::update_data_subscription::Update {
            track_sid: event.sid.into(),
            subscribe: event.subscribe,
            options: Default::default(), // TODO: pass through options
        };
        Self { updates: vec![update] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_subscriber_handles() {
        let sub_handles = [
            (
                1,
                proto::data_track_subscriber_handles::PublishedDataTrack {
                    track_sid: "DTR_1234".into(),
                    ..Default::default()
                },
            ),
            (
                2,
                proto::data_track_subscriber_handles::PublishedDataTrack {
                    track_sid: "DTR_4567".into(),
                    ..Default::default()
                },
            ),
        ];
        let subscriber_handles =
            proto::DataTrackSubscriberHandles { sub_handles: HashMap::from(sub_handles) };

        let event: SubscriberHandlesEvent = subscriber_handles.try_into().unwrap();
        assert_eq!(
            event.mapping.get(&1u32.try_into().unwrap()).unwrap(),
            &"DTR_1234".to_string().try_into().unwrap()
        );
        assert_eq!(
            event.mapping.get(&2u32.try_into().unwrap()).unwrap(),
            &"DTR_4567".to_string().try_into().unwrap()
        );
    }

    #[test]
    fn test_extract_track_info() {
        let data_tracks = vec![proto::DataTrackInfo {
            pub_handle: 1,
            sid: "DTR_1234".into(),
            name: "track1".into(),
            encryption: proto::encryption::Type::Gcm.into(),
        }];
        let mut participant_info = proto::ParticipantInfo { data_tracks, ..Default::default() };

        let track_info = extract_track_info(&mut participant_info).unwrap();
        assert!(participant_info.data_tracks.is_empty(), "Expected original vec taken");
        assert_eq!(track_info.len(), 1);

        let first = track_info.first().unwrap();
        assert_eq!(first.pub_handle, 1u32.try_into().unwrap());
        assert_eq!(first.name, "track1");
        assert_eq!(first.sid, "DTR_1234".to_string().try_into().unwrap());
    }
}
