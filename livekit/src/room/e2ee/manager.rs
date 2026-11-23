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

use std::{collections::HashMap, sync::Arc};

use libwebrtc::{
    native::{
        frame_cryptor::{
            DataPacketCryptor, EncryptedPacket, EncryptionAlgorithm, EncryptionState, FrameCryptor,
            KeyProvider as NativeKeyProvider,
        },
        packet_trailer,
    },
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};
use livekit_protocol::PacketTrailerFeature;
use parking_lot::Mutex;

use super::{key_provider::KeyProvider, EncryptionType};
use crate::{
    e2ee::E2eeOptions,
    id::{ParticipantIdentity, TrackSid},
    participant::{LocalParticipant, RemoteParticipant},
    prelude::{LocalTrack, LocalTrackPublication, RemoteTrack, RemoteTrackPublication},
    rtc_engine::lk_runtime::LkRuntime,
};
use std::fmt::Debug;

type StateChangedHandler = Box<dyn Fn(ParticipantIdentity, EncryptionState) + Send>;

fn has_packet_trailer_features(features: &[i32]) -> bool {
    features.iter().any(|f| {
        *f == PacketTrailerFeature::PtfUserTimestamp as i32
            || *f == PacketTrailerFeature::PtfFrameId as i32
            || *f == PacketTrailerFeature::PtfUserData as i32
    })
}

fn needs_video_receiver_packet_trailer_handler(features: &[i32]) -> bool {
    has_packet_trailer_features(features)
}

struct ManagerInner {
    options: Option<E2eeOptions>, // If Some, it means the e2ee was initialized
    enabled: bool,                // Used to enable/disable e2ee
    dc_encryption_enabled: bool,
    frame_cryptors: HashMap<(ParticipantIdentity, TrackSid), FrameCryptor>,
    /// By receiver track id, the cryptors set on receivers as they are created.
    receivers: HashMap<String, ReceiverCryptor>,
    data_packet_cryptor: Option<DataPacketCryptor>,
}

impl Drop for ManagerInner {
    /// A join dropped before its session took the receivers over, such as a cancelled
    /// `Room::connect`, still frees them in a key provider the app may keep.
    fn drop(&mut self) {
        if let Some(options) = self.options.as_ref() {
            for receiver in self.receivers.values() {
                options.key_provider.handle.remove_receiver(receiver.key_id.clone());
            }
        }
    }
}

#[derive(Clone)]
pub struct E2eeManager {
    inner: Arc<Mutex<ManagerInner>>,
    state_changed: Arc<Mutex<Option<StateChangedHandler>>>,
}

/// A shared-key cryptor set on a receiver as soon as the receiver exists.
struct ReceiverCryptor {
    cryptor: FrameCryptor,
    /// The cryptor's own key-handler id.
    key_id: String,
    /// The participant whose track the receiver carries, for state changes.
    owner: Arc<Mutex<Option<ParticipantIdentity>>>,
    /// False while the receiver carries a track sent in the clear. A receiver that carries no
    /// track yet decrypts, so an encrypted track's first frames are decrypted.
    encrypted: bool,
    /// A clear track's packet-trailer handler replaced the cryptor as the receiver's frame
    /// transformer, so the cryptor no longer sees the receiver's frames.
    detached: bool,
}

impl ReceiverCryptor {
    /// While the receiver carries an encrypted track, it decrypts with its sender's own key
    /// handler, as a cryptor made for the sender would: that handler holds the ratchets its
    /// other tracks and its data messages followed, and those follow this track's. Otherwise
    /// it is back on its own handler, so no other sender's handler is moved.
    fn assign(
        &mut self,
        keys: &NativeKeyProvider,
        owner: Option<ParticipantIdentity>,
        encrypted: bool,
        enabled: bool,
    ) {
        let target = owner.as_ref().filter(|_| encrypted).map(|owner| owner.to_string());
        keys.bind_receiver(self.key_id.clone(), target.unwrap_or_default());
        *self.owner.lock() = owner;
        self.encrypted = encrypted;
        self.cryptor.set_enabled(enabled && encrypted);
    }
}

impl E2eeManager {
    /// E2eeOptions is an optional parameter. We may support to reconfigure e2ee after connect in
    /// the future.
    pub(crate) fn new(options: Option<E2eeOptions>, with_dc_encryption: bool) -> Self {
        // Create DataPacketCryptor whenever E2EE options are available
        // This allows for decryption even if we're not encrypting our own data
        let data_packet_cryptor = options.as_ref().map(|opts| {
            DataPacketCryptor::new(EncryptionAlgorithm::AesGcm, opts.key_provider.handle.clone())
        });

        Self {
            inner: Arc::new(Mutex::new(ManagerInner {
                enabled: options.is_some(), // Enabled by default if options is provided
                dc_encryption_enabled: options.is_some() && with_dc_encryption,
                options,
                frame_cryptors: HashMap::new(),
                receivers: HashMap::new(),
                data_packet_cryptor,
            })),
            state_changed: Default::default(),
        }
    }

    pub(crate) fn cleanup(&self) {
        let mut inner = self.inner.lock();
        for cryptor in inner.frame_cryptors.values() {
            cryptor.set_enabled(false);
        }
        inner.frame_cryptors.clear();
        drop(inner);
        self.forget_receivers();
    }

    /// Drops the cryptors of a previous session's receivers.
    pub(crate) fn forget_receivers(&self) {
        let mut inner = self.inner.lock();
        let receivers = std::mem::take(&mut inner.receivers);
        for receiver in receivers.into_values() {
            receiver.cryptor.set_enabled(false);
            // A frame already past the enabled check finds no key: it reports for no one.
            *receiver.owner.lock() = None;
            if let Some(options) = inner.options.as_ref() {
                options.key_provider.handle.remove_receiver(receiver.key_id);
            }
        }
    }

    /// Attaches a shared-key cryptor to a new `receiver`, so its first frames are decrypted:
    /// a cryptor attached on `TrackSubscribed` misses the frames the server sent before it.
    /// With per-participant keys the sender isn't known yet, and the cryptor is made on
    /// subscribe as before.
    pub(crate) fn prepare_receiver(&self, receiver: RtpReceiver) {
        let mut inner = self.inner.lock();
        let Some(options) = inner.options.as_ref() else { return };
        let Some(track) = receiver.track() else { return };
        if !options.key_provider.shared {
            return;
        }
        // Its own key handler, as it isn't known whose track the receiver will carry: the
        // native provider keeps ratcheted keys per handler, so one sender's ratchet must not move
        // the key other senders' receivers use. Shared-key sets and ratchets still reach it.
        // Once its sender is known, it decrypts with the sender's handler (`assign`). The track
        // id is locally generated, so a remote publisher can't choose it.
        let key_id = format!("\u{1f}receiver:{}", track.id());
        options.key_provider.handle.add_receiver(key_id.clone());
        let owner = Arc::new(Mutex::new(None));
        let frame_cryptor = self.receiver_cryptor(
            &options.key_provider.handle,
            inner.enabled,
            &key_id,
            &owner,
            receiver,
        );
        inner.receivers.insert(
            track.id(),
            ReceiverCryptor {
                cryptor: frame_cryptor,
                key_id,
                owner,
                encrypted: true,
                detached: false,
            },
        );
    }

    /// A cryptor for `receiver` on key handler `key_id`, reporting state changes for `owner`.
    fn receiver_cryptor(
        &self,
        keys: &NativeKeyProvider,
        enabled: bool,
        key_id: &str,
        owner: &Arc<Mutex<Option<ParticipantIdentity>>>,
        receiver: RtpReceiver,
    ) -> FrameCryptor {
        let frame_cryptor = FrameCryptor::new_for_rtp_receiver(
            LkRuntime::instance().pc_factory(),
            key_id.to_owned(),
            EncryptionAlgorithm::AesGcm,
            keys.clone(),
            receiver,
        );
        frame_cryptor.set_enabled(enabled);
        let owner = owner.clone();
        let state_changed = self.state_changed.clone();
        frame_cryptor.on_state_change(Some(Box::new(move |_, state| {
            let owner = owner.lock().clone();
            if let (Some(owner), Some(state_changed)) = (owner, state_changed.lock().as_ref()) {
                state_changed(owner, state);
            }
        })));
        frame_cryptor
    }

    /// Register to e2ee state changes
    /// Used by the room to dispatch the event to the room dispatcher
    pub(crate) fn on_state_changed(
        &self,
        handler: impl Fn(ParticipantIdentity, EncryptionState) + Send + 'static,
    ) {
        *self.state_changed.lock() = Some(Box::new(handler));
    }

    pub(crate) fn initialized(&self) -> bool {
        self.inner.lock().options.is_some()
    }

    pub(crate) fn on_track_subscribed(
        &self,
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    ) {
        let identity = participant.identity();
        let receiver = track.transceiver().unwrap().receiver();
        let mut packet_trailer_handler = None;

        let needs_packet_trailer_handler = needs_video_receiver_packet_trailer_handler(
            &publication.proto_info().packet_trailer_features,
        );

        if let RemoteTrack::Video(video_track) = &track {
            if needs_packet_trailer_handler {
                let handler = packet_trailer::create_receiver_handler(
                    LkRuntime::instance().pc_factory(),
                    &receiver,
                );
                video_track.set_packet_trailer_handler(handler.clone());
                packet_trailer_handler = Some(handler);
                log::info!(
                    "attached packet_trailer handler for subscribed track {} from {}",
                    publication.sid(),
                    identity,
                );
            } else {
                log::debug!(
                    "skipping packet_trailer handler for subscribed track {} from {} without advertised packet trailer support",
                    publication.sid(),
                    identity,
                );
            }
        }

        if !self.initialized() || publication.encryption_type() == EncryptionType::None {
            // A track sent in the clear: the receiver's cryptor would drop its frames.
            if let Some(track) = receiver.track() {
                let mut inner = self.inner.lock();
                let inner = &mut *inner;
                let keys = inner.options.as_ref().map(|options| &options.key_provider.handle);
                if let (Some(keys), Some(prepared)) = (keys, inner.receivers.get_mut(&track.id())) {
                    prepared.assign(keys, Some(identity), false, inner.enabled);
                    prepared.detached |= packet_trailer_handler.is_some();
                }
            }
            return;
        }

        let prepared = receiver.track().and_then(|track| {
            let mut inner = self.inner.lock();
            let inner = &mut *inner;
            let keys = &inner.options.as_ref()?.key_provider.handle;
            let prepared = inner.receivers.get_mut(&track.id())?;
            if prepared.detached && packet_trailer_handler.is_none() {
                // Nothing would put the cryptor back on the receiver: a new one takes its place,
                // on the same key handler.
                prepared.cryptor.set_enabled(false);
                prepared.cryptor = self.receiver_cryptor(
                    keys,
                    inner.enabled,
                    &prepared.key_id,
                    &prepared.owner,
                    receiver.clone(),
                );
            }
            // Setting the trailer handler below chains the cryptor back onto the receiver.
            prepared.detached = false;
            prepared.assign(keys, Some(identity.clone()), true, inner.enabled);
            Some(prepared.cryptor.clone())
        });
        let frame_cryptor = prepared.unwrap_or_else(|| {
            let frame_cryptor = self.setup_rtp_receiver(&identity, receiver);
            self.setup_cryptor(&frame_cryptor);
            frame_cryptor
        });
        // A reused receiver keeps the previous track's handler when this one has none. It
        // only strips frames that end in the trailer magic, so frames without one pass.
        if let Some(handler) = packet_trailer_handler.as_ref() {
            frame_cryptor.set_packet_trailer_handler(handler);
        }

        let mut inner = self.inner.lock();
        inner.frame_cryptors.insert((identity, publication.sid()), frame_cryptor.clone());
    }

    pub(crate) fn on_local_track_published(
        &self,
        track: LocalTrack,
        publication: LocalTrackPublication,
        participant: LocalParticipant,
    ) {
        let identity = participant.identity();
        let sender = track.transceiver().unwrap().sender();

        let packet_trailer_handler = if let LocalTrack::Video(video_track) = &track {
            let handler = video_track.packet_trailer_handler();
            if handler.is_some() {
                log::info!(
                    "packet_trailer enabled for published track {} from {}",
                    publication.sid(),
                    identity,
                );
            } else {
                log::info!(
                    "packet_trailer not enabled for published track {} from {}",
                    publication.sid(),
                    identity,
                );
            }
            handler
        } else {
            None
        };

        if !self.initialized() || publication.encryption_type() == EncryptionType::None {
            return;
        }

        let frame_cryptor = self.setup_rtp_sender(&identity, sender);
        if let Some(handler) = packet_trailer_handler.as_ref() {
            frame_cryptor.set_packet_trailer_handler(handler);
        }
        self.setup_cryptor(&frame_cryptor);

        let mut inner = self.inner.lock();
        inner.frame_cryptors.insert((identity, publication.sid()), frame_cryptor.clone());
    }

    fn setup_cryptor(&self, frame_cryptor: &FrameCryptor) {
        let state_changed = self.state_changed.clone();
        frame_cryptor.on_state_change(Some(Box::new(move |participant_identity, state| {
            if let Some(state_changed) = state_changed.lock().as_ref() {
                state_changed(participant_identity.into(), state);
            }
        })));
    }

    /// Called by the room
    pub(crate) fn on_local_track_unpublished(
        &self,
        publication: LocalTrackPublication,
        participant: LocalParticipant,
    ) {
        self.remove_frame_cryptor(participant.identity(), publication.sid());
    }

    /// Called by the room
    pub(crate) fn on_track_unsubscribed(
        &self,
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    ) {
        self.remove_frame_cryptor(participant.identity(), publication.sid());
        // The receiver may carry another track next: it decrypts again, for no one yet.
        let mut inner = self.inner.lock();
        let inner = &mut *inner;
        let keys = inner.options.as_ref().map(|options| &options.key_provider.handle);
        let identity = participant.identity();
        if let (Some(keys), Some(prepared)) =
            (keys, inner.receivers.get_mut(&track.rtc_track().id()))
        {
            // Unless the receiver already carries its next track.
            if prepared.owner.lock().as_ref() == Some(&identity) {
                prepared.assign(keys, None, true, inner.enabled);
            }
        }
    }

    pub fn frame_cryptors(&self) -> HashMap<(ParticipantIdentity, TrackSid), FrameCryptor> {
        self.inner.lock().frame_cryptors.clone()
    }

    pub fn enabled(&self) -> bool {
        self.inner.lock().enabled && self.initialized()
    }

    pub fn is_dc_encryption_enabled(&self) -> bool {
        self.inner.lock().dc_encryption_enabled
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut inner = self.inner.lock();
        if inner.enabled == enabled {
            return;
        }

        inner.enabled = enabled;
        for (_, cryptor) in inner.frame_cryptors.iter() {
            cryptor.set_enabled(enabled);
        }
        for receiver in inner.receivers.values() {
            receiver.cryptor.set_enabled(enabled && receiver.encrypted);
        }
    }

    pub fn key_provider(&self) -> Option<KeyProvider> {
        let inner = self.inner.lock();
        inner.options.as_ref().map(|opts| opts.key_provider.clone())
    }

    pub fn encryption_type(&self) -> EncryptionType {
        let inner = self.inner.lock();
        inner.options.as_ref().map(|opts| opts.encryption_type).unwrap_or(EncryptionType::None)
    }

    fn setup_rtp_sender(
        &self,
        participant_identity: &ParticipantIdentity,
        sender: RtpSender,
    ) -> FrameCryptor {
        let inner = self.inner.lock();
        let options = inner.options.as_ref().unwrap();

        let frame_cryptor = FrameCryptor::new_for_rtp_sender(
            LkRuntime::instance().pc_factory(),
            participant_identity.to_string(),
            EncryptionAlgorithm::AesGcm,
            options.key_provider.handle.clone(),
            sender,
        );
        frame_cryptor.set_enabled(inner.enabled);
        frame_cryptor
    }

    fn setup_rtp_receiver(
        &self,
        participant_identity: &ParticipantIdentity,
        receiver: RtpReceiver,
    ) -> FrameCryptor {
        let inner = self.inner.lock();
        let options = inner.options.as_ref().unwrap();

        let frame_cryptor = FrameCryptor::new_for_rtp_receiver(
            LkRuntime::instance().pc_factory(),
            participant_identity.to_string(),
            EncryptionAlgorithm::AesGcm,
            options.key_provider.handle.clone(),
            receiver,
        );
        frame_cryptor.set_enabled(inner.enabled);
        frame_cryptor
    }

    fn remove_frame_cryptor(&self, participant_identity: ParticipantIdentity, track_sid: TrackSid) {
        log::debug!("removing frame cryptor for {}", participant_identity);

        let mut inner = self.inner.lock();
        inner.frame_cryptors.remove(&(participant_identity, track_sid));
    }

    /// Decrypt data received from a data channel
    pub(crate) fn decrypt_data(
        &self,
        data: Vec<u8>,
        iv: Vec<u8>,
        key_index: u32,
        participant_identity: &str,
    ) -> Option<Vec<u8>> {
        let inner = self.inner.lock();

        let data_packet_cryptor = inner.data_packet_cryptor.as_ref()?;

        let encrypted_packet = EncryptedPacket { data, iv, key_index };
        match data_packet_cryptor.decrypt(participant_identity, &encrypted_packet) {
            Ok(decrypted_data) => Some(decrypted_data),
            Err(e) => {
                log::warn!("handle_encrypted_data error: {}", e);
                None
            }
        }
    }

    /// Encrypt data for transmission over a data channel
    pub(crate) fn encrypt_data(
        &self,
        data: Vec<u8>,
        participant_identity: &ParticipantIdentity,
        key_index: u32,
    ) -> Result<EncryptedPacket, Box<dyn std::error::Error>> {
        let inner = self.inner.lock();

        let data_packet_cryptor =
            inner.data_packet_cryptor.as_ref().ok_or("DataPacketCryptor is not initialized")?;

        data_packet_cryptor.encrypt(participant_identity.as_str(), key_index, &data)
    }
}

impl Debug for E2eeManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("E2eeManager")
            .field("enabled", &self.inner.lock().enabled)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_manager_frees_its_receivers() {
        use libwebrtc::prelude::{
            MediaType, RtcConfiguration, RtpTransceiverDirection, RtpTransceiverInit,
        };

        use crate::e2ee::key_provider::KeyProviderOptions;

        let key_provider =
            KeyProvider::with_shared_key(KeyProviderOptions::default(), b"key".to_vec());
        let manager = E2eeManager::new(
            Some(E2eeOptions {
                encryption_type: EncryptionType::Gcm,
                key_provider: key_provider.clone(),
            }),
            false,
        );
        let pc = LkRuntime::instance()
            .pc_factory()
            .create_peer_connection(RtcConfiguration::default())
            .unwrap();
        let init = RtpTransceiverInit {
            direction: RtpTransceiverDirection::RecvOnly,
            stream_ids: Vec::new(),
            send_encodings: Vec::new(),
        };
        let receiver = pc.add_transceiver_for_media(MediaType::Video, init).unwrap().receiver();
        let key_id =
            ParticipantIdentity(format!("\u{1f}receiver:{}", receiver.track().unwrap().id()));
        manager.prepare_receiver(receiver);
        assert!(key_provider.get_key(&key_id, 0).is_some());

        // As when the `Room::connect` future is dropped during signalling.
        drop(manager);
        assert!(key_provider.get_key(&key_id, 0).is_none());
    }

    #[test]
    fn receiver_packet_trailer_handler_is_skipped_without_features() {
        assert!(!needs_video_receiver_packet_trailer_handler(&[]));
    }

    #[test]
    fn receiver_packet_trailer_handler_is_needed_for_user_timestamp() {
        let features = [PacketTrailerFeature::PtfUserTimestamp as i32];

        assert!(needs_video_receiver_packet_trailer_handler(&features));
    }

    #[test]
    fn receiver_packet_trailer_handler_is_needed_for_frame_id() {
        let features = [PacketTrailerFeature::PtfFrameId as i32];

        assert!(needs_video_receiver_packet_trailer_handler(&features));
    }

    #[test]
    fn receiver_packet_trailer_handler_is_needed_for_user_data() {
        let features = [PacketTrailerFeature::PtfUserData as i32];

        assert!(needs_video_receiver_packet_trailer_handler(&features));
    }
}
