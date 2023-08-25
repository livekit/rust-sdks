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

pub mod manager;
pub mod options;
pub mod key_provider;

use livekit_webrtc::frame_cryptor::FrameCryptionState;

#[derive(Debug, Clone)]
#[repr(i32)]
pub enum E2EEState {
    New = 0,
    Ok,
    EncryptionFailed,
    DecryptionFailed,
    MissingKey,
    KeyRatcheted,
    InternalError,
}

impl From< livekit_webrtc::frame_cryptor::FrameCryptionState> for E2EEState {
    fn from(value: livekit_webrtc::frame_cryptor::FrameCryptionState) -> Self {
        match value {
            FrameCryptionState::New => Self::New,
            FrameCryptionState::Ok => Self::Ok,
            FrameCryptionState::EncryptionFailed => Self::EncryptionFailed,
            FrameCryptionState::DecryptionFailed => Self::DecryptionFailed,
            FrameCryptionState::MissingKey => Self::MissingKey,
            FrameCryptionState::KeyRatcheted => Self::KeyRatcheted,
            FrameCryptionState::InternalError => Self::InternalError,
        }
    }
}