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

use livekit::e2ee::{
    frame_cryptor::{FrameCryptor, FrameCryptorOptions},
    key_provider::{new_key_provider, KeyProvider, KeyProviderOptions},
};

#[derive(Clone)]
pub struct E2EEManager {
    options: E2EEOptions,
    frame_cryptors: HashMap<String, SharedPtr<FrameCryptor>>
    room: Room,
    room_events: mpsc::UnboundedReceiver<RoomEvent>,
    event_loop: Option<JoinHandle<()>>,
    enabled: bool,
}

impl E2EEManager {
    pub fn new(options: E2EEOptions) -> Self {
        Self {
            options,
            frame_cryptors: HashMap::new(),
            room: None,
            enabled: false,
        }
    }

    pub fn setup(room: &Room) {

        if options.encryption_type == EncryptionType::None {
            return;
        }

        if(self.room.is_some()) {
            cleanup();
        }

        self.room = room;
        self.room_events = self.room.subscribe();
        self.event_loop = thread::spawn(move || self {
            let rx = self.room_events.recv().unwrap();
            while let Some(msg) = rx.recv().await {
                match msg {
                    RoomEvent::TrackSubscribed {
                        track,
                        publication: _,
                        participant: _,
                    } => {
                        self.add_remote_track(track);
                    },
                    RoomEvent::LocalTrackPublished {
                        publication,
                        track,
                        participant: _,
                    } => {
                        self.add_local_track(track);
                    }
                    _ => {}
                }
            }
        });
    }

    pub fn set_enabled(bool enabled)  {
        self.enabled = enabled;
        for((participant_id, cryptor) in self.frame_cryptors.into_iter()) {
            cryptor.set_enabled(enabled);
            if(self.options.key_provider.shared_key) {
                self.options.key_provider.set_key_index(
                    participant_id,
                    index: 0,
                    self.options.shared_key);
                cryptor.set_key_index(0);
            }
        }
    }

    fn add_local_track(track: LocalTrack) {

    }

    fn add_remote_track(track: RemoteTrack) {

    }

    pub fn cleanup() {
        self.frame_cryptors.clear();
        self.room = None;
        if let Some(event_loop) = self.event_loop.take() {
            event_loop.join().unwrap();
        }
        self.room_events = None;
    }

    pub fn ratchet_key() {
        for((participant_id, _) in self.frame_cryptors.into_iter()) {
            let new_key = self.options.key_provider.ratchet_key(participant_id, 0);
        }
    }

    fn _add_rtp_sender(sid: String? track_id: String, kind: String, sender: RtpSender) -> SharedPtr<FrameCryptor> {
        let participant_id = kind + "-sender-" + sid + "-" + track_id;
        let frame_cryptor = new_frame_cryptor_for_rtp_sender(
            participant_id,
            self.options.key_provider.algorithm,
            self.options.key_provider,
            sender,
        );

        self.frame_cryptors[track_id] = frame_cryptor;

        frame_cryptor.set_enabled(self.enabled);

        if (self.options.key_provider.shared_key) {
            self.options.key_provider.set_key(participant_id, 0, self.options.shared_key);
            frame_cryptor.set_key_index(0);
        }

        return frame_cryptor;
    }

    fn _add_rtp_receiver(sid: String? track_id: String, kind: String, receiver: RtpReceiver) -> SharedPtr<FrameCryptor> {
        let participant_id = kind + "-receiver-" + sid + "-" + track_id;
        let frame_cryptor = new_frame_cryptor_for_rtp_receiver(
            participant_id,
            self.options.key_provider.algorithm,
            self.options.key_provider,
            receiver,
        );
        frame_cryptor.set_enabled(self.enabled);
        if (self.options.key_provider.shared_key) {
            self.options.key_provider.set_key(participant_id, 0, self.options.shared_key);
            frame_cryptor.set_key_index(0);
        }
        self.frame_cryptors[track_id] = frame_cryptor;

        return frame_cryptor;
    }
}