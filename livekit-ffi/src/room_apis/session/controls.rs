//! Thin commands supported by the public core; no signaling or media backend is recreated.
use super::{FfiRoom, NativeSessionError};

use livekit::id::{ParticipantIdentity, TrackSid};
pub use livekit::participant::ParticipantTrackPermission;

/// Core subscription permissions; SID validation uses the core newtype converter.
#[uniffi::remote(Record)]
pub struct ParticipantTrackPermission {
    pub participant_identity: ParticipantIdentity,
    pub allow_all: bool,
    pub allowed_track_sids: Vec<TrackSid>,
}

pub use livekit::track::VideoQuality;
/// Highest simulcast layer requested from the core; bandwidth may select a lower layer.
#[uniffi::remote(Enum)]
pub enum VideoQuality {
    Low,
    Medium,
    High,
}

#[uniffi::export]
impl FfiRoom {
    /// Publishes a DTMF packet through the core's reliable transport.
    /// Core sends DTMF without data E2EE, including in encrypted rooms.
    pub async fn publish_dtmf(
        &self,
        code: u32,
        digit: String,
        destination_identities: Vec<String>,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        super::on_runtime(async move {
            room.local_participant()
                .publish_dtmf(livekit::SipDTMF {
                    code,
                    digit,
                    destination_identities: destination_identities
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                })
                .await
                .map_err(Into::into)
        })
        .await
    }

    /// Updates core's desired subscription permissions. Completion is not an SFU acknowledgement.
    pub async fn set_track_subscription_permissions(
        &self,
        all_participants_allowed: bool,
        permissions: Vec<ParticipantTrackPermission>,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        super::on_runtime(async move {
            room.local_participant()
                .set_track_subscription_permissions(all_participants_allowed, permissions)
                .await
                .map_err(Into::into)
        })
        .await
    }
}
