//! Thin commands supported by the public core; no signaling or media backend is recreated.
use super::{NativeRoomSession, NativeSessionError};

/// A participant's allowed subscriptions to this room's local tracks.
#[derive(Clone, Debug, uniffi::Record)]
pub struct NativeTrackPermission {
    pub participant_identity: String,
    pub allow_all: bool,
    pub allowed_track_sids: Vec<String>,
}

pub use livekit::track::VideoQuality as NativeVideoQuality;
/// Highest simulcast layer requested from the core; bandwidth may select a lower layer.
#[uniffi::remote(Enum)]
pub enum NativeVideoQuality {
    Low,
    Medium,
    High,
}

#[uniffi::export]
impl NativeRoomSession {
    /// Publishes a DTMF packet through the core's reliable transport.
    /// Core sends DTMF without data E2EE, including in encrypted rooms.
    pub async fn publish_dtmf(
        &self,
        code: u32,
        digit: String,
        destination_identities: Vec<String>,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        self.shared
            .command(async move {
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
            .await?
    }

    /// Updates core's desired subscription permissions. Completion is not an SFU acknowledgement.
    pub async fn set_track_subscription_permissions(
        &self,
        all_participants_allowed: bool,
        permissions: Vec<NativeTrackPermission>,
    ) -> Result<(), NativeSessionError> {
        let room = self.shared.room()?;
        let permissions = permissions
            .into_iter()
            .map(|p| {
                let allowed_track_sids = p
                    .allowed_track_sids
                    .into_iter()
                    .map(|sid| {
                        sid.try_into().map_err(|_| NativeSessionError::InvalidState {
                            reason: "invalid allowed track SID".into(),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(livekit::participant::ParticipantTrackPermission {
                    participant_identity: p.participant_identity.into(),
                    allow_all: p.allow_all,
                    allowed_track_sids,
                })
            })
            .collect::<Result<Vec<_>, NativeSessionError>>()?;
        self.shared
            .command(async move {
                room.local_participant()
                    .set_track_subscription_permissions(all_participants_allowed, permissions)
                    .await
                    .map_err(Into::into)
            })
            .await?
    }
}
