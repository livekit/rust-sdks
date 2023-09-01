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

use std::sync::Arc;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit")]
pub mod ffi {

    #[derive(Debug)]
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

    unsafe extern "C++" {
        include!("livekit/frame_cryptor.h");

        pub type KeyProvider;

        pub fn new_key_provider(options: KeyProviderOptions) -> SharedPtr<KeyProvider>;

        pub fn set_shared_key(self: &KeyProvider, key_index: i32, key: Vec<u8>) -> bool;

        pub fn ratchet_shared_key(self: &KeyProvider, key_index: i32) -> Vec<u8>;

        pub fn export_shared_key(self: &KeyProvider, key_index: i32) -> Vec<u8>;

        pub fn set_key(
            self: &KeyProvider,
            participant_id: String,
            key_index: i32,
            key: Vec<u8>,
        ) -> bool;

        pub fn ratchet_key(self: &KeyProvider, participant_id: String, key_index: i32) -> Vec<u8>;

        pub fn export_key(self: &KeyProvider, participant_id: String, key_index: i32) -> Vec<u8>;
    }

    unsafe extern "C++" {
        include!("livekit/frame_cryptor.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;

        pub type FrameCryptor;

        pub fn new_frame_cryptor_for_rtp_sender(
            participant_id: String,
            algorithm: Algorithm,
            key_provider: SharedPtr<KeyProvider>,
            sender: SharedPtr<RtpSender>,
        ) -> SharedPtr<FrameCryptor>;

        pub fn new_frame_cryptor_for_rtp_receiver(
            participant_id: String,
            algorithm: Algorithm,
            key_provider: SharedPtr<KeyProvider>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<FrameCryptor>;

        pub fn set_enabled(self: &FrameCryptor, enabled: bool);

        pub fn enabled(self: &FrameCryptor) -> bool;

        pub fn set_key_index(self: &FrameCryptor, index: i32);

        pub fn key_index(self: &FrameCryptor) -> i32;

        pub fn participant_id(self: &FrameCryptor) -> String;

        pub fn register_observer(
            self: &FrameCryptor,
            observer: Box<RTCFrameCryptorObserverWrapper>,
        );

        pub fn unregister_observer(self: &FrameCryptor);
    }

    extern "Rust" {
        type RTCFrameCryptorObserverWrapper;

        fn on_frame_cryption_state_change(
            self: &RTCFrameCryptorObserverWrapper,
            participant_id: String,
            state: FrameCryptionState,
        );
    }
} // namespace livekit

impl_thread_safety!(ffi::FrameCryptor, Send + Sync);
impl_thread_safety!(ffi::KeyProvider, Send + Sync);

use ffi::FrameCryptionState;

pub trait RTCFrameCryptorObserver: Send + Sync {
    fn on_frame_cryption_state_change(&self, participant_id: String, state: FrameCryptionState);
}

pub struct RTCFrameCryptorObserverWrapper {
    observer: Arc<dyn RTCFrameCryptorObserver>,
}

impl RTCFrameCryptorObserverWrapper {
    pub fn new(observer: Arc<dyn RTCFrameCryptorObserver>) -> Self {
        Self { observer }
    }

    fn on_frame_cryption_state_change(
        self: &RTCFrameCryptorObserverWrapper,
        participant_id: String,
        state: FrameCryptionState,
    ) {
        self.observer
            .on_frame_cryption_state_change(participant_id, state);
    }
}
