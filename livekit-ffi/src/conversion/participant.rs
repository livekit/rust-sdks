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

use crate::{proto, server::participant::FfiParticipant};
use livekit::prelude::*;
use livekit::DisconnectReason;
use livekit::ParticipantKind;

impl From<&FfiParticipant> for proto::ParticipantInfo {
    fn from(value: &FfiParticipant) -> Self {
        From::<&Participant>::from(&value.participant)
    }
}

impl From<&Participant> for proto::ParticipantInfo {
    fn from(participant: &Participant) -> Self {
        Self {
            sid: participant.sid().into(),
            name: participant.name(),
            identity: participant.identity().into(),
            metadata: participant.metadata(),
            attributes: participant.attributes(),
            kind: proto::ParticipantKind::from(participant.kind()).into(),
            disconnect_reason: proto::DisconnectReason::from(participant.disconnect_reason())
                .into(),
        }
    }
}

impl From<ParticipantKind> for proto::ParticipantKind {
    fn from(kind: ParticipantKind) -> Self {
        match kind {
            ParticipantKind::Standard => proto::ParticipantKind::Standard,
            ParticipantKind::Sip => proto::ParticipantKind::Sip,
            ParticipantKind::Ingress => proto::ParticipantKind::Ingress,
            ParticipantKind::Egress => proto::ParticipantKind::Egress,
            ParticipantKind::Agent => proto::ParticipantKind::Agent,
        }
    }
}

impl From<DisconnectReason> for proto::DisconnectReason {
    fn from(reason: DisconnectReason) -> Self {
        match reason {
            DisconnectReason::UnknownReason => proto::DisconnectReason::UnknownReason,
            DisconnectReason::ClientInitiated => proto::DisconnectReason::ClientInitiated,
            DisconnectReason::DuplicateIdentity => proto::DisconnectReason::DuplicateIdentity,
            DisconnectReason::ServerShutdown => proto::DisconnectReason::ServerShutdown,
            DisconnectReason::ParticipantRemoved => proto::DisconnectReason::ParticipantRemoved,
            DisconnectReason::RoomDeleted => proto::DisconnectReason::RoomDeleted,
            DisconnectReason::StateMismatch => proto::DisconnectReason::StateMismatch,
            DisconnectReason::JoinFailure => proto::DisconnectReason::JoinFailure,
            DisconnectReason::Migration => proto::DisconnectReason::Migration,
            DisconnectReason::SignalClose => proto::DisconnectReason::SignalClose,
            DisconnectReason::RoomClosed => proto::DisconnectReason::RoomClosed,
            DisconnectReason::UserUnavailable => proto::DisconnectReason::UserUnavailable,
            DisconnectReason::UserRejected => proto::DisconnectReason::UserRejected,
            DisconnectReason::SipTrunkFailure => proto::DisconnectReason::SipTrunkFailure,
            DisconnectReason::ConnectionTimeout => proto::DisconnectReason::ConnectionTimeout,
            DisconnectReason::MediaFailure => proto::DisconnectReason::MediaFailure,
        }
    }
}
