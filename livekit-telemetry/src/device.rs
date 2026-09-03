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
/// status, the web Compute Pressure API — all four share this vocabulary).
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

/// Memory pressure as the OS reports it (`DISPATCH_MEMORYPRESSURE_*`, `onTrimMemory` levels:
/// `RUNNING_LOW`/`BACKGROUND` → warning, `RUNNING_CRITICAL`/`COMPLETE` → critical).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryPressure {
    #[default]
    Normal,
    Warning,
    Critical,
}

/// The active network path, in OTel `network.connection.type` terms.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkType {
    #[default]
    Unknown,
    Wifi,
    Cell,
    Wired,
    Unavailable,
}

/// What the host observes about the device; pushed with
/// [`Telemetry::set_device_state`](crate::Telemetry::set_device_state) whenever anything changes.
///
/// The host owns the OS APIs (thermal, power, memory, battery, network path, lifecycle) — they
/// are not reachable from Rust without a JVM/ObjC bridge — and the core owns what to do with
/// them: emit the change events from `SPEC.md`, stretch its own cadence and hold uploads so
/// telemetry never competes with the call. CPU is deliberately not measured here (measuring CPU
/// costs CPU): thermal state is the OS's judgement, and WebRTC's own `qualityLimitationReason`
/// (see [`RtcStatsSample`](crate::RtcStatsSample)) says when the encoder is CPU-starved.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeviceState {
    pub thermal: ThermalState,
    pub low_power_mode: bool,
    pub app_state: AppState,
    // No `uniffi(default)` on enum fields: uniffi 0.31's Swift generator rejects `Default` there.
    pub memory: MemoryPressure,
    pub network: NetworkType,
    /// Cellular / personal hotspot (`NWPath.isExpensive`, metered on Android).
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub network_expensive: bool,
    /// The user asked for less traffic: Low Data Mode (`NWPath.isConstrained`), Data Saver,
    /// `navigator.connection.saveData`.
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub network_constrained: bool,
    /// Battery charge in percent; `None` when unknown (desktops, tvOS, monitoring disabled).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub battery_level: Option<u32>,
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub battery_charging: bool,
}

impl DeviceState {
    /// Below this, unplugged, the pipeline still records but stops uploading (the Datadog rule).
    const BATTERY_HOLD_PERCENT: u32 = 10;
    /// Below this, unplugged, the cadence doubles.
    const BATTERY_LOW_PERCENT: u32 = 20;

    /// Multiplier on the flush interval and stats window: 1× at rest, up to 4× under pressure
    /// (15 s → 60 s at the production cadence, per the design doc). Serious thermal, memory
    /// warning, low-power mode, background, low battery and a constrained network each double
    /// it; critical thermal or memory quadruple it.
    pub fn cadence_factor(&self) -> u32 {
        let thermal = match self.thermal {
            ThermalState::Nominal | ThermalState::Fair => 1,
            ThermalState::Serious => 2,
            ThermalState::Critical => 4,
        };
        let memory = match self.memory {
            MemoryPressure::Normal => 1,
            MemoryPressure::Warning => 2,
            MemoryPressure::Critical => 4,
        };
        let doubled = [
            self.low_power_mode,
            self.app_state == AppState::Background,
            self.battery_bucket() >= 1,
            self.network_constrained,
        ]
        .iter()
        .filter(|&&on| on)
        .count() as u32;
        ((thermal * memory) << doubled).min(4)
    }

    /// Uploads should wait: the user asked for less traffic (Low Data Mode / Data Saver) or the
    /// battery is nearly empty and unplugged. Data keeps flowing into the cache meanwhile.
    pub fn holds_uploads(&self) -> bool {
        self.network_constrained || self.battery_bucket() == 2
    }

    /// 0 = fine or unknown, 1 = low (≤ 20 %), 2 = nearly empty (≤ 10 %); always 0 on charge.
    /// Per-percent battery updates only matter when they cross one of these.
    fn battery_bucket(&self) -> u8 {
        match self.battery_level {
            Some(level) if !self.battery_charging && level <= Self::BATTERY_HOLD_PERCENT => 2,
            Some(level) if !self.battery_charging && level <= Self::BATTERY_LOW_PERCENT => 1,
            _ => 0,
        }
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
        if previous.is_none_or(|p| p.memory != self.memory) {
            events.push(
                TelemetryEvent::new("lk.device.memory.changed")
                    .with_attribute("lk.device.memory.pressure", self.memory.as_str()),
            );
        }
        if previous.is_none_or(|p| {
            (p.network, p.network_expensive, p.network_constrained)
                != (self.network, self.network_expensive, self.network_constrained)
        }) {
            events.push(
                TelemetryEvent::new("lk.device.network.changed")
                    .with_attribute("network.connection.type", self.network.as_str())
                    .with_attribute("lk.device.network.expensive", self.network_expensive)
                    .with_attribute("lk.device.network.constrained", self.network_constrained),
            );
        }
        // Per-percent updates are noise; report charging flips and bucket crossings.
        if self.battery_level.is_some()
            && previous.is_none_or(|p| {
                p.battery_charging != self.battery_charging
                    || p.battery_bucket() != self.battery_bucket()
                    || p.battery_level.is_none()
            })
        {
            events.push(
                TelemetryEvent::new("lk.device.battery.changed")
                    .with_attribute(
                        "lk.device.battery.level",
                        self.battery_level.unwrap_or(0) as i64,
                    )
                    .with_attribute("lk.device.battery.charging", self.battery_charging),
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

impl MemoryPressure {
    fn as_str(self) -> &'static str {
        match self {
            MemoryPressure::Normal => "normal",
            MemoryPressure::Warning => "warning",
            MemoryPressure::Critical => "critical",
        }
    }
}

impl NetworkType {
    fn as_str(self) -> &'static str {
        match self {
            NetworkType::Unknown => "unknown",
            NetworkType::Wifi => "wifi",
            NetworkType::Cell => "cell",
            NetworkType::Wired => "wired",
            NetworkType::Unavailable => "unavailable",
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
        let tight = DeviceState { memory: MemoryPressure::Warning, ..Default::default() };
        assert_eq!(tight.cadence_factor(), 2);
        let low_battery = DeviceState { battery_level: Some(15), ..Default::default() };
        assert_eq!(low_battery.cadence_factor(), 2);
        let charging = DeviceState { battery_charging: true, ..low_battery };
        assert_eq!(charging.cadence_factor(), 1, "a charging battery is not low");
        let everything = DeviceState {
            thermal: ThermalState::Critical,
            low_power_mode: true,
            app_state: AppState::Background,
            memory: MemoryPressure::Critical,
            network_constrained: true,
            ..Default::default()
        };
        assert_eq!(everything.cadence_factor(), 4);
    }

    #[test]
    fn uploads_hold_on_constrained_network_or_empty_battery() {
        assert!(!DeviceState::default().holds_uploads());
        let saver = DeviceState { network_constrained: true, ..Default::default() };
        assert!(saver.holds_uploads());
        let empty = DeviceState { battery_level: Some(8), ..Default::default() };
        assert!(empty.holds_uploads());
        let plugged = DeviceState { battery_charging: true, ..empty };
        assert!(!plugged.holds_uploads());
        let low = DeviceState { battery_level: Some(15), ..Default::default() };
        assert!(!low.holds_uploads(), "low only stretches the cadence");
    }

    #[test]
    fn only_changed_fields_produce_events() {
        let initial = DeviceState::default();
        let names: Vec<_> = initial.change_events(None).into_iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            [
                "lk.device.thermal.changed",
                "lk.device.low_power.changed",
                "lk.device.app_state.changed",
                "lk.device.memory.changed",
                "lk.device.network.changed",
            ],
            "initial values at session start; battery is unknown so it stays silent"
        );
        let hot = DeviceState { thermal: ThermalState::Critical, ..initial };
        let events = hot.change_events(Some(&initial));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "lk.device.thermal.changed");
        assert!(hot.change_events(Some(&hot)).is_empty());
    }

    #[test]
    fn battery_reports_bucket_crossings_not_every_percent() {
        let at = |level| DeviceState { battery_level: Some(level), ..Default::default() };
        assert_eq!(at(80).change_events(Some(&DeviceState::default())).len(), 1, "first reading");
        assert!(at(79).change_events(Some(&at(80))).is_empty());
        assert_eq!(at(20).change_events(Some(&at(21))).len(), 1, "crossed into low");
        assert_eq!(at(10).change_events(Some(&at(11))).len(), 1, "crossed into hold");
        let charging = DeviceState { battery_charging: true, ..at(10) };
        assert_eq!(charging.change_events(Some(&at(10))).len(), 1, "plugged in");
    }
}
