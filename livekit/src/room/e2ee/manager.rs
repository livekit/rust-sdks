// Copyright 2023 LiveKit, Inc.
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

use super::key_provider::KeyProvider;
use super::{E2eeState, EncryptionType};
use crate::participant::{LocalParticipant, RemoteParticipant};
use crate::prelude::{LocalTrack, LocalTrackPublication, RemoteTrack, RemoteTrackPublication};
use crate::publication::TrackPublication;
use crate::{e2ee::E2eeOptions, participant::Participant};
use livekit_webrtc::frame_cryptor::{Algorithm, FrameCryptor};
use livekit_webrtc::{rtp_receiver::RtpReceiver, rtp_sender::RtpSender};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

type StateChangedHandler = Box<dyn Fn(Participant, TrackPublication, E2eeState) + Send>;

struct ManagerInner {
    options: Option<E2eeOptions>, // If Some, it means the e2ee was initialized
    enabled: bool,                // Used to enable/disable e2ee
    frame_cryptors: HashMap<String, FrameCryptor>,
}

#[derive(Clone)]
pub struct E2eeManager {
    inner: Arc<Mutex<ManagerInner>>,
    state_changed: Arc<Mutex<Option<StateChangedHandler>>>,
}

impl E2eeManager {
    /// E2eeOptions is an optional parameter. We may support to reconfigure e2ee after connect in the future.
    pub(crate) fn new(options: Option<E2eeOptions>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ManagerInner {
                enabled: options.is_some(), // Enabled by default if options is provided
                options,
                frame_cryptors: HashMap::new(),
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
    }

    /// Register to e2ee state changes
    /// Used by the room to dispatch the event to the room dispatcher
    pub(crate) fn on_state_changed(
        &self,
        handler: impl Fn(Participant, TrackPublication, E2eeState) + Send + 'static,
    ) {
        *self.state_changed.lock() = Some(Box::new(handler));
    }

    pub(crate) fn initialized(&self) -> bool {
        self.inner.lock().options.is_some()
    }

    /// Called by the room
    pub(crate) fn on_track_subscribed(
        &self,
        track: RemoteTrack,
        publication: RemoteTrackPublication,
        participant: RemoteParticipant,
    ) {
        if !self.initialized() {
            return;
        }

        let receiver = track.transceiver().unwrap().receiver();
        let frame_cryptor = self.setup_rtp_receiver(publication.sid().to_string(), receiver);

        let mut inner = self.inner.lock();
        inner
            .frame_cryptors
            .insert(publication.sid().to_string(), frame_cryptor.clone());
    }

    /// Called by the room
    pub(crate) fn on_local_track_published(
        &self,
        publication: LocalTrackPublication,
        track: LocalTrack,
        participant: LocalParticipant,
    ) {
        if !self.initialized() {
            return;
        }

        let sender = track.transceiver().unwrap().sender();
        let frame_cryptor = self.setup_rtp_sender(publication.sid().to_string(), sender);

        let mut inner = self.inner.lock();
        inner
            .frame_cryptors
            .insert(publication.sid().to_string(), frame_cryptor.clone());
    }

    /// Called by the room
    pub(crate) fn on_local_track_unpublished(
        &self,
        publication: LocalTrackPublication,
        _: LocalParticipant,
    ) {
        self.remove_frame_cryptor(publication.sid().as_str());
    }

    /// Called by the room
    pub(crate) fn on_track_unsubscribed(
        &self,
        _: RemoteTrack,
        publication: RemoteTrackPublication,
        _: RemoteParticipant,
    ) {
        self.remove_frame_cryptor(publication.sid().as_str());
    }

    pub fn frame_cryptors(&self) -> HashMap<String, FrameCryptor> {
        self.inner.lock().frame_cryptors.clone()
    }

    pub fn enabled(&self) -> bool {
        self.inner.lock().enabled && self.initialized()
    }

    pub fn set_enabled(&self, enabled: bool) {
        let inner = self.inner.lock();
        if inner.enabled == enabled {
            return;
        }

        for (_, cryptor) in inner.frame_cryptors.iter() {
            cryptor.set_enabled(enabled);
        }
    }

    pub fn key_provider(&self) -> Option<KeyProvider> {
        let inner = self.inner.lock();
        inner.options.as_ref().map(|opts| opts.key_provider.clone())
    }

    pub fn encryption_type(&self) -> EncryptionType {
        let inner = self.inner.lock();
        inner
            .options
            .as_ref()
            .map(|opts| opts.encryption_type)
            .unwrap_or(EncryptionType::None)
    }

    fn setup_rtp_sender(&self, participant_id: String, sender: RtpSender) -> FrameCryptor {
        let inner = self.inner.lock();
        let options = inner.options.as_ref().unwrap();

        let frame_cryptor = FrameCryptor::new_for_rtp_sender(
            participant_id,
            Algorithm::AesGcm,
            options.key_provider.handle.clone(),
            sender,
        );
        frame_cryptor.set_enabled(inner.enabled);
        frame_cryptor
    }

    fn setup_rtp_receiver(&self, participant_id: String, receiver: RtpReceiver) -> FrameCryptor {
        let inner = self.inner.lock();
        let options = inner.options.as_ref().unwrap();

        let frame_cryptor = FrameCryptor::new_for_rtp_receiver(
            participant_id,
            Algorithm::AesGcm,
            options.key_provider.handle.clone(),
            receiver,
        );
        frame_cryptor.set_enabled(inner.enabled);
        frame_cryptor
    }

    fn remove_frame_cryptor(&self, sid: &str) {
        let mut inner = self.inner.lock();

        inner
            .frame_cryptors
            .retain(|participant_id, _| !participant_id.contains(sid));
    }
}
