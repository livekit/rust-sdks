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

use livekit_protocol::observer::Dispatcher;
use parking_lot::Mutex;
use std::collections::HashMap;

use livekit_webrtc::{
    frame_cryptor::{Algorithm, FrameCryptionState, FrameCryptor},
    rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

use crate::RoomEvent;
use crate::{e2ee::options::E2EEOptions, participant::Participant};

use crate::prelude::TrackKind;

use super::options::EncryptionType;

pub use crate::publication::TrackPublication;

pub struct E2EEManager {
    dispatcher: Dispatcher<RoomEvent>,
    options: Option<E2EEOptions>,
    frame_cryptors: Mutex<HashMap<String, FrameCryptor>>,
    enabled: bool,
}

impl E2EEManager {
    pub fn new(dispatcher: Dispatcher<RoomEvent>, options: Option<E2EEOptions>) -> Self {
        Self {
            dispatcher,
            frame_cryptors: HashMap::new().into(),
            enabled: options.is_some(),
            options,
        }
    }

    pub fn encryption_type(&self) -> EncryptionType {
        if let Some(options) = &self.options {
            return options.encryption_type;
        }
        EncryptionType::None
    }

    pub fn handle_track_events(&self, event: RoomEvent) {
        if self.options.is_none() {
            return;
        }
        match event {
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                let transceiver = track.transceiver();
                if let Some(transceiver) = transceiver {
                    let fc = self._add_rtp_receiver(
                        track.sid().to_string(),
                        track.rtc_track().id().to_string(),
                        String::from(match track.kind() {
                            TrackKind::Audio => "audio",
                            TrackKind::Video => "video",
                        }),
                        transceiver.receiver(),
                    );

                    if let Some(fc) = fc {
                        let dispatcher = self.dispatcher.clone();
                        fc.on_state_change(Some(Box::new(
                            move |participant_id: String, state: FrameCryptionState| {
                                log::error!(
                                    "frame cryptor state changed for {}, state {:?}",
                                    participant_id,
                                    state
                                );
                                dispatcher.dispatch(&RoomEvent::E2EEStateEvent {
                                    participant: Participant::Remote(participant.clone()),
                                    publication: TrackPublication::Remote(publication.clone()),
                                    state: state.into(),
                                });
                            },
                        )));
                    }
                }
            }
            RoomEvent::LocalTrackPublished {
                publication,
                track,
                participant,
            } => {
                let transceiver = track.transceiver();
                if let Some(transceiver) = transceiver {
                    let fc = self._add_rtp_sender(
                        track.sid().to_string(),
                        track.rtc_track().id().to_string(),
                        String::from(match track.kind() {
                            TrackKind::Audio => "audio",
                            TrackKind::Video => "video",
                        }),
                        transceiver.sender(),
                    );

                    if let Some(fc) = fc {
                        let dispatcher = self.dispatcher.clone();
                        fc.on_state_change(Some(Box::new(
                            move |participant_id: String, state: FrameCryptionState| {
                                log::error!(
                                    "frame cryptor state changed for {}, state {:?}",
                                    participant_id,
                                    state
                                );
                                dispatcher.dispatch(&RoomEvent::E2EEStateEvent {
                                    participant: Participant::Local(participant.clone()),
                                    publication: TrackPublication::Local(publication.clone()),
                                    state: state.into(),
                                });
                            },
                        )));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled && self.options.is_some()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if let Some(options) = &self.options {
            for (participant_id, cryptor) in self.frame_cryptors.lock().iter() {
                cryptor.set_enabled(enabled);
                if options.key_provider.is_shared_key() {
                    options.key_provider.set_key(
                        participant_id.clone(),
                        0,
                        options.key_provider.shared_key().as_bytes().to_vec(),
                    );
                    cryptor.set_key_index(0);
                }
            }
        }
    }

    pub fn cleanup(&mut self) {
        for cryptor in self.frame_cryptors.lock().values() {
            cryptor.set_enabled(false);
        }
        self.options = None;
    }

    pub fn ratchet_key(&mut self) {
        if let Some(options) = &self.options {
            for participant_id in self.frame_cryptors.lock().keys() {
                let new_key = options.key_provider.ratchet_key(participant_id.clone(), 0);
                log::info!(
                    "ratcheting key for {}, new_key {}",
                    participant_id,
                    new_key.len()
                );
            }
        }
    }

    fn _add_rtp_sender(
        &self,
        sid: String,
        track_id: String,
        kind: String,
        sender: RtpSender,
    ) -> Option<FrameCryptor> {
        let participant_id = kind + "-sender-" + &sid + "-" + &track_id;
        log::error!("_add_rtp_sender {} !!!!", participant_id);
        if let Some(options) = &self.options {
            let frame_cryptor = FrameCryptor::new_for_rtp_sender(
                participant_id.clone(),
                Algorithm::AesGcm,
                options.key_provider.handle.clone(),
                sender,
            );

            frame_cryptor.set_enabled(self.enabled);

            if options.key_provider.is_shared_key() {
                options.key_provider.set_key(
                    participant_id.clone(),
                    0,
                    options.key_provider.shared_key().as_bytes().to_vec(),
                );
                frame_cryptor.set_key_index(0);
            }

            let mut frame_cryptors = self.frame_cryptors.lock();
            frame_cryptors.insert(participant_id.clone(), frame_cryptor.clone());
            return Some(frame_cryptor);
        }
        return None;
    }

    fn _add_rtp_receiver(
        &self,
        sid: String,
        track_id: String,
        kind: String,
        receiver: RtpReceiver,
    ) -> Option<FrameCryptor> {
        let participant_id = kind + "-receiver-" + &sid + "-" + &track_id;
        log::error!("_add_rtp_receiver {} !!!!", participant_id);
        if let Some(options) = &self.options {
            let frame_cryptor = FrameCryptor::new_for_rtp_receiver(
                participant_id.clone(),
                Algorithm::AesGcm,
                options.key_provider.handle.clone(),
                receiver,
            );
            frame_cryptor.set_enabled(self.enabled);
            if options.key_provider.is_shared_key() {
                options.key_provider.set_key(
                    participant_id.clone(),
                    0,
                    options.key_provider.shared_key().as_bytes().to_vec(),
                );
                frame_cryptor.set_key_index(0);
            }
            let mut frame_cryptors = self.frame_cryptors.lock();
            frame_cryptors.insert(participant_id.clone(), frame_cryptor.clone());
            return Some(frame_cryptor);
        }
        return None;
    }
}
