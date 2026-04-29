// Copyright 2026 LiveKit, Inc.
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

//! Error types for platform audio operations.

use std::fmt;

/// Errors that can occur during audio operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Platform ADM could not be initialized.
    ///
    /// This can happen if:
    /// - No audio devices are available
    /// - Audio permissions are not granted
    /// - Platform audio subsystem is unavailable
    PlatformInitFailed,

    /// The specified device index is invalid.
    ///
    /// Device indices are 0-based and must be less than the device count.
    InvalidDeviceIndex,

    /// An audio operation failed.
    OperationFailed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::PlatformInitFailed => {
                write!(f, "Failed to initialize platform audio")
            }
            AudioError::InvalidDeviceIndex => write!(f, "Invalid device index"),
            AudioError::OperationFailed(msg) => write!(f, "Audio operation failed: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

/// Result type for audio operations.
pub type AudioResult<T> = Result<T, AudioError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_error_display() {
        let err = AudioError::PlatformInitFailed;
        let msg = format!("{}", err);
        assert!(msg.contains("platform audio"));

        let err = AudioError::InvalidDeviceIndex;
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid device index"));

        let err = AudioError::OperationFailed("test message".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("test message"));
    }

    #[test]
    fn audio_error_debug() {
        let err = AudioError::PlatformInitFailed;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("PlatformInitFailed"));

        let err = AudioError::InvalidDeviceIndex;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("InvalidDeviceIndex"));
    }

    #[test]
    fn audio_error_equality() {
        assert_eq!(AudioError::PlatformInitFailed, AudioError::PlatformInitFailed);
        assert_eq!(AudioError::InvalidDeviceIndex, AudioError::InvalidDeviceIndex);
        assert_eq!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("a".to_string())
        );
        assert_ne!(
            AudioError::OperationFailed("a".to_string()),
            AudioError::OperationFailed("b".to_string())
        );
    }

    #[test]
    fn audio_error_clone() {
        let err = AudioError::OperationFailed("test".to_string());
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn audio_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(AudioError::InvalidDeviceIndex);
        assert!(err.to_string().contains("Invalid device index"));
    }

    #[test]
    fn audio_result_ok() {
        let result: AudioResult<i32> = Ok(42);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn audio_result_err() {
        let result: AudioResult<i32> = Err(AudioError::InvalidDeviceIndex);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AudioError::InvalidDeviceIndex);
    }
}
