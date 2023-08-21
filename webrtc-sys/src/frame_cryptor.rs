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

        type KeyProvider;
 
        fn new_key_provider(
            options: KeyProviderOptions,
        ) -> SharedPtr<KeyProvider>;

        fn set_key(self: &KeyProvider, participant_id: String, key_index: i32, key: Vec<u8>) -> bool;

        fn ratchet_key(self: &KeyProvider, participant_id: String, key_index: i32) -> Vec<u8>;

        fn export_key(self: &KeyProvider, participant_id: String, key_index: i32) -> Vec<u8>;
    }

    unsafe extern "C++" {
        include!("livekit/frame_cryptor.h");
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;

        type FrameCryptor;

        fn new_frame_cryptor(
            participant_id: String,
            algorithm: Algorithm,
            key_provider: SharedPtr<KeyProvider>,
            sender: SharedPtr<RtpSender>,
            receiver: SharedPtr<RtpReceiver>,
        ) -> SharedPtr<FrameCryptor>;

        fn set_enabled(self: &FrameCryptor, enabled: bool);

        fn enabled(self: &FrameCryptor) -> bool;

        fn set_key_index(self: &FrameCryptor, index: i32);

        fn key_index(self: &FrameCryptor) -> i32;

        fn participant_id(self: &FrameCryptor) -> String;
        
        fn register_observer(self: &FrameCryptor, observer: Box<RTCFrameCryptorObserver>);

        fn unregister_observer(self: &FrameCryptor);
    }

    extern "Rust" {
        type RTCFrameCryptorObserver;

        fn on_frame_cryption_state_change(self: &RTCFrameCryptorObserver, participant_id: String,  state: FrameCryptionState);
    }

} // namespace livekit

impl_thread_safety!(ffi::FrameCryptor, Send + Sync);
impl_thread_safety!(ffi::KeyProvider, Send + Sync);