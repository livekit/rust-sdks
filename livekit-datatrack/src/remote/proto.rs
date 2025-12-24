use super::manager::{PublicationsUpdatedEvent, SubscriberHandlesEvent, SubscriptionUpdatedEvent};
use crate::{dtp::TrackHandle, DataTrackInfo, InternalError};
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
                let handle = TrackHandle::try_from(handle).map_err(anyhow::Error::from)?;
                Ok((handle, info.track_sid))
            })
            .collect::<Result<HashMap<TrackHandle, String>, _>>()?;
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
) -> Result<PublicationsUpdatedEvent, InternalError> {
    event_from_participant_info(&mut msg.other_participants)
}

/// Extracts a [`PublicationsUpdatedEvent`] from a participant update.
///
/// This takes ownership of the `data_tracks` vector for each participant in
/// the update, leaving an empty vector in its place.
///
pub fn event_from_participant_update(
    msg: &mut proto::ParticipantUpdate,
) -> Result<PublicationsUpdatedEvent, InternalError> {
    event_from_participant_info(&mut msg.participants)
}

fn event_from_participant_info(
    msg: &mut Vec<ParticipantInfo>,
) -> Result<PublicationsUpdatedEvent, InternalError> {
    let tracks_by_participant = msg
        .iter_mut()
        .map(|participant| -> Result<_, InternalError> {
            Ok((participant.identity.clone(), extract_track_info(participant)?))
        })
        .collect::<Result<HashMap<String, Vec<DataTrackInfo>>, _>>()?;
    Ok(PublicationsUpdatedEvent { tracks_by_participant })
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
            track_sid: event.track_sid,
            subscribe: event.subscribe,
            options: Default::default(), // TODO: pass through options
        };
        Self { updates: vec![update] }
    }
}
