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
    /// The platform has no thermal source: no pressure assumed, no change event.
    #[default]
    Unknown,
    /// No thermal pressure; sent as `nominal`.
    Nominal,
    /// Slightly raised thermal pressure; sent as `fair`.
    Fair,
    /// High thermal pressure; sent as `serious`.
    Serious,
    /// Pressure high enough to cut performance until the device cools; sent as `critical`.
    Critical,
}

/// Whether the app is in front of the user.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppState {
    /// The app is in front of the user; sent as `foreground`.
    #[default]
    Foreground,
    /// The app runs in the background; sent as `background`.
    Background,
}

/// Memory pressure as the OS reports it (`DISPATCH_MEMORYPRESSURE_*`, `onTrimMemory` levels:
/// `RUNNING_LOW`/`BACKGROUND` → warning, `RUNNING_CRITICAL`/`COMPLETE` → critical).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryPressure {
    /// No memory pressure; sent as `normal`.
    #[default]
    Normal,
    /// The OS warns that memory runs low; sent as `warning`.
    Warning,
    /// Memory is critically low; sent as `critical`.
    Critical,
}

/// The active network path, in OTel `network.connection.type` terms.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkType {
    /// The platform cannot tell the network type; sent as `unknown`.
    #[default]
    Unknown,
    /// Wi-Fi; sent as `wifi`.
    Wifi,
    /// A cellular network; sent as `cell`.
    Cell,
    /// Ethernet or another wired link; sent as `wired`.
    Wired,
    /// A VPN tunnel is the active interface (Android `TRANSPORT_VPN`, WebRTC candidate `vpn`).
    Vpn,
    /// Bluetooth tethering (Android `TRANSPORT_BLUETOOTH`, WebRTC candidate `bluetooth`).
    Bluetooth,
    /// Connected over something the platform does not name (`NWInterface.InterfaceType.other`,
    /// Android `TRANSPORT_USB` / `LOWPAN`, …).
    Other,
    /// No network path: the device is offline; sent as `unavailable`.
    Unavailable,
}

/// What the host observes about the device; pushed with `Telemetry::set_device_state` whenever
/// anything changes.
///
/// The host owns the OS APIs (thermal, power, memory, battery, network path, lifecycle) — they
/// are not reachable from Rust without a JVM/ObjC bridge — and the core owns what to do with
/// them: emit the change events from `SPEC.md`, stretch its own cadence and hold uploads so
/// telemetry never competes with the call. CPU is deliberately not measured here (measuring CPU
/// costs CPU): thermal state is the OS's judgement, and WebRTC's own `qualityLimitationReason`
/// (see [`RtcStatsSample`](crate::RtcStatsSample)) says when the encoder is CPU-starved.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeviceState {
    /// The OS's thermal state; `Unknown` when the platform has no source.
    pub thermal: ThermalState,
    /// Low Power Mode / Battery Saver; `None` when the platform has no source (no change event,
    /// no stretch).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub low_power_mode: Option<bool>,
    /// Whether the app is in front of the user.
    pub app_state: AppState,
    // No `uniffi(default)` on enum fields: uniffi 0.31's Swift generator rejects `Default` there.
    /// The OS's memory pressure.
    pub memory: MemoryPressure,
    /// The active network path.
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
    /// Plugged in and charging: a charging battery is never low.
    #[cfg_attr(feature = "uniffi", uniffi(default = false))]
    pub battery_charging: bool,
}

/// The largest cadence factor: the flush interval and stats window stretch at most this much.
pub(crate) const MAX_CADENCE_FACTOR: u32 = 4;

/// How much the battery level matters; per-percent updates only count when they cross a bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BatteryBucket {
    /// Charging, unknown, or above 20 %.
    Fine,
    /// At most 20 % and unplugged: the cadence doubles.
    Low,
    /// At most 10 % and unplugged: uploads hold.
    NearlyEmpty,
}

impl DeviceState {
    /// Below this, unplugged, the pipeline still records but stops uploading (a soft hold).
    const BATTERY_HOLD_PERCENT: u32 = 10;
    /// Below this, unplugged, the cadence doubles.
    const BATTERY_LOW_PERCENT: u32 = 20;

    /// Multiplier on the flush interval and stats window: 1× at rest, up to
    /// [`MAX_CADENCE_FACTOR`] under pressure (60 s → 4 min at the default cadence). Serious
    /// thermal, memory warning, low-power mode, background, low battery and a constrained network
    /// each double it; critical thermal or memory quadruple it.
    pub(crate) fn cadence_factor(&self) -> u32 {
        let thermal = match self.thermal {
            ThermalState::Unknown | ThermalState::Nominal | ThermalState::Fair => 1,
            ThermalState::Serious => 2,
            ThermalState::Critical => 4,
        };
        let memory = match self.memory {
            MemoryPressure::Normal => 1,
            MemoryPressure::Warning => 2,
            MemoryPressure::Critical => 4,
        };
        let doubled = [
            self.low_power_mode == Some(true),
            self.app_state == AppState::Background,
            self.battery_bucket() != BatteryBucket::Fine,
            self.network_constrained,
        ]
        .iter()
        .filter(|&&on| on)
        .count() as u32;
        ((thermal * memory) << doubled).min(MAX_CADENCE_FACTOR)
    }

    /// A soft hold: the user asked for less traffic (Low Data Mode / Data Saver) or the battery
    /// is nearly empty and unplugged. Uploads wait (at most a minute at a time); data keeps
    /// flowing into the cache meanwhile. Being offline (`network: Unavailable`) is a hard hold:
    /// nothing is attempted until the network is back.
    pub(crate) fn holds_uploads(&self) -> bool {
        self.network_constrained || self.battery_bucket() == BatteryBucket::NearlyEmpty
    }

    /// Where the battery level stands; always [`BatteryBucket::Fine`] on charge.
    fn battery_bucket(&self) -> BatteryBucket {
        match self.battery_level {
            Some(level) if !self.battery_charging && level <= Self::BATTERY_HOLD_PERCENT => {
                BatteryBucket::NearlyEmpty
            }
            Some(level) if !self.battery_charging && level <= Self::BATTERY_LOW_PERCENT => {
                BatteryBucket::Low
            }
            _ => BatteryBucket::Fine,
        }
    }

    /// The `SPEC.md` events describing what changed between `previous` and `self` (every known
    /// field, when there is no previous state: "initial value at session start"). Each event
    /// carries a display body that repeats its values: the body takes the place of the name a
    /// body-less event would show (see `SPEC.md`, Events), so it costs about what that name copy
    /// would.
    pub(crate) fn change_events(&self, previous: Option<&DeviceState>) -> Vec<TelemetryEvent> {
        let mut events = Vec::new();
        // An unknown reading is no reading: it never makes a change event.
        if self.thermal != ThermalState::Unknown
            && previous.is_none_or(|p| p.thermal != self.thermal)
        {
            events.push(
                TelemetryEvent::new("lk.device.thermal.changed")
                    .with_body(format!("thermal: {}", self.thermal.as_str()))
                    .with_attribute("lk.device.thermal.state", self.thermal.as_str()),
            );
        }
        if let Some(low_power) = self
            .low_power_mode
            .filter(|_| previous.is_none_or(|p| p.low_power_mode != self.low_power_mode))
        {
            events.push(
                TelemetryEvent::new("lk.device.low_power.changed")
                    .with_body(format!("low power mode: {}", if low_power { "on" } else { "off" }))
                    .with_attribute("lk.device.low_power.enabled", low_power),
            );
        }
        if previous.is_none_or(|p| p.app_state != self.app_state) {
            events.push(
                TelemetryEvent::new("lk.device.app_state.changed")
                    .with_body(format!("app: {}", self.app_state.as_str()))
                    .with_attribute("lk.device.app_state", self.app_state.as_str()),
            );
        }
        if previous.is_none_or(|p| p.memory != self.memory) {
            events.push(
                TelemetryEvent::new("lk.device.memory.changed")
                    .with_body(format!("memory pressure: {}", self.memory.as_str()))
                    .with_attribute("lk.device.memory.pressure", self.memory.as_str()),
            );
        }
        if previous.is_none_or(|p| {
            (p.network, p.network_expensive, p.network_constrained)
                != (self.network, self.network_expensive, self.network_constrained)
        }) {
            events.push(
                TelemetryEvent::new("lk.device.network.changed")
                    .with_body(format!(
                        "network: {}{}{}",
                        self.network.as_str(),
                        if self.network_expensive { ", expensive" } else { "" },
                        if self.network_constrained { ", constrained" } else { "" }
                    ))
                    .with_attribute("network.connection.type", self.network.as_str())
                    .with_attribute("lk.device.network.expensive", self.network_expensive)
                    .with_attribute("lk.device.network.constrained", self.network_constrained),
            );
        }
        // Per-percent updates are noise; report charging flips and bucket crossings.
        if let Some(level) = self.battery_level.filter(|_| {
            previous.is_none_or(|p| {
                p.battery_charging != self.battery_charging
                    || p.battery_bucket() != self.battery_bucket()
                    || p.battery_level.is_none()
            })
        }) {
            events.push(
                TelemetryEvent::new("lk.device.battery.changed")
                    .with_body(format!(
                        "battery: {level}%{}",
                        if self.battery_charging { ", charging" } else { "" }
                    ))
                    .with_attribute("lk.device.battery.level", i64::from(level))
                    .with_attribute("lk.device.battery.charging", self.battery_charging),
            );
        }
        events
    }
}

impl ThermalState {
    fn as_str(self) -> &'static str {
        match self {
            ThermalState::Unknown => "unknown",
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
            NetworkType::Vpn => "vpn",
            NetworkType::Bluetooth => "bluetooth",
            NetworkType::Other => "other",
            NetworkType::Unavailable => "unavailable",
        }
    }
}

/// Why the audio route changed, in platform-neutral terms (AVAudioSession reasons; Android's
/// device added/removed callbacks map to `NewDevice` / `OldDeviceUnavailable`).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRouteReason {
    /// An audio device became available, such as plugged-in headphones; sent as `new_device`.
    NewDevice,
    /// The audio device in use went away; sent as `old_device_unavailable`.
    OldDeviceUnavailable,
    /// The audio session category changed; sent as `category_change`.
    CategoryChange,
    /// The app overrode the route, such as forcing the speaker; sent as `override`.
    Override,
    /// The device woke from sleep; sent as `wake_from_sleep`.
    WakeFromSleep,
    /// No route suits the audio session category; sent as `no_suitable_route`.
    NoSuitableRoute,
    /// The configuration of the route's ports changed; sent as `route_configuration_change`.
    RouteConfigurationChange,
    /// The platform gives no reason; sent as `unknown`.
    Unknown,
}

/// Where the audio goes, in platform-neutral terms (AVAudioSession port types, Android's
/// `AudioDevice`; the web's `MediaDeviceInfo.kind` only ever gives `other`).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioOutput {
    /// The built-in loudspeaker; sent as `speaker`.
    Speaker,
    /// The phone earpiece (`builtInReceiver`, Android `Earpiece`).
    Receiver,
    /// Wired headphones or a wired headset; sent as `wired_headset`.
    WiredHeadset,
    /// A Bluetooth headset, headphones or speaker; sent as `bluetooth`.
    Bluetooth,
    /// A car's audio system; sent as `car_audio`.
    CarAudio,
    /// An AirPlay receiver; sent as `air_play`.
    AirPlay,
    /// An HDMI display or receiver; sent as `hdmi`.
    Hdmi,
    /// A USB audio device; sent as `usb`.
    Usb,
    /// An output the platform does not name; sent as `other`.
    Other,
}

/// Which capture device a failure is about.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureDevice {
    /// A camera; sent as `camera`.
    Camera,
    /// A microphone; sent as `microphone`.
    Microphone,
    /// Screen capture; sent as `screen_share`.
    ScreenShare,
}

/// Why a capture device could not be used: the `getUserMedia` failure taxonomy, which every
/// platform maps onto (authorization status, `onCameraDisconnected`, `NotReadableError`, …).
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFailure {
    /// The user (or MDM) said no: the most common pre-connect failure on mobile.
    PermissionDenied,
    /// No such device exists (`NotFoundError`); sent as `not_found`.
    NotFound,
    /// Another app holds the device (`NotReadableError`, `TrackStartError`).
    InUse,
    /// The device went away mid-capture (unplugged, `onCameraDisconnected`).
    Disconnected,
    /// Any other failure; sent as `other`.
    Other,
}

/// Things that happen to the device mid-call and explain what the media did next. Not state
/// (see [`DeviceState`]): each one is a record with a display body.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The audio output route changed.
    AudioRouteChanged {
        /// The outputs now in use, in platform order; sent in `lk.device.audio_route.outputs`.
        outputs: Vec<AudioOutput>,
        /// Why the route changed; sent in `lk.device.audio_route.reason`.
        reason: AudioRouteReason,
    },
    /// A phone call, Siri, another app taking audio focus: `began`, then the matching `ended`.
    AudioInterruption {
        /// Began (`true`) or ended (`false`); sent in `lk.device.audio.interruption`.
        began: bool,
    },
    /// A capture device could not be, or stay, used.
    CaptureFailed {
        /// The device that failed; sent in `lk.device.capture.device`.
        device: CaptureDevice,
        /// Why it failed; sent in `lk.device.capture.reason`.
        reason: CaptureFailure,
    },
}

/// A variant's wire value: its `Debug` name in snake case (`OldDeviceUnavailable` →
/// `old_device_unavailable`). Variant names are therefore the wire contract: renaming a variant
/// changes what ships, and the tests pin every value to `SPEC.md`.
pub(crate) fn snake_case(debug: impl std::fmt::Debug) -> String {
    let mut out = String::new();
    for (i, c) in format!("{debug:?}").chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

impl From<DeviceEvent> for TelemetryEvent {
    /// The `SPEC.md` event for a device event, with its display body.
    fn from(event: DeviceEvent) -> Self {
        match event {
            DeviceEvent::AudioRouteChanged { outputs, reason } => {
                let outputs = outputs.into_iter().map(snake_case).collect::<Vec<_>>().join(",");
                let reason = snake_case(reason);
                TelemetryEvent::new("lk.device.audio_route.changed")
                    .with_body(format!("audio route: {outputs} ({reason})"))
                    .with_attribute("lk.device.audio_route.outputs", outputs)
                    .with_attribute("lk.device.audio_route.reason", reason)
            }
            DeviceEvent::AudioInterruption { began } => {
                let phase = if began { "began" } else { "ended" };
                TelemetryEvent::new("lk.device.audio.interruption")
                    .with_body(format!("audio interruption {phase}"))
                    .with_attribute("lk.device.audio.interruption", phase)
            }
            DeviceEvent::CaptureFailed { device, reason } => {
                let (device, reason) = (snake_case(device), snake_case(reason));
                TelemetryEvent::new("lk.device.capture.failed")
                    .with_severity(crate::Severity::Warn)
                    .with_body(format!("capture failed: {device} ({reason})"))
                    .with_attribute("lk.device.capture.device", device)
                    .with_attribute("lk.device.capture.reason", reason)
            }
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
        let low_power = DeviceState { low_power_mode: Some(true), ..Default::default() };
        assert_eq!(low_power.cadence_factor(), 2);
        let tight = DeviceState { memory: MemoryPressure::Warning, ..Default::default() };
        assert_eq!(tight.cadence_factor(), 2);
        let low_battery = DeviceState { battery_level: Some(15), ..Default::default() };
        assert_eq!(low_battery.cadence_factor(), 2);
        let charging = DeviceState { battery_charging: true, ..low_battery };
        assert_eq!(charging.cadence_factor(), 1, "a charging battery is not low");
        let everything = DeviceState {
            thermal: ThermalState::Critical,
            low_power_mode: Some(true),
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
        let initial = DeviceState {
            thermal: ThermalState::Nominal,
            low_power_mode: Some(false),
            ..Default::default()
        };
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

    /// A platform without a thermal or low-power source reports them unknown — never a made-up
    /// nominal/off: no change event, no stretch, no hold.
    #[test]
    fn unknown_thermal_and_low_power_are_silent_and_neutral() {
        let unknown = DeviceState::default();
        assert_eq!((unknown.thermal, unknown.low_power_mode), (ThermalState::Unknown, None));
        let names: Vec<_> = unknown.change_events(None).into_iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            [
                "lk.device.app_state.changed",
                "lk.device.memory.changed",
                "lk.device.network.changed"
            ]
        );
        assert_eq!(unknown.cadence_factor(), 1);
        assert!(!unknown.holds_uploads());
        let known = DeviceState {
            thermal: ThermalState::Serious,
            low_power_mode: Some(true),
            ..Default::default()
        };
        assert!(unknown.change_events(Some(&known)).is_empty(), "losing the source is silent");
        assert_eq!(known.change_events(Some(&unknown)).len(), 2, "a first real reading reports");
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

    #[test]
    fn battery_events_carry_the_level_and_the_charging_flag() {
        let low = DeviceState { battery_level: Some(15), ..Default::default() };
        let events = low.change_events(Some(&DeviceState::default()));
        let [event] = &events[..] else { panic!("one battery event: {events:?}") };
        let value = |key: &str| event.attributes.iter().find(|a| a.key == key).map(|a| &a.value);
        assert_eq!(value("lk.device.battery.level"), Some(&crate::AttributeValue::Int(15)));
        assert_eq!(value("lk.device.battery.charging"), Some(&crate::AttributeValue::Bool(false)));
    }

    #[test]
    fn the_core_owns_the_network_connection_type() {
        assert!(!crate::event::valid_custom("network.connection.type", None));
        assert!(crate::event::valid_custom("network.peer.address", None), "other `network.*`");
    }

    /// Every enum value that ships, pinned to the lists in `SPEC.md`.
    #[test]
    fn wire_values_match_the_spec() {
        let names = |values: Vec<String>| values.join(" | ");
        use AudioRouteReason as R;
        let reasons = [
            R::NewDevice,
            R::OldDeviceUnavailable,
            R::CategoryChange,
            R::Override,
            R::WakeFromSleep,
            R::NoSuitableRoute,
            R::RouteConfigurationChange,
            R::Unknown,
        ];
        assert_eq!(
            names(reasons.into_iter().map(snake_case).collect()),
            "new_device | old_device_unavailable | category_change | override | wake_from_sleep \
             | no_suitable_route | route_configuration_change | unknown"
        );
        use AudioOutput as O;
        let outputs = [
            O::Speaker,
            O::Receiver,
            O::WiredHeadset,
            O::Bluetooth,
            O::CarAudio,
            O::AirPlay,
            O::Hdmi,
            O::Usb,
            O::Other,
        ];
        assert_eq!(
            names(outputs.into_iter().map(snake_case).collect()),
            "speaker | receiver | wired_headset | bluetooth | car_audio | air_play | hdmi | usb \
             | other"
        );
        let devices =
            [CaptureDevice::Camera, CaptureDevice::Microphone, CaptureDevice::ScreenShare];
        assert_eq!(
            names(devices.into_iter().map(snake_case).collect()),
            "camera | microphone | screen_share"
        );
        use CaptureFailure as F;
        let failures = [F::PermissionDenied, F::NotFound, F::InUse, F::Disconnected, F::Other];
        assert_eq!(
            names(failures.into_iter().map(snake_case).collect()),
            "permission_denied | not_found | in_use | disconnected | other"
        );
    }

    #[test]
    fn device_events_have_bodies_and_snake_case_values() {
        let event: TelemetryEvent = DeviceEvent::AudioRouteChanged {
            outputs: vec![AudioOutput::Speaker],
            reason: AudioRouteReason::OldDeviceUnavailable,
        }
        .into();
        assert_eq!(event.body.as_deref(), Some("audio route: speaker (old_device_unavailable)"));
        let denied: TelemetryEvent = DeviceEvent::CaptureFailed {
            device: CaptureDevice::ScreenShare,
            reason: CaptureFailure::PermissionDenied,
        }
        .into();
        assert_eq!(
            denied.body.as_deref(),
            Some("capture failed: screen_share (permission_denied)")
        );
        assert_eq!(denied.severity, crate::Severity::Warn);
    }
}
