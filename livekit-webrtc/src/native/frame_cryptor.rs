use crate::frame_cryptor::{
    Algorithm, FrameCryptionState, KeyProviderOptions
};

use cxx::SharedPtr;
use webrtc_sys::frame_cryptor::{self as sys_fc, RTCFrameCryptorObserverWrapper};

use super::{rtp_sender::RtpSender, rtp_receiver::RtpReceiver};

impl From<sys_fc::ffi::Algorithm> for Algorithm {
    fn from(value: sys_fc::ffi::Algorithm) -> Self {
        match value {
            sys_fc::ffi::Algorithm::AesGcm => Self::AesGcm,
            sys_fc::ffi::Algorithm::AesCbc => Self::AesCbc,
            _ => panic!("unknown frame cyrptor Algorithm"),
        }
    }
}

impl From<Algorithm> for sys_fc::ffi::Algorithm {
    fn from(value: Algorithm) -> Self {
        match value {
            Algorithm::AesGcm => Self::AesGcm,
            Algorithm::AesCbc => Self::AesCbc,
        }
    }
}

impl From<sys_fc::ffi::FrameCryptionState> for FrameCryptionState {
    fn from(value: sys_fc::ffi::FrameCryptionState) -> Self {
        match value {
            sys_fc::ffi::FrameCryptionState::New => Self::New,
            sys_fc::ffi::FrameCryptionState::Ok => Self::Ok,
            sys_fc::ffi::FrameCryptionState::EncryptionFailed => Self::EncryptionFailed,
            sys_fc::ffi::FrameCryptionState::DecryptionFailed => Self::DecryptionFailed,
            sys_fc::ffi::FrameCryptionState::MissingKey => Self::MissingKey,
            sys_fc::ffi::FrameCryptionState::KeyRatcheted => Self::KeyRatcheted,
            sys_fc::ffi::FrameCryptionState::InternalError => Self::InternalError,
            _ => panic!("unknown frame cyrptor FrameCryptionState"),
        }
    }
}

impl From<KeyProviderOptions> for sys_fc::ffi::KeyProviderOptions {
    fn from(value: KeyProviderOptions) -> Self {
        Self {
            shared_key: value.shared_key,
            ratchet_window_size: value.ratchet_window_size,
            ratchet_salt: value.ratchet_salt,
            uncrypted_magic_bytes: value.uncrypted_magic_bytes,
        }
    }
}


#[derive(Clone)]
pub struct KeyProvider {
    pub(crate) sys_handle: SharedPtr<sys_fc::ffi::KeyProvider>,
}

impl KeyProvider {
    pub fn new(options: crate::frame_cryptor::KeyProviderOptions) -> Self {
        Self {
            sys_handle: sys_fc::ffi::new_key_provider(options.into()),
        }
    }

    pub fn set_key(&self, participant_id: String, key_index: i32, key: Vec<u8>) -> bool {
        return self.sys_handle.set_key(participant_id, key_index, key);
    }

    pub fn ratchet_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        return self.sys_handle.ratchet_key(participant_id, key_index);
    }

    pub fn export_key(&self, participant_id: String, key_index: i32) -> Vec<u8> {
        return self.sys_handle.export_key(participant_id, key_index);
    }
}

#[derive(Clone)]
pub struct FrameCryptor {
    pub(crate) sys_handle: SharedPtr<sys_fc::ffi::FrameCryptor>,
}

impl FrameCryptor {
 
    pub fn new_for_rtp_sender(
        participant_id: String,
        algorithm: Algorithm,
        key_provider: KeyProvider,
        sender: RtpSender,
    ) -> Self {
        Self {
            sys_handle: sys_fc::ffi::new_frame_cryptor_for_rtp_sender(
                participant_id,
                algorithm.into(),
                key_provider.sys_handle,
                sender.sys_handle,
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
            sys_handle: sys_fc::ffi::new_frame_cryptor_for_rtp_receiver(
                participant_id,
                algorithm.into(),
                key_provider.sys_handle,
                receiver.sys_handle,
            ),
        }
    }

    pub fn set_enabled(self: &FrameCryptor, enabled: bool) {
        self.sys_handle.set_enabled(enabled);
    }

    pub fn enabled(self: &FrameCryptor) -> bool {
        self.sys_handle.enabled()
    }

    pub fn set_key_index(self: &FrameCryptor, index: i32) {
        self.sys_handle.set_key_index(index);
    }

    pub fn key_index(self: &FrameCryptor) -> i32 {
        self.sys_handle.key_index()
    }

    pub fn participant_id(self: &FrameCryptor) -> String {
        self.sys_handle.participant_id()
    }
    
    pub fn register_observer(self: &FrameCryptor, observer: Box<RTCFrameCryptorObserverWrapper>) {
        self.sys_handle.register_observer(observer);
    }

    pub fn unregister_observer(self: &FrameCryptor) {
        self.sys_handle.unregister_observer();
    }
    
}