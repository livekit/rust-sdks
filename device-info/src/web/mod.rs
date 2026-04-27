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

pub fn device_info() -> Result<DeviceInfo, DeviceInfoError> {
    let window = web_sys::window().ok_or(DeviceInfoError::Query("no global window".into()))?;
    let navigator = window.navigator();

    let platform = navigator.platform().unwrap_or_default();
    let user_agent = navigator.user_agent().unwrap_or_default();

    let device_type = parse_device_type(&user_agent);
    let name = parse_browser_name(&user_agent);

    Ok(DeviceInfo { model: platform, name, device_type })
}

fn parse_browser_name(ua: &str) -> String {
    // Order matters: check specific browsers before generic engines.
    // E.g. Chrome UA contains "Safari", Edge UA contains "Chrome".
    if ua.contains("Edg/") {
        "Edge".into()
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        "Opera".into()
    } else if ua.contains("Firefox/") {
        "Firefox".into()
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        "Chrome".into()
    } else if ua.contains("Safari/") {
        "Safari".into()
    } else {
        "Browser".into()
    }
}

fn parse_device_type(ua: &str) -> DeviceType {
    let ua_lower = ua.to_lowercase();

    if ua_lower.contains("quest") {
        DeviceType::Headset
    } else if ua_lower.contains("hololens") {
        DeviceType::Headset
    } else if ua_lower.contains("ipad") {
        DeviceType::Tablet
    } else if ua_lower.contains("tablet")
        || ua_lower.contains("android") && !ua_lower.contains("mobile")
    {
        DeviceType::Tablet
    } else if ua_lower.contains("mobile")
        || ua_lower.contains("iphone")
        || ua_lower.contains("ipod")
    {
        DeviceType::Phone
    } else {
        DeviceType::Desktop
    }
}
