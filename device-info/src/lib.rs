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

use std::fmt;

#[cfg_attr(target_arch = "wasm32", path = "web/mod.rs")]
#[cfg_attr(not(target_arch = "wasm32"), path = "native/mod.rs")]
mod imp;

#[cfg(target_os = "android")]
pub use imp::android;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Headset,
    Television,
    Watch,
    Unknown,
}

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceType::Desktop => write!(f, "Desktop"),
            DeviceType::Laptop => write!(f, "Laptop"),
            DeviceType::Phone => write!(f, "Phone"),
            DeviceType::Tablet => write!(f, "Tablet"),
            DeviceType::Headset => write!(f, "Headset"),
            DeviceType::Television => write!(f, "Television"),
            DeviceType::Watch => write!(f, "Watch"),
            DeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub model: String,
    pub name: String,
    pub device_type: DeviceType,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DeviceInfoError {
    #[error("platform not supported")]
    Unsupported,
    #[error("failed to query device info: {0}")]
    Query(String),
    #[cfg(target_os = "android")]
    #[error("android JNI not initialized — call device_info::android::init() first")]
    NotInitialized,
    #[cfg(target_os = "android")]
    #[error("JNI error: {0}")]
    Jni(String),
}

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    imp::device_info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info() {
        let info = device_info().expect("device_info() should succeed");
        assert!(!info.model.is_empty(), "model should not be empty");
        assert!(!info.name.is_empty(), "name should not be empty");
        println!("model: {}", info.model);
        println!("name: {}", info.name);
        println!("type: {}", info.device_type);
    }

    #[test]
    fn test_device_type_display() {
        assert_eq!(DeviceType::Desktop.to_string(), "Desktop");
        assert_eq!(DeviceType::Laptop.to_string(), "Laptop");
        assert_eq!(DeviceType::Phone.to_string(), "Phone");
        assert_eq!(DeviceType::Tablet.to_string(), "Tablet");
        assert_eq!(DeviceType::Headset.to_string(), "Headset");
        assert_eq!(DeviceType::Television.to_string(), "Television");
        assert_eq!(DeviceType::Watch.to_string(), "Watch");
        assert_eq!(DeviceType::Unknown.to_string(), "Unknown");
    }
}
