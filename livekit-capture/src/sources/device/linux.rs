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

//! Linux dispatch backend merging Argus (Jetson CSI) and V4L2 capture.
//!
//! Argus sensors are listed first, under `argus:N` identifiers. The raw V4L2
//! nodes belonging to those sensors (recognizable by their Tegra capture
//! driver) are suppressed from enumeration: they deliver raw Bayer frames the
//! V4L2 backend cannot convert, while Argus captures the same sensor through
//! the hardware ISP. Every other V4L2 device (e.g. USB webcams) passes
//! through unchanged, and when no Argus sensor is present — including any
//! non-Jetson machine — enumeration and selection are identical to the plain
//! V4L2 backend.

use livekit::webrtc::video_frame::BoxVideoFrame;

use super::{
    argus, v4l2, DeviceFormat, DeviceInfo, DeviceSelector, DeviceVideoSourceConfig,
    DeviceVideoSourceError,
};
use crate::pump::PumpStop;

/// Capture session dispatching to one of the Linux backends.
pub(super) enum Session {
    V4l2(v4l2::Session),
    Argus(argus::Session),
}

impl Session {
    /// Routes the selector to a backend and opens the device.
    pub(super) fn open(config: &DeviceVideoSourceConfig) -> Result<Self, DeviceVideoSourceError> {
        match route_selector(&config.device, argus::sensor_count()) {
            Route::Argus(sensor_index) => {
                argus::Session::open(sensor_index, config).map(Self::Argus)
            }
            Route::V4l2(selector) => {
                let mut config = config.clone();
                config.device = selector;
                v4l2::Session::open(&config).map(Self::V4l2)
            }
            Route::V4l2Tail(offset) => {
                // An index past the Argus sensors addresses the merged
                // enumeration order, so resolve it against the same filtered
                // V4L2 list that devices() reports.
                let suppress_csi = argus::sensor_count() > 0;
                let v4l2_devices = filter_v4l2_devices(v4l2::devices()?, suppress_csi);
                let device =
                    v4l2_devices.get(offset).ok_or(DeviceVideoSourceError::DeviceNotFound)?;
                let mut config = config.clone();
                config.device = DeviceSelector::Id(device.id.clone());
                v4l2::Session::open(&config).map(Self::V4l2)
            }
        }
    }

    /// Returns the negotiated capture format.
    pub(super) fn format(&self) -> DeviceFormat {
        match self {
            Self::V4l2(session) => session.format(),
            Self::Argus(session) => session.format(),
        }
    }

    /// Blocks until the next frame is available, returning `Ok(None)` once
    /// the stop token fires.
    pub(super) fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
        match self {
            Self::V4l2(session) => session.next_frame(stop),
            Self::Argus(session) => session.next_frame(stop),
        }
    }
}

/// Lists Argus sensors followed by (non-CSI) V4L2 devices.
pub(super) fn devices() -> Result<Vec<DeviceInfo>, DeviceVideoSourceError> {
    // Argus failures never break enumeration: degrade to V4L2-only.
    let argus_devices = argus::devices().unwrap_or_else(|error| {
        log::debug!("Argus device enumeration unavailable: {error}");
        Vec::new()
    });
    let v4l2_devices = v4l2::devices()?;
    Ok(merge_devices(argus_devices, v4l2_devices))
}

/// Backend resolved for a device selector.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Route {
    /// Argus sensor by index.
    Argus(u32),
    /// V4L2 with the selector passed through unchanged.
    V4l2(DeviceSelector),
    /// The V4L2 device at this position in the *filtered* V4L2 list (the
    /// merged enumeration order with the Argus prefix stripped).
    V4l2Tail(usize),
}

/// Pure routing decision for a selector given how many Argus sensors exist.
fn route_selector(selector: &DeviceSelector, argus_sensor_count: u32) -> Route {
    match selector {
        // A CSI sensor's own V4L2 node delivers raw Bayer the V4L2 backend
        // cannot convert, so on a Jetson with a connected sensor the default
        // must be Argus — which also matches devices() ordering.
        DeviceSelector::Default => {
            if argus_sensor_count > 0 {
                Route::Argus(0)
            } else {
                Route::V4l2(DeviceSelector::Default)
            }
        }
        // Indices address the merged enumeration order: Argus first.
        DeviceSelector::Index(index) => {
            if (*index as u64) < u64::from(argus_sensor_count) {
                Route::Argus(*index as u32)
            } else {
                Route::V4l2Tail(index - argus_sensor_count as usize)
            }
        }
        DeviceSelector::Id(id) => match parse_argus_id(id) {
            // Route even out-of-range indices to Argus so they fail with
            // DeviceNotFound instead of hitting V4L2 with a foreign id.
            Some(sensor_index) => Route::Argus(sensor_index),
            None => Route::V4l2(selector.clone()),
        },
    }
}

/// Parses an `argus:N` device identifier.
fn parse_argus_id(id: &str) -> Option<u32> {
    let index = id.strip_prefix("argus:")?;
    if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    index.parse().ok()
}

/// Merges the two backend device lists, suppressing the V4L2 nodes of
/// Argus-backed CSI sensors — but only when Argus actually reports sensors,
/// so a missing Argus stack changes nothing.
fn merge_devices(argus_devices: Vec<DeviceInfo>, v4l2_devices: Vec<DeviceInfo>) -> Vec<DeviceInfo> {
    let suppress_csi = !argus_devices.is_empty();
    let mut merged = argus_devices;
    merged.extend(filter_v4l2_devices(v4l2_devices, suppress_csi));
    merged
}

/// Drops Argus-backed CSI capture nodes from a V4L2 device list when
/// `suppress_csi` is set.
fn filter_v4l2_devices(devices: Vec<DeviceInfo>, suppress_csi: bool) -> Vec<DeviceInfo> {
    devices
        .into_iter()
        .filter(|device| {
            let suppressed =
                suppress_csi && is_argus_backed_v4l2_node(device.manufacturer.as_deref());
            if suppressed {
                log::debug!(
                    "Suppressing CSI V4L2 node \"{}\" ({}): captured through Argus",
                    device.name,
                    device.id
                );
            }
            !suppressed
        })
        .collect()
}

/// Recognizes the V4L2 capture drivers of Argus-backed Tegra CSI sensors.
/// The V4L2 backend reports the driver name in [`DeviceInfo::manufacturer`].
fn is_argus_backed_v4l2_node(driver: Option<&str>) -> bool {
    // "tegra-video" covers JetPack 5/6 (L4T r35+); the others appear on
    // older L4T releases.
    const ARGUS_BACKED_DRIVERS: [&str; 3] = ["tegra-video", "tegra-vi4", "vi"];
    driver.is_some_and(|driver| ARGUS_BACKED_DRIVERS.contains(&driver))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, driver: Option<&str>) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            name: format!("device {id}"),
            model_id: None,
            manufacturer: driver.map(str::to_string),
            formats: Vec::new(),
            formats_complete: true,
        }
    }

    #[test]
    fn parses_argus_ids() {
        assert_eq!(parse_argus_id("argus:0"), Some(0));
        assert_eq!(parse_argus_id("argus:12"), Some(12));
        assert_eq!(parse_argus_id("argus:"), None);
        assert_eq!(parse_argus_id("argus:x"), None);
        assert_eq!(parse_argus_id("argus:-1"), None);
        assert_eq!(parse_argus_id("argus:1x"), None);
        assert_eq!(parse_argus_id("0"), None);
        assert_eq!(parse_argus_id("/dev/video0"), None);
    }

    #[test]
    fn default_routes_to_argus_only_when_sensors_exist() {
        assert_eq!(route_selector(&DeviceSelector::Default, 1), Route::Argus(0));
        assert_eq!(
            route_selector(&DeviceSelector::Default, 0),
            Route::V4l2(DeviceSelector::Default)
        );
    }

    #[test]
    fn indices_address_the_merged_order() {
        assert_eq!(route_selector(&DeviceSelector::Index(0), 2), Route::Argus(0));
        assert_eq!(route_selector(&DeviceSelector::Index(1), 2), Route::Argus(1));
        assert_eq!(route_selector(&DeviceSelector::Index(2), 2), Route::V4l2Tail(0));
        assert_eq!(route_selector(&DeviceSelector::Index(3), 2), Route::V4l2Tail(1));
        assert_eq!(
            route_selector(&DeviceSelector::Index(0), 0),
            Route::V4l2Tail(0)
        );
    }

    #[test]
    fn ids_route_by_namespace() {
        assert_eq!(route_selector(&DeviceSelector::Id("argus:1".into()), 2), Route::Argus(1));
        // Out of range still routes to Argus, which reports DeviceNotFound.
        assert_eq!(route_selector(&DeviceSelector::Id("argus:9".into()), 2), Route::Argus(9));
        assert_eq!(
            route_selector(&DeviceSelector::Id("0".into()), 2),
            Route::V4l2(DeviceSelector::Id("0".into()))
        );
        assert_eq!(
            route_selector(&DeviceSelector::Id("/dev/video7".into()), 2),
            Route::V4l2(DeviceSelector::Id("/dev/video7".into()))
        );
    }

    #[test]
    fn recognizes_tegra_capture_drivers() {
        assert!(is_argus_backed_v4l2_node(Some("tegra-video")));
        assert!(is_argus_backed_v4l2_node(Some("vi")));
        assert!(!is_argus_backed_v4l2_node(Some("uvcvideo")));
        assert!(!is_argus_backed_v4l2_node(None));
    }

    #[test]
    fn merge_suppresses_csi_nodes_only_with_argus_present() {
        let argus_devices = vec![device("argus:0", Some("nvidia-argus"))];
        let v4l2_devices =
            vec![device("0", Some("tegra-video")), device("1", Some("uvcvideo"))];

        let merged = merge_devices(argus_devices, v4l2_devices.clone());
        let ids: Vec<&str> = merged.iter().map(|device| device.id.as_str()).collect();
        assert_eq!(ids, ["argus:0", "1"]);

        // No Argus sensors: nothing is suppressed.
        let merged = merge_devices(Vec::new(), v4l2_devices);
        let ids: Vec<&str> = merged.iter().map(|device| device.id.as_str()).collect();
        assert_eq!(ids, ["0", "1"]);
    }
}
