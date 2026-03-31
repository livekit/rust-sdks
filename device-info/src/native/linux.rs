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

use crate::{DeviceInfo, DeviceInfoError, DeviceType};
use std::ffi::CStr;
use std::fs;

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let model = read_dmi_file("/sys/class/dmi/id/product_name").unwrap_or_else(|| "Unknown".into());
    let name = hostname()?;
    let device_type = chassis_type();

    Ok(DeviceInfo { model, name, device_type })
}

fn read_dmi_file(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn hostname() -> Result<String, DeviceInfoError> {
    let mut buf = [0u8; 256];
    let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
    if ret != 0 {
        return Err(DeviceInfoError::Query("gethostname failed".into()));
    }

    let cstr = CStr::from_bytes_until_nul(&buf)
        .map_err(|e| DeviceInfoError::Query(format!("invalid hostname: {e}")))?;
    Ok(cstr.to_string_lossy().into_owned())
}

fn chassis_type() -> DeviceType {
    let Some(raw) = read_dmi_file("/sys/class/dmi/id/chassis_type") else {
        return DeviceType::Unknown;
    };
    let Ok(code) = raw.parse::<u32>() else {
        return DeviceType::Unknown;
    };

    // SMBIOS chassis type codes
    match code {
        3 | 4 | 5 | 6 | 7 | 11 | 15 | 16 | 24 | 33 | 34 | 35 | 36 => DeviceType::Desktop,
        8 | 9 | 10 | 14 | 31 | 32 => DeviceType::Laptop,
        30 => DeviceType::Tablet,
        17 => DeviceType::Unknown, // Server — not modeled as a DeviceType
        _ => DeviceType::Unknown,
    }
}
