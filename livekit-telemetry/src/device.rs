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

use crate::TelemetryEvent;

/// Thermal pressure as the OS reports it (`ProcessInfo.thermalState`, `PowerManager` thermal
/// status).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThermalState {
    #[default]
    Nominal,
    Fair,
    Serious,
    Critical,
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppState {
    #[default]
    Foreground,
    Background,
}

/// What the host observes about the device; pushed with
/// [`Telemetry::set_device_state`](crate::Telemetry::set_device_state) whenever it changes.
///
/// The host owns the OS APIs (thermal, power, lifecycle) — they are not reachable from Rust
/// without a JVM/ObjC bridge — and the core owns what to do with them: emit the change events
/// from `SPEC.md` and stretch its own cadence so telemetry never competes with the call.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeviceState {
    pub thermal: ThermalState,
    pub low_power_mode: bool,
    pub app_state: AppState,
}

impl DeviceState {
    /// Multiplier on the flush interval: 1× at rest, up to 4× under pressure (15 s → 60 s at the
    /// production cadence, per the design doc). Serious thermal, low-power mode and background
    /// each double it; critical thermal quadruples it.
    pub fn cadence_factor(&self) -> u32 {
        let thermal = match self.thermal {
            ThermalState::Nominal | ThermalState::Fair => 1,
            ThermalState::Serious => 2,
            ThermalState::Critical => 4,
        };
        let power = if self.low_power_mode { 2 } else { 1 };
        let background = if self.app_state == AppState::Background { 2 } else { 1 };
        (thermal * power * background).min(4)
    }

    /// The `SPEC.md` events describing what changed between `previous` and `self`
    /// (everything, when there is no previous state: "initial value at session start").
    pub(crate) fn change_events(&self, previous: Option<&DeviceState>) -> Vec<TelemetryEvent> {
        let mut events = Vec::new();
        if previous.is_none_or(|p| p.thermal != self.thermal) {
            events.push(
                TelemetryEvent::new("lk.device.thermal.changed")
                    .with_attribute("lk.device.thermal.state", self.thermal.as_str()),
            );
        }
        if previous.is_none_or(|p| p.low_power_mode != self.low_power_mode) {
            events.push(
                TelemetryEvent::new("lk.device.low_power.changed")
                    .with_attribute("lk.device.low_power.enabled", self.low_power_mode),
            );
        }
        if previous.is_none_or(|p| p.app_state != self.app_state) {
            events.push(
                TelemetryEvent::new("lk.device.app_state.changed")
                    .with_attribute("lk.device.app_state", self.app_state.as_str()),
            );
        }
        events
    }
}

impl ThermalState {
    fn as_str(self) -> &'static str {
        match self {
            ThermalState::Nominal => "nominal",
            ThermalState::Fair => "fair",
            ThermalState::Serious => "serious",
            ThermalState::Critical => "critical",
        }
    }
}

impl AppState {
    fn as_str(self) -> &'static str {
        match self {
            AppState::Foreground => "foreground",
            AppState::Background => "background",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_stretches_under_pressure_and_caps_at_four() {
        assert_eq!(DeviceState::default().cadence_factor(), 1);
        let serious = DeviceState { thermal: ThermalState::Serious, ..Default::default() };
        assert_eq!(serious.cadence_factor(), 2);
        let low_power = DeviceState { low_power_mode: true, ..Default::default() };
        assert_eq!(low_power.cadence_factor(), 2);
        let everything = DeviceState {
            thermal: ThermalState::Critical,
            low_power_mode: true,
            app_state: AppState::Background,
        };
        assert_eq!(everything.cadence_factor(), 4);
    }

    #[test]
    fn only_changed_fields_produce_events() {
        let initial = DeviceState::default();
        assert_eq!(initial.change_events(None).len(), 3, "initial value at session start");
        let hot = DeviceState { thermal: ThermalState::Critical, ..initial };
        let events = hot.change_events(Some(&initial));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "lk.device.thermal.changed");
        assert!(hot.change_events(Some(&hot)).is_empty());
    }
}
