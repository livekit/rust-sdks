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
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use std::ffi::CStr;
use std::ptr;

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let model = sysctl_model()?;
    let name = computer_name()?;
    let device_type = parse_device_type(&model);

    Ok(DeviceInfo {
        model,
        name,
        device_type,
    })
}

fn sysctl_model() -> Result<String, DeviceInfoError> {
    let name = c"hw.model";
    let mut size: libc::size_t = 0;

    let ret = unsafe { libc::sysctlbyname(name.as_ptr(), ptr::null_mut(), &mut size, ptr::null_mut(), 0) };
    if ret != 0 || size == 0 {
        return Err(DeviceInfoError::Query("sysctlbyname hw.model failed".into()));
    }

    let mut buf = vec![0u8; size];
    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
            ptr::null_mut(),
            0,
        )
    };
    if ret != 0 {
        return Err(DeviceInfoError::Query("sysctlbyname hw.model read failed".into()));
    }

    let cstr = CStr::from_bytes_until_nul(&buf)
        .map_err(|e| DeviceInfoError::Query(format!("invalid model string: {e}")))?;
    Ok(cstr.to_string_lossy().into_owned())
}

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    fn SCDynamicStoreCopyComputerName(
        store: *const std::ffi::c_void,
        encoding: *mut u32,
    ) -> *const std::ffi::c_void;
}

fn computer_name() -> Result<String, DeviceInfoError> {
    let mut encoding: u32 = 0;
    let cf_str_ref = unsafe { SCDynamicStoreCopyComputerName(ptr::null(), &mut encoding) };
    if cf_str_ref.is_null() {
        return Err(DeviceInfoError::Query("SCDynamicStoreCopyComputerName returned null".into()));
    }

    let cf_string: CFString = unsafe { TCFType::wrap_under_create_rule(cf_str_ref as _) };
    Ok(cf_string.to_string())
}

fn parse_device_type(model: &str) -> DeviceType {
    // Legacy model identifiers (pre-Apple Silicon) have clear prefixes
    if model.starts_with("MacBook") {
        return DeviceType::Laptop;
    }
    if model.starts_with("iMac")
        || model.starts_with("MacPro")
        || model.starts_with("Macmini")
        || model.starts_with("MacStudio")
    {
        return DeviceType::Desktop;
    }

    // Apple Silicon Macs all use "Mac{N},{N}" — detect laptop via battery presence
    if model.starts_with("Mac") {
        return if has_battery() {
            DeviceType::Laptop
        } else {
            DeviceType::Desktop
        };
    }

    DeviceType::Unknown
}

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceGetMatchingService(
        main_port: u32,
        matching: *const std::ffi::c_void,
    ) -> u32;
    fn IOServiceMatching(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn IOObjectRelease(object: u32) -> i32;
}

fn has_battery() -> bool {
    unsafe {
        let matching = IOServiceMatching(c"AppleSmartBattery".as_ptr());
        if matching.is_null() {
            return false;
        }
        // kIOMainPortDefault = 0
        let service = IOServiceGetMatchingService(0, matching);
        if service != 0 {
            IOObjectRelease(service);
            true
        } else {
            false
        }
    }
}
