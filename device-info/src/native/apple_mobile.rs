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

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let model = utsname_machine()?;
    let name = ui_device_name()?;
    let device_type = parse_device_type(&model);

    Ok(DeviceInfo { model, name, device_type })
}

fn utsname_machine() -> Result<String, DeviceInfoError> {
    unsafe {
        let mut uts: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut uts) != 0 {
            return Err(DeviceInfoError::Query("uname failed".into()));
        }
        let cstr = CStr::from_ptr(uts.machine.as_ptr());
        Ok(cstr.to_string_lossy().into_owned())
    }
}

fn ui_device_name() -> Result<String, DeviceInfoError> {
    use objc2::MainThreadMarker;
    use objc2_ui_kit::UIDevice;

    // UIDevice can be accessed from any thread, but currentDevice() requires
    // a MainThreadMarker in objc2. We assume the caller is on the main thread
    // or that UIDevice is safe to query (which it is in practice).
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let device = UIDevice::currentDevice(mtm);
    let name = device.name();
    Ok(name.to_string())
}

fn parse_device_type(model: &str) -> DeviceType {
    if model.starts_with("iPhone") || model.starts_with("iPod") {
        DeviceType::Phone
    } else if model.starts_with("iPad") {
        DeviceType::Tablet
    } else if model.starts_with("AppleTV") {
        DeviceType::Television
    } else if model.starts_with("Watch") {
        DeviceType::Watch
    } else if model.starts_with("RealityDevice") {
        DeviceType::Headset
    } else {
        DeviceType::Unknown
    }
}
