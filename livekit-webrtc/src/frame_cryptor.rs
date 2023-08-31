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

use crate::{imp::frame_cryptor as fc_imp, rtp_sender::RtpSender, prelude::RtpReceiver};


#[derive(Debug, Clone)]
pub struct KeyProviderOptions {
    pub shared_key: bool,
    pub ratchet_window_size: i32,
    pub ratchet_salt: Vec<u8>,
    pub uncrypted_magic_bytes: Vec<u8>,
}

#[derive(Debug)]
#[repr(i32)]
pub enum Algorithm {
    AesGcm = 0,
    AesCbc,
}

#[derive(Debug)]
#[repr(i32)]
pub enum FrameCryptionState {
    New = 0,
    Ok,
    EncryptionFailed,
    DecryptionFailed,
    MissingKey,
    KeyRatcheted,
    InternalError,
}

#[derive(Clone)]
pub struct KeyProvider {
    pub(crate) handle: fc_imp::KeyProvider,
}

impl KeyProvider {

    pub fn new(options: KeyProviderOptions) -> Self {
        Self {
            handle: fc_imp::KeyProvider::new(options),
        }
    }

    pub fn set_shared_key(&self, key_index: i32, key: Vec<u8>) -> bool {
        return self.handle.set_shared_key(key_index, key)
    }

    pub fn set_key(&self, participant_id: String, key_index: i32, key: Vec<u8>) -> bool {
        self.handle.set_key(participant_id, key_index, key)
    }

    pub fn ratchet_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.ratchet_key(participant_id, key_index)
    }

    pub fn export_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        self.handle.export_key(participant_id, key_index)
    }
}

#[derive(Clone)]
pub struct FrameCryptor {
    pub(crate) handle: fc_imp::FrameCryptor,
}

pub type OnStateChange = Box<dyn FnMut(String, FrameCryptionState) + Send + Sync>;

impl FrameCryptor {
    pub fn new_for_rtp_sender(
        participant_id: String,
        algorithm: Algorithm,
        key_provider: KeyProvider,
        sender: RtpSender,
    ) -> Self {
        Self {
            handle: fc_imp::FrameCryptor::new_for_rtp_sender(
                participant_id,
                algorithm,
                key_provider.handle,
                sender.handle,
            ),
        }
    }

    pub fn new_for_rtp_receiver(
        participant_id: String,
        algorithm: Algorithm,
        key_provider: KeyProvider,
        receiver: RtpReceiver,
    ) -> Self {
        Self {
            handle: fc_imp::FrameCryptor::new_for_rtp_receiver(
                participant_id,
                algorithm,
                key_provider.handle,
                receiver.handle,
            ),
        }
    }

    pub fn set_enabled(self: &FrameCryptor, enabled: bool) {
        self.handle.set_enabled(enabled)
    }

    pub fn enabled(self: &FrameCryptor) -> bool {
        self.handle.enabled()
    }

    pub fn set_key_index(self: &FrameCryptor, index: i32) {
        self.handle.set_key_index(index)
    }

    pub fn key_index(self: &FrameCryptor) -> i32 {
        self.handle.key_index()
    }

    pub fn participant_id(self: &FrameCryptor) -> String {
        self.handle.participant_id()
    }
    
    pub fn on_state_change(&self, callback: Option<OnStateChange>) {
        self.handle.on_state_change(callback)
    }
    
}