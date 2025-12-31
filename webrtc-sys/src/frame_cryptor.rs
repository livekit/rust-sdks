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

use std::sync::Arc;

use crate::sys;
use parking_lot::Mutex;

use crate::{
    peer_connection_factory::PeerConnectionFactory, rtp_receiver::RtpReceiver,
    rtp_sender::RtpSender,
};

pub type OnStateChange = Box<dyn FnMut(String, EncryptionState) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct KeyProviderOptions {
    pub shared_key: bool,
    pub ratchet_window_size: i32,
    pub ratchet_salt: Vec<u8>,
    pub failure_tolerance: i32,
}

impl Default for KeyProviderOptions {
    fn default() -> Self {
        Self {
            shared_key: true,
            ratchet_window_size: 10,
            ratchet_salt: vec![],
            failure_tolerance: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    AesGcm,
    AesCbc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionState {
    New,
    Ok,
    EncryptionFailed,
    DecryptionFailed,
    MissingKey,
    KeyRatcheted,
    InternalError,
}

#[derive(Debug, Clone)]
pub struct EncryptedPacket {
    pub data: Vec<u8>,
    pub iv: Vec<u8>,
    pub key_index: u32,
}

#[derive(Clone)]
pub struct KeyProvider {
    pub(crate) ffi: sys::RefCounted<sys::lkKeyProvider>,
}

impl KeyProvider {
    pub fn new(options: KeyProviderOptions) -> Self {
        unsafe {
            let lk_options_ptr = sys::lkKeyProviderOptionsCreate();
            sys::lkKeyProviderOptionsSetSharedKey(lk_options_ptr, options.shared_key);
            sys::lkKeyProviderOptionsSetRatchetWindowSize(
                lk_options_ptr,
                options.ratchet_window_size,
            );
            sys::lkKeyProviderOptionsSetRatchetSalt(
                lk_options_ptr,
                options.ratchet_salt.as_ptr(),
                options.ratchet_salt.len() as u32,
            );
            sys::lkKeyProviderOptionsSetFailureTolerance(lk_options_ptr, options.failure_tolerance);

            let ffi = sys::lkKeyProviderCreate(lk_options_ptr);

            let _ = sys::RefCounted::from_raw(lk_options_ptr);
            Self { ffi: sys::RefCounted::from_raw(ffi) }
        }
    }

    pub fn set_shared_key(&self, key_index: i32, key: Vec<u8>) -> bool {
        unsafe {
            sys::lkKeyProviderSetSharedKey(
                self.ffi.as_ptr(),
                key_index,
                key.as_ptr(),
                key.len() as u32,
            )
        }
    }

    pub fn ratchet_shared_key(&self, key_index: i32) -> Option<Vec<u8>> {
        unsafe {
            let key = sys::lkKeyProviderRatchetSharedKey(self.ffi.as_ptr(), key_index);
            if key.is_null() {
                None
            } else {
                let key = sys::RefCountedData::from_native(key);
                Some(key.as_bytes())
            }
        }
    }

    pub fn get_shared_key(&self, key_index: i32) -> Option<Vec<u8>> {
        unsafe {
            let key = sys::lkKeyProviderGetSharedKey(self.ffi.as_ptr(), key_index);
            if key.is_null() {
                None
            } else {
                let key = sys::RefCountedData::from_native(key);
                Some(key.as_bytes())
            }
        }
    }

    pub fn set_key(&self, participant_id: String, key_index: i32, key: Vec<u8>) -> bool {
        unsafe {
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            sys::lkKeyProviderSetKey(
                self.ffi.as_ptr(),
                c_str.as_ptr() as *const u8,
                key_index,
                key.as_ptr(),
                key.len().try_into().unwrap(),
            )
        }
    }

    pub fn ratchet_key(&self, participant_id: String, key_index: i32) -> Option<Vec<u8>> {
        unsafe {
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let key = sys::lkKeyProviderRatchetKey(
                self.ffi.as_ptr(),
                c_str.as_ptr() as *const u8,
                key_index,
            );
            if key.is_null() {
                None
            } else {
                let key = sys::RefCountedData::from_native(key);
                Some(key.as_bytes())
            }
        }
    }

    pub fn get_key(&self, participant_id: String, key_index: i32) -> Option<Vec<u8>> {
        unsafe {
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let key =
                sys::lkKeyProviderGetKey(self.ffi.as_ptr(), c_str.as_ptr() as *const u8, key_index);
            if key.is_null() {
                None
            } else {
                let key = sys::RefCountedData::from_native(key);
                Some(key.as_bytes())
            }
        }
    }

    pub fn set_sif_trailer(&self, trailer: Vec<u8>) {
        unsafe {
            sys::lkKeyProviderSetSifTrailer(
                self.ffi.as_ptr(),
                trailer.as_ptr(),
                trailer.len() as u32,
            );
        }
    }
}

#[derive(Clone)]
pub struct FrameCryptor {
    observer: Arc<RtcFrameCryptorObserver>,
    pub(crate) ffi: sys::RefCounted<sys::lkFrameCryptor>,
}

impl FrameCryptor {
    pub extern "C" fn on_encryption_state_changed(
        participant_id: *const ::std::os::raw::c_char,
        state: sys::lkEncryptionState,
        userdata: *mut ::std::os::raw::c_void,
    ) {
        let observer = unsafe { &*(userdata as *const Arc<RtcFrameCryptorObserver>) };
        let str: String =
            unsafe { std::ffi::CStr::from_ptr(participant_id).to_str().unwrap().to_string() };
        observer.on_frame_cryption_state_change(str, state.into());
    }

    pub fn new_for_rtp_sender(
        peer_factory: &PeerConnectionFactory,
        participant_id: String,
        algorithm: EncryptionAlgorithm,
        key_provider: KeyProvider,
        sender: RtpSender,
    ) -> Self {
        unsafe {
            let observer = Arc::new(RtcFrameCryptorObserver::default());
            let observer_box: *mut Arc<RtcFrameCryptorObserver> =
                Box::into_raw(Box::new(observer.clone()));
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let ffi = sys::lkNewFrameCryptorForRtpSender(
                peer_factory.ffi.as_ptr(),
                c_str.as_ptr() as *const u8,
                algorithm.into(),
                key_provider.ffi.as_ptr(),
                sender.ffi.as_ptr(),
                Some(FrameCryptor::on_encryption_state_changed),
                observer_box as *mut ::std::os::raw::c_void,
            );
            Self { observer: observer, ffi: sys::RefCounted::from_raw(ffi) }
        }
    }

    pub fn new_for_rtp_receiver(
        peer_factory: &PeerConnectionFactory,
        participant_id: String,
        algorithm: EncryptionAlgorithm,
        key_provider: KeyProvider,
        receiver: RtpReceiver,
    ) -> Self {
        unsafe {
            let observer = Arc::new(RtcFrameCryptorObserver::default());
            let observer_box: *mut Arc<RtcFrameCryptorObserver> =
                Box::into_raw(Box::new(observer.clone()));
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let ffi = sys::lkNewFrameCryptorForRtpReceiver(
                peer_factory.ffi.as_ptr(),
                c_str.as_ptr() as *const u8,
                algorithm.into(),
                key_provider.ffi.as_ptr(),
                receiver.ffi.as_ptr(),
                Some(FrameCryptor::on_encryption_state_changed),
                observer_box as *mut ::std::os::raw::c_void,
            );
            Self { observer: observer, ffi: sys::RefCounted::from_raw(ffi) }
        }
    }

    pub fn set_enabled(self: &FrameCryptor, enabled: bool) {
        unsafe {
            sys::lkFrameCryptorSetEnabled(self.ffi.as_ptr(), enabled);
        }
    }

    pub fn enabled(self: &FrameCryptor) -> bool {
        unsafe { sys::lkFrameCryptorGetEnabled(self.ffi.as_ptr()) }
    }

    pub fn set_key_index(self: &FrameCryptor, index: i32) {
        unsafe {
            sys::lkFrameCryptorSetKeyIndex(self.ffi.as_ptr(), index);
        }
    }

    pub fn key_index(self: &FrameCryptor) -> i32 {
        unsafe { sys::lkFrameCryptorGetKeyIndex(self.ffi.as_ptr()) }
    }

    pub fn participant_id(self: &FrameCryptor) -> String {
        unsafe {
            let str_ptr = sys::lkFrameCryptorGetParticipantId(self.ffi.as_ptr());
            let ref_counted_str = sys::RefCountedString { ffi: sys::RefCounted::from_raw(str_ptr) };
            ref_counted_str.as_str()
        }
    }

    pub fn on_state_change(&self, handler: Option<OnStateChange>) {
        *self.observer.state_change_handler.lock() = handler;
    }
}

#[derive(Clone)]
pub struct DataPacketCryptor {
    pub(crate) ffi: sys::RefCounted<sys::lkDataPacketCryptor>,
}

impl DataPacketCryptor {
    pub fn new(algorithm: EncryptionAlgorithm, key_provider: KeyProvider) -> Self {
        unsafe {
            let ffi = sys::lkNewDataPacketCryptor(algorithm.into(), key_provider.ffi.as_ptr());
            Self { ffi: sys::RefCounted::from_raw(ffi) }
        }
    }

    pub fn encrypt(
        &self,
        participant_id: &str,
        key_index: u32,
        data: &[u8],
    ) -> Result<EncryptedPacket, Box<dyn std::error::Error>> {
        unsafe {
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let data_vec: Vec<u8> = data.to_vec();
            let mut rtc_err = sys::lkRtcError { message: std::ptr::null() };
            let encrypted_packet = sys::lkDataPacketCryptorEncrypt(
                self.ffi.as_ptr(),
                c_str.as_ptr() as *const i8,
                key_index,
                data_vec.as_ptr() as *const i8,
                data_vec.len() as u32,
                &mut rtc_err, // Assuming the sixth argument is a mutable pointer, adjust as needed
            );

            if encrypted_packet.is_null() {
                return Err(format!("Decryption failed: {:?}", rtc_err).into());
            }

            let encrypted_data =
                sys::RefCountedData::from_native(sys::lkEncryptedPacketGetData(encrypted_packet));
            let iv =
                sys::RefCountedData::from_native(sys::lkEncryptedPacketGetIv(encrypted_packet));
            let key_index = sys::lkEncryptedPacketGetKeyIndex(encrypted_packet);

            let result =
                EncryptedPacket { data: encrypted_data.as_bytes(), iv: iv.as_bytes(), key_index };

            sys::RefCounted::from_raw(encrypted_packet); // Manage the lifetime

            Ok(result)
        }
    }

    pub fn decrypt(
        &self,
        participant_id: &str,
        encrypted_packet: &EncryptedPacket,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        unsafe {
            let c_str = std::ffi::CString::new(participant_id).unwrap();
            let lk_encrypted_data = sys::lkNewlkEncryptedPacket(
                encrypted_packet.data.as_ptr(),
                encrypted_packet.data.len() as u32,
                encrypted_packet.iv.as_ptr(),
                encrypted_packet.iv.len() as u32,
                encrypted_packet.key_index,
            );

            let mut rtc_err = sys::lkRtcError { message: std::ptr::null() };
            let decrypted_data = sys::lkDataPacketCryptorDecrypt(
                self.ffi.as_ptr(),
                c_str.as_ptr() as *const i8,
                lk_encrypted_data,
                &mut rtc_err, // Assuming the eighth argument is a mutable pointer, adjust as needed
            );

            if decrypted_data.is_null() {
                return Err(format!("Decryption failed: {:?}", rtc_err).into());
            }

            let decrypted_data_rcd = sys::RefCountedData::from_native(decrypted_data);
            let result = decrypted_data_rcd.as_bytes();

            Ok(result)
        }
    }
}

#[derive(Default)]
struct RtcFrameCryptorObserver {
    state_change_handler: Mutex<Option<OnStateChange>>,
}

impl RtcFrameCryptorObserver {
    fn on_frame_cryption_state_change(
        &self,
        participant_id: String,
        state: sys::lkEncryptionState,
    ) {
        let mut handler = self.state_change_handler.lock();
        if let Some(f) = handler.as_mut() {
            f(participant_id, state.into());
        }
    }
}

impl From<sys::lkEncryptionAlgorithm> for EncryptionAlgorithm {
    fn from(value: sys::lkEncryptionAlgorithm) -> Self {
        match value {
            sys::lkEncryptionAlgorithm::AesGcm => Self::AesGcm,
            sys::lkEncryptionAlgorithm::AesCbc => Self::AesCbc,
            _ => panic!("unknown frame cyrptor Algorithm"),
        }
    }
}

impl From<EncryptionAlgorithm> for sys::lkEncryptionAlgorithm {
    fn from(value: EncryptionAlgorithm) -> Self {
        match value {
            EncryptionAlgorithm::AesGcm => Self::AesGcm,
            EncryptionAlgorithm::AesCbc => Self::AesCbc,
        }
    }
}

impl From<sys::lkEncryptionState> for EncryptionState {
    fn from(value: sys::lkEncryptionState) -> Self {
        match value {
            sys::lkEncryptionState::New => Self::New,
            sys::lkEncryptionState::Ok => Self::Ok,
            sys::lkEncryptionState::EncryptionFailed => Self::EncryptionFailed,
            sys::lkEncryptionState::DecryptionFailed => Self::DecryptionFailed,
            sys::lkEncryptionState::MissingKey => Self::MissingKey,
            sys::lkEncryptionState::KeyRatcheted => Self::KeyRatcheted,
            sys::lkEncryptionState::InternalError => Self::InternalError,
            _ => panic!("unknown frame cyrptor FrameCryptionState"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::sys;

    #[tokio::test]
    async fn key_provider_options() {
        let options = super::KeyProviderOptions {
            shared_key: true,
            ratchet_window_size: 20,
            ratchet_salt: vec![1, 2, 3, 4],
            failure_tolerance: 5,
        };

        unsafe {
            let lk_options_ptr = sys::lkKeyProviderOptionsCreate();
            sys::lkKeyProviderOptionsSetSharedKey(lk_options_ptr, options.shared_key);
            sys::lkKeyProviderOptionsSetRatchetWindowSize(
                lk_options_ptr,
                options.ratchet_window_size,
            );
            sys::lkKeyProviderOptionsSetRatchetSalt(
                lk_options_ptr,
                options.ratchet_salt.as_ptr(),
                options.ratchet_salt.len() as u32,
            );
            sys::lkKeyProviderOptionsSetFailureTolerance(lk_options_ptr, options.failure_tolerance);

            let _ = sys::RefCounted::from_raw(lk_options_ptr);
        }

        let key_provider = super::KeyProvider::new(options);
        assert!(!key_provider.ffi.as_ptr().is_null());

        assert_eq!(key_provider.set_shared_key(1, vec![0; 16]), true);

        assert_eq!(key_provider.get_shared_key(1), Some(vec![0; 16]));

        assert_ne!(key_provider.ratchet_shared_key(1), Some(vec![0; 16]));

        assert_eq!(key_provider.set_key("participant1".to_string(), 1, vec![1; 16]), true);

        assert_eq!(key_provider.get_key("participant1".to_string(), 1), Some(vec![1; 16]));

        assert_ne!(key_provider.ratchet_key("participant1".to_string(), 1), Some(vec![1; 16]));

        key_provider.set_sif_trailer(vec![9, 8, 7, 6]);
        

    }
}