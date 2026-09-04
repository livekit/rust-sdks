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
    /// The RTP stream this sample describes when a track has several (simulcast layers): any
    /// stable id (`rid`, ssrc, the stats id). The core folds layers into the track; a platform
    /// never sums them itself.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub layer: Option<String>,
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
            layer: None,
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
    /// Cumulative counters at the window's first reading, for the display body's deltas.
    first: RtcStatsSample,
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
            first: first.clone(),
            last: first.clone(),
            jitter: Gauge::default(),
            rtt: Gauge::default(),
            fps: Gauge::default(),
            audio_level: Gauge::default(),
        };
        window.add(first);
        window
    }

    /// `video outbound: 1204 kbps, loss 0.4%, rtt 48 ms, 30 fps` — deltas over the window.
    fn summary(&self, window_ms: u64) -> String {
        let mut parts = Vec::new();
        let delta = |a: Option<u64>, b: Option<u64>| Some(a?.saturating_sub(b?));
        if let (Some(bytes), true) = (delta(self.last.bytes, self.first.bytes), window_ms > 0) {
            parts.push(format!("{} kbps", bytes * 8 / window_ms));
        }
        if let (Some(lost), Some(packets)) = (
            delta(self.last.packets_lost, self.first.packets_lost),
            delta(self.last.packets, self.first.packets),
        ) {
            if lost + packets > 0 {
                parts.push(format!("loss {:.1}%", lost as f64 * 100.0 / (lost + packets) as f64));
            }
        }
        if self.rtt.n > 0 {
            parts.push(format!("rtt {:.0} ms", self.rtt.sum / self.rtt.n as f64));
        }
        if self.fps.n > 0 {
            parts.push(format!("{:.0} fps", self.fps.sum / self.fps.n as f64));
        }
        if let Some(freezes) =
            delta(self.last.freeze_count, self.first.freeze_count).filter(|f| *f > 0)
        {
            parts.push(format!("{freezes} freezes"));
        }
        format!(
            "{} {}: {}",
            self.last.kind.as_str(),
            self.last.direction.as_str(),
            if parts.is_empty() { "no data".to_owned() } else { parts.join(", ") }
        )
    }

    fn add(&mut self, sample: RtcStatsSample) {
        self.samples += 1;
        self.jitter.add(sample.jitter_ms);
        self.rtt.add(sample.rtt_ms);
        self.fps.add(sample.frames_per_second);
        self.audio_level.add(sample.audio_level);
        self.last = sample;
    }

    /// The `lk.rtc.stats.sample` event for this window, stamped at `end_ns`. Its body is the
    /// one-line human summary a log view shows (OTel: an event's body is its display message);
    /// the attributes carry the numbers.
    fn into_event(self, end_ns: u64) -> TelemetryEvent {
        let window_ms = end_ns.saturating_sub(self.start_ns) / 1_000_000;
        let body = self.summary(window_ms);
        let last = self.last;
        let mut event = TelemetryEvent::new("lk.rtc.stats.sample")
            .with_body(body)
            .with_attribute("lk.track.sid", last.track_sid)
            .with_attribute("lk.track.kind", last.kind.as_str())
            .with_attribute("lk.track.direction", last.direction.as_str())
            .with_attribute("lk.rtc.window_ms", window_ms as i64)
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
    /// Last cumulative `qualityLimitationDurations.cpu` per outbound track.
    // ponytail: grows with the tracks published in a session (a few dozen at most).
    limitation: HashMap<String, u64>,
    cpu_limited_until: Option<Instant>,
    /// Last cumulative counters per simulcast layer, per outbound track: the track's counters
    /// are their sum, so a suspended top layer cannot freeze them.
    layers: HashMap<(String, StreamDirection), HashMap<String, LayerCounters>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct LayerCounters {
    bytes: Option<u64>,
    packets: Option<u64>,
    fps: Option<f64>,
    limitation_bandwidth_ms: Option<u64>,
    limitation_cpu_ms: Option<u64>,
}

fn sum(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.flatten().reduce(|a, b| a + b)
}

fn max_u64(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.flatten().max()
}

/// CPU starvation is sticky: stretch the cadence for a while after the last sign of it.
/// `qualityLimitationDurations.bandwidth` is deliberately not a signal: WebRTC reports it for
/// minutes during a normal ramp-up and for as long as an encoder stalls (an iPhone camera at
/// 0 kbps held uploads for 8 minutes); the congestion controller does not need our help.
const CPU_LIMITED_HOLD: Duration = Duration::from_secs(60);

impl StatsWindows {
    pub fn record_in(&mut self, mut sample: RtcStatsSample, session: &Arc<SessionState>) {
        self.fold_layers(&mut sample);
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

    /// Simulcast publishes one RTP stream per layer under one track sid. Remember each layer's
    /// latest cumulative counters and rewrite the sample as the track's total, so the window
    /// sees one monotonic series per track.
    fn fold_layers(&mut self, sample: &mut RtcStatsSample) {
        let Some(layer) = sample.layer.clone() else { return };
        let layers = self.layers.entry((sample.track_sid.clone(), sample.direction)).or_default();
        layers.insert(
            layer,
            LayerCounters {
                bytes: sample.bytes,
                packets: sample.packets,
                fps: sample.frames_per_second,
                limitation_bandwidth_ms: sample.quality_limitation_bandwidth_ms,
                limitation_cpu_ms: sample.quality_limitation_cpu_ms,
            },
        );
        sample.bytes = sum(layers.values().map(|l| l.bytes));
        sample.packets = sum(layers.values().map(|l| l.packets));
        sample.frames_per_second = layers
            .values()
            .filter_map(|l| l.fps)
            .fold(None, |m, v| Some(m.map_or(v, |m: f64| m.max(v))));
        sample.quality_limitation_bandwidth_ms =
            max_u64(layers.values().map(|l| l.limitation_bandwidth_ms));
        sample.quality_limitation_cpu_ms = max_u64(layers.values().map(|l| l.limitation_cpu_ms));
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

    /// The encoder was CPU-starved within the last minute: stretch the cadence, like thermal
    /// pressure.
    pub fn cpu_limited(&self) -> bool {
        self.cpu_limited_until.is_some_and(|t| Instant::now() < t)
    }

    /// A cpu limitation counter that grew since the previous reading means the encoder was
    /// starved in between.
    fn track_limitation(&mut self, sample: &RtcStatsSample) {
        if sample.direction != StreamDirection::Outbound {
            return;
        }
        let current = sample.quality_limitation_cpu_ms.unwrap_or(0);
        if let Some(previous) = self.limitation.insert(sample.track_sid.clone(), current) {
            if current > previous {
                self.cpu_limited_until = Some(Instant::now() + CPU_LIMITED_HOLD);
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

    #[test]
    fn simulcast_layers_fold_into_one_track_series() {
        let mut windows = StatsWindows::default();
        let layer = |id: &str, bytes: u64, fps: f64| RtcStatsSample {
            bytes: Some(bytes),
            frames_per_second: Some(fps),
            layer: Some(id.into()),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        windows.record(layer("h", 1_000, 30.0));
        windows.record(layer("f", 4_000, 30.0));
        // The top layer stalls; the half layer keeps sending.
        windows.record(layer("h", 3_000, 30.0));
        windows.record(layer("f", 4_000, 0.0));
        let events = windows.close_events();
        assert_eq!(events.len(), 1, "one window per track, not per layer");
        let bytes =
            events[0].attributes.iter().find(|a| a.key == "lk.rtc.bytes").map(|a| a.value.clone());
        assert_eq!(bytes, Some(AttributeValue::Int(7_000)), "last folded total");
        let body = events[0].body.clone().unwrap_or_default();
        assert!(!body.contains(" 0 kbps"), "a stalled layer is not a stalled track: {body}");
    }

    #[tokio::test(start_paused = true)]
    async fn cpu_limitation_counter_drives_cadence_pressure() {
        let mut windows = StatsWindows::default();
        let reading = |bandwidth_ms, cpu_ms| RtcStatsSample {
            quality_limitation_bandwidth_ms: Some(bandwidth_ms),
            quality_limitation_cpu_ms: Some(cpu_ms),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        windows.record(reading(0, 0));
        assert!(!windows.cpu_limited(), "first reading: no delta");
        windows.record(reading(5_000, 0));
        assert!(!windows.cpu_limited(), "bandwidth limitation is not pressure");
        windows.record(reading(5_000, 250));
        assert!(windows.cpu_limited());
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(windows.cpu_limited(), "cpu hold is sticky");
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(!windows.cpu_limited(), "and expires");
        let mut inbound = RtcStatsSample::new("TR_2", TrackKind::Audio, StreamDirection::Inbound);
        inbound.quality_limitation_cpu_ms = Some(9_999);
        windows.record(inbound.clone());
        windows.record(inbound);
        assert!(!windows.cpu_limited(), "inbound counters are ignored");
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
