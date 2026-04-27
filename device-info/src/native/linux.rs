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
    let model = device_model();
    let name = hostname()?;
    let device_type = detect_device_type();

    Ok(DeviceInfo { model, name, device_type })
}

fn device_model() -> String {
    // x86/SMBIOS systems (most PCs, laptops, servers)
    if let Some(m) = read_trimmed("/sys/class/dmi/id/product_name") {
        return m;
    }
    // ARM boards (Raspberry Pi, Jetson, etc.)
    if let Some(m) = read_trimmed("/proc/device-tree/model") {
        // device-tree model often has a trailing null byte
        return m.trim_end_matches('\0').to_string();
    }
    "Unknown".into()
}

fn read_trimmed(path: &str) -> Option<String> {
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

fn detect_device_type() -> DeviceType {
    // 1. SMBIOS chassis type (x86 PCs)
    if let Some(raw) = read_trimmed("/sys/class/dmi/id/chassis_type") {
        if let Ok(code) = raw.parse::<u32>() {
            match code {
                3 | 4 | 5 | 6 | 7 | 11 | 15 | 16 | 24 | 33 | 34 | 35 | 36 => {
                    return DeviceType::Desktop
                }
                8 | 9 | 10 | 14 | 31 | 32 => return DeviceType::Laptop,
                30 => return DeviceType::Tablet,
                _ => {}
            }
        }
    }

    // 2. systemd machine-info (set via hostnamectl set-chassis)
    if let Some(chassis) = parse_machine_info_chassis() {
        match chassis.as_str() {
            "desktop" | "tower" | "server" | "all-in-one" => return DeviceType::Desktop,
            "laptop" | "notebook" | "convertible" => return DeviceType::Laptop,
            "tablet" => return DeviceType::Tablet,
            "handset" => return DeviceType::Phone,
            _ => {}
        }
    }

    // 3. Battery presence as laptop indicator
    if has_battery() {
        return DeviceType::Laptop;
    }

    DeviceType::Unknown
}

fn parse_machine_info_chassis() -> Option<String> {
    let content = fs::read_to_string("/etc/machine-info").ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("CHASSIS=") {
            return Some(value.trim_matches('"').to_lowercase());
        }
    }
    None
}

fn has_battery() -> bool {
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return false;
    };
    for entry in entries.flatten() {
        let type_path = entry.path().join("type");
        if let Ok(t) = fs::read_to_string(type_path) {
            if t.trim().eq_ignore_ascii_case("battery") {
                return true;
            }
        }
    }
    false
}
