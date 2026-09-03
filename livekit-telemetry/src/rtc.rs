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

use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::time::Instant;

use crate::{event::now_unix_nanos, session::SessionState, store::Queued, TelemetryEvent};

#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Audio,
    Video,
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamDirection {
    Inbound,
    Outbound,
}

/// One reading of a track's RTP statistics, as `getStats()` reports them.
///
/// Cumulative counters (`bytes`, `packets`, `freeze_count`, …) are passed through as reported —
/// monotonic, paired with their denominators (the W3C webrtc-stats model) so any layer can
/// recompute rates and a dropped window never corrupts the next. Gauges (`jitter_ms`, `rtt_ms`,
/// `frames_per_second`, `audio_level`) are summarised per window as min/max/avg. Fields a
/// platform or direction does not have stay `None` and are omitted from the wire.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct RtcStatsSample {
    pub track_sid: String,
    pub kind: TrackKind,
    pub direction: StreamDirection,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub codec: Option<String>,
    // Cumulative counters.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub bytes: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub packets: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub packets_lost: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub freeze_count: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub freezes_duration_ms: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub concealed_samples: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub concealment_events: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_buffer_delay_ms: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_buffer_emitted_count: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_bandwidth_ms: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_cpu_ms: Option<u64>,
    // Gauges.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_ms: Option<f64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub rtt_ms: Option<f64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub frames_per_second: Option<f64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub audio_level: Option<f64>,
    /// When the reading was taken; `None` = now.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub timestamp_ns: Option<u64>,
}

impl RtcStatsSample {
    /// A sample with every optional field unset.
    pub fn new(track_sid: impl Into<String>, kind: TrackKind, direction: StreamDirection) -> Self {
        Self {
            track_sid: track_sid.into(),
            kind,
            direction,
            codec: None,
            bytes: None,
            packets: None,
            packets_lost: None,
            freeze_count: None,
            freezes_duration_ms: None,
            concealed_samples: None,
            concealment_events: None,
            jitter_buffer_delay_ms: None,
            jitter_buffer_emitted_count: None,
            quality_limitation_bandwidth_ms: None,
            quality_limitation_cpu_ms: None,
            jitter_ms: None,
            rtt_ms: None,
            frames_per_second: None,
            audio_level: None,
            timestamp_ns: None,
        }
    }
}

/// min / max / avg of a gauge over a window.
#[derive(Debug, Default, Clone, Copy)]
struct Gauge {
    min: f64,
    max: f64,
    sum: f64,
    n: u32,
}

impl Gauge {
    fn add(&mut self, value: Option<f64>) {
        let Some(value) = value else { return };
        if self.n == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.sum += value;
        self.n += 1;
    }

    fn attach(&self, event: TelemetryEvent, key: &str) -> TelemetryEvent {
        if self.n == 0 {
            return event;
        }
        event
            .with_attribute(format!("{key}.min"), self.min)
            .with_attribute(format!("{key}.max"), self.max)
            .with_attribute(format!("{key}.avg"), self.sum / self.n as f64)
    }
}

/// Samples of one track in one direction accumulated since the window opened.
struct Window {
    session: Arc<SessionState>,
    start_ns: u64,
    samples: u32,
    last: RtcStatsSample,
    jitter: Gauge,
    rtt: Gauge,
    fps: Gauge,
    audio_level: Gauge,
}

impl Window {
    fn open(start_ns: u64, first: RtcStatsSample, session: Arc<SessionState>) -> Self {
        let mut window = Self {
            session,
            start_ns,
            samples: 0,
            last: first.clone(),
            jitter: Gauge::default(),
            rtt: Gauge::default(),
            fps: Gauge::default(),
            audio_level: Gauge::default(),
        };
        window.add(first);
        window
    }

    fn add(&mut self, sample: RtcStatsSample) {
        self.samples += 1;
        self.jitter.add(sample.jitter_ms);
        self.rtt.add(sample.rtt_ms);
        self.fps.add(sample.frames_per_second);
        self.audio_level.add(sample.audio_level);
        self.last = sample;
    }

    /// The `lk.rtc.stats.sample` event for this window, stamped at `end_ns`.
    fn into_event(self, end_ns: u64) -> TelemetryEvent {
        let last = self.last;
        let mut event = TelemetryEvent::new("lk.rtc.stats.sample")
            .with_attribute("lk.track.sid", last.track_sid)
            .with_attribute("lk.track.kind", last.kind.as_str())
            .with_attribute("lk.track.direction", last.direction.as_str())
            .with_attribute(
                "lk.rtc.window_ms",
                (end_ns.saturating_sub(self.start_ns) / 1_000_000) as i64,
            )
            .with_attribute("lk.rtc.samples", self.samples as i64);
        if let Some(codec) = last.codec {
            event = event.with_attribute("lk.rtc.codec", codec);
        }
        let counters = [
            ("lk.rtc.bytes", last.bytes),
            ("lk.rtc.packets", last.packets),
            ("lk.rtc.packets_lost", last.packets_lost),
            ("lk.rtc.freeze_count", last.freeze_count),
            ("lk.rtc.freezes_duration_ms", last.freezes_duration_ms),
            ("lk.rtc.concealed_samples", last.concealed_samples),
            ("lk.rtc.concealment_events", last.concealment_events),
            ("lk.rtc.jitter_buffer_delay_ms", last.jitter_buffer_delay_ms),
            ("lk.rtc.jitter_buffer_emitted_count", last.jitter_buffer_emitted_count),
            ("lk.rtc.quality_limitation.bandwidth_ms", last.quality_limitation_bandwidth_ms),
            ("lk.rtc.quality_limitation.cpu_ms", last.quality_limitation_cpu_ms),
        ];
        for (key, value) in counters {
            if let Some(value) = value {
                event = event.with_attribute(key, value as i64);
            }
        }
        event = self.jitter.attach(event, "lk.rtc.jitter_ms");
        event = self.rtt.attach(event, "lk.rtc.rtt_ms");
        event = self.fps.attach(event, "lk.rtc.fps");
        event = self.audio_level.attach(event, "lk.rtc.audio_level");
        event.timestamp_ns = Some(end_ns);
        event
    }
}

/// Open RTC stats windows, one per track and direction.
///
/// The platform pushes raw `getStats()` readings every 1–2 s; the core ships one
/// `lk.rtc.stats.sample` per window (15 s by default, stretched under device pressure) — on
/// device: 1 Hz raw sampling across ~100k concurrent participants would be the same ~100k
/// records/s fleet-wide.
#[derive(Default)]
pub(crate) struct StatsWindows {
    windows: HashMap<(String, StreamDirection), Window>,
    /// Last cumulative `qualityLimitationDurations` per outbound track: (bandwidth_ms, cpu_ms).
    // ponytail: grows with the tracks published in a session (a few dozen at most).
    limitation: HashMap<String, (u64, u64)>,
    bandwidth_limited_until: Option<Instant>,
    cpu_limited_until: Option<Instant>,
}

/// What WebRTC's own adaptation says about the call, derived from the
/// `qualityLimitationDurations` counters of outbound tracks — no extra measurement, and exactly
/// the cpu-vs-bandwidth split the design doc asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MediaPressure {
    /// The congestion controller is holding the encoder back: uploads must not add to the uplink.
    pub bandwidth_limited: bool,
    /// The encoder is CPU-starved: stretch the cadence, like thermal pressure.
    pub cpu_limited: bool,
}

/// Congestion is bursty: hold uploads for a short while after the last sign of it.
const BANDWIDTH_LIMITED_HOLD: Duration = Duration::from_secs(10);
/// CPU starvation is sticky: stretch the cadence for a while after the last sign of it.
const CPU_LIMITED_HOLD: Duration = Duration::from_secs(60);

impl StatsWindows {
    pub fn record_in(&mut self, mut sample: RtcStatsSample, session: &Arc<SessionState>) {
        self.track_limitation(&sample);
        let timestamp = *sample.timestamp_ns.get_or_insert_with(now_unix_nanos);
        let key = (sample.track_sid.clone(), sample.direction);
        match self.windows.get_mut(&key) {
            Some(window) => window.add(sample),
            None => {
                self.windows.insert(key, Window::open(timestamp, sample, session.clone()));
            }
        }
    }

    /// Close every open window into its event, filed under the window's session, and start fresh.
    pub fn close(&mut self) -> Vec<Queued> {
        let end = now_unix_nanos();
        self.windows
            .drain()
            .map(|(_, window)| {
                let session = window.session.clone();
                Queued { event: window.into_event(end), session }
            })
            .collect()
    }

    #[cfg(test)]
    pub fn record(&mut self, sample: RtcStatsSample) {
        self.record_in(sample, &SessionState::new());
    }

    #[cfg(test)]
    pub fn close_events(&mut self) -> Vec<TelemetryEvent> {
        self.close().into_iter().map(|q| q.event).collect()
    }

    pub fn media_pressure(&self) -> MediaPressure {
        let now = Instant::now();
        MediaPressure {
            bandwidth_limited: self.bandwidth_limited_until.is_some_and(|t| now < t),
            cpu_limited: self.cpu_limited_until.is_some_and(|t| now < t),
        }
    }

    /// A limitation counter that grew since the previous reading means the encoder was held
    /// back in between.
    fn track_limitation(&mut self, sample: &RtcStatsSample) {
        if sample.direction != StreamDirection::Outbound {
            return;
        }
        let current = (
            sample.quality_limitation_bandwidth_ms.unwrap_or(0),
            sample.quality_limitation_cpu_ms.unwrap_or(0),
        );
        if let Some(previous) = self.limitation.insert(sample.track_sid.clone(), current) {
            let now = Instant::now();
            if current.0 > previous.0 {
                self.bandwidth_limited_until = Some(now + BANDWIDTH_LIMITED_HOLD);
            }
            if current.1 > previous.1 {
                self.cpu_limited_until = Some(now + CPU_LIMITED_HOLD);
            }
        }
    }
}

impl TrackKind {
    fn as_str(self) -> &'static str {
        match self {
            TrackKind::Audio => "audio",
            TrackKind::Video => "video",
        }
    }
}

impl StreamDirection {
    fn as_str(self) -> &'static str {
        match self {
            StreamDirection::Inbound => "inbound",
            StreamDirection::Outbound => "outbound",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn limitation_counters_drive_media_pressure() {
        let mut windows = StatsWindows::default();
        let reading = |bandwidth_ms, cpu_ms| RtcStatsSample {
            quality_limitation_bandwidth_ms: Some(bandwidth_ms),
            quality_limitation_cpu_ms: Some(cpu_ms),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        windows.record(reading(0, 0));
        assert_eq!(windows.media_pressure(), MediaPressure::default(), "first reading: no delta");
        windows.record(reading(500, 0));
        assert!(windows.media_pressure().bandwidth_limited);
        assert!(!windows.media_pressure().cpu_limited);
        tokio::time::advance(Duration::from_secs(11)).await;
        assert!(!windows.media_pressure().bandwidth_limited, "congestion hold expires");
        windows.record(reading(500, 250));
        assert!(windows.media_pressure().cpu_limited);
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(windows.media_pressure().cpu_limited, "cpu hold is sticky");
        let mut inbound = RtcStatsSample::new("TR_2", TrackKind::Audio, StreamDirection::Inbound);
        inbound.quality_limitation_bandwidth_ms = Some(9_999);
        windows.record(inbound.clone());
        windows.record(inbound);
        assert!(!windows.media_pressure().bandwidth_limited, "inbound counters are ignored");
    }
    use crate::AttributeValue;

    fn attr(event: &TelemetryEvent, key: &str) -> Option<AttributeValue> {
        event.attributes.iter().find(|a| a.key == key).map(|a| a.value.clone())
    }

    #[test]
    fn window_keeps_last_counter_and_summarises_gauges() {
        let mut windows = StatsWindows::default();
        for (bytes, jitter) in [(100, 1.0), (200, 3.0), (300, 2.0)] {
            let mut sample =
                RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Inbound);
            sample.bytes = Some(bytes);
            sample.jitter_ms = Some(jitter);
            windows.record(sample);
        }
        let mut other = RtcStatsSample::new("TR_1", TrackKind::Audio, StreamDirection::Outbound);
        other.packets = Some(7);
        windows.record(other);

        let mut events = windows.close_events();
        assert!(windows.close_events().is_empty(), "closing again yields nothing");
        events.sort_by_key(|e| attr(e, "lk.track.direction").map(|v| format!("{v:?}")));
        assert_eq!(events.len(), 2, "one event per track and direction");
        let inbound = &events[0];
        assert_eq!(inbound.name, "lk.rtc.stats.sample");
        assert_eq!(
            attr(inbound, "lk.track.direction"),
            Some(AttributeValue::Str("inbound".into()))
        );
        assert_eq!(
            attr(inbound, "lk.rtc.bytes"),
            Some(AttributeValue::Int(300)),
            "cumulative: last value"
        );
        assert_eq!(attr(inbound, "lk.rtc.samples"), Some(AttributeValue::Int(3)));
        assert_eq!(attr(inbound, "lk.rtc.jitter_ms.min"), Some(AttributeValue::Double(1.0)));
        assert_eq!(attr(inbound, "lk.rtc.jitter_ms.max"), Some(AttributeValue::Double(3.0)));
        assert_eq!(attr(inbound, "lk.rtc.jitter_ms.avg"), Some(AttributeValue::Double(2.0)));
        assert_eq!(attr(inbound, "lk.rtc.rtt_ms.avg"), None, "absent gauges are omitted");
        assert_eq!(attr(&events[1], "lk.rtc.packets"), Some(AttributeValue::Int(7)));
    }
}
