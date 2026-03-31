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
use windows_sys::Win32::System::{
    Power::GetSystemPowerStatus,
    Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
    },
    SystemInformation::GetComputerNameExW,
};

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let model = registry_model()?;
    let name = computer_name()?;
    let device_type = detect_device_type(&model);

    Ok(DeviceInfo {
        model,
        name,
        device_type,
    })
}

fn registry_model() -> Result<String, DeviceInfoError> {
    let subkey: Vec<u16> = "HARDWARE\\DESCRIPTION\\System\\BIOS\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "SystemProductName\0".encode_utf16().collect();

    unsafe {
        let mut hkey = 0isize;
        let status = RegOpenKeyExW(HKEY_LOCAL_MACHINE, subkey.as_ptr(), 0, KEY_READ, &mut hkey);
        if status != 0 {
            return Err(DeviceInfoError::Query("failed to open BIOS registry key".into()));
        }

        let mut data_type: u32 = 0;
        let mut data_size: u32 = 0;
        let status = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            std::ptr::null_mut(),
            &mut data_size,
        );
        if status != 0 || data_type != REG_SZ || data_size == 0 {
            RegCloseKey(hkey);
            return Err(DeviceInfoError::Query("failed to query SystemProductName size".into()));
        }

        let mut buf = vec![0u16; (data_size as usize) / 2];
        let status = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &mut data_type,
            buf.as_mut_ptr() as *mut u8,
            &mut data_size,
        );
        RegCloseKey(hkey);

        if status != 0 {
            return Err(DeviceInfoError::Query("failed to read SystemProductName".into()));
        }

        // Trim trailing null
        if let Some(last) = buf.last() {
            if *last == 0 {
                buf.pop();
            }
        }
        Ok(String::from_utf16_lossy(&buf))
    }
}

fn computer_name() -> Result<String, DeviceInfoError> {
    use windows_sys::Win32::System::SystemInformation::ComputerNamePhysicalDnsHostname;

    unsafe {
        let mut size: u32 = 0;
        // First call to get required buffer size
        GetComputerNameExW(ComputerNamePhysicalDnsHostname, std::ptr::null_mut(), &mut size);

        if size == 0 {
            return Err(DeviceInfoError::Query("GetComputerNameExW returned zero size".into()));
        }

        let mut buf = vec![0u16; size as usize];
        let ok = GetComputerNameExW(ComputerNamePhysicalDnsHostname, buf.as_mut_ptr(), &mut size);
        if ok == 0 {
            return Err(DeviceInfoError::Query("GetComputerNameExW failed".into()));
        }

        buf.truncate(size as usize);
        Ok(String::from_utf16_lossy(&buf))
    }
}

fn detect_device_type(model: &str) -> DeviceType {
    let model_lower = model.to_lowercase();

    // HoloLens detection
    if model_lower.contains("hololens") {
        return DeviceType::Headset;
    }

    // Check for battery presence as laptop indicator
    unsafe {
        let mut power_status = std::mem::zeroed();
        if GetSystemPowerStatus(&mut power_status) != 0 {
            // BatteryFlag 128 = no battery, 255 = unknown
            if power_status.BatteryFlag != 128 && power_status.BatteryFlag != 255 {
                return DeviceType::Laptop;
            }
        }
    }

    DeviceType::Desktop
}
