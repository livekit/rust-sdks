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

use crate::{
    event::now_unix_nanos, scope::ScopeState, store::Queued, Attribute, AttributeValue,
    TelemetryEvent,
};

/// Audio or video.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Audio,
    Video,
}

/// Received (inbound) or sent (outbound) media.
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
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_other_ms: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub pause_count: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub pauses_duration_ms: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub silent_concealed_samples: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub interruption_count: Option<u64>,
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub interruptions_duration_ms: Option<u64>,
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

/// One entry of a WebRTC `RTCStatsReport`, as the platform got it: the entry's `type`, `id` and
/// its standard members (W3C webrtc-stats names; nested maps flattened with a dot, e.g.
/// `qualityLimitationDurations.cpu`). Numbers may arrive as `Int`, `Double` or numeric `Str`.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct RtcStat {
    pub kind: String,
    pub id: String,
    pub members: HashMap<String, AttributeValue>,
}

fn num(stat: &RtcStat, key: &str) -> Option<f64> {
    match stat.members.get(key)? {
        AttributeValue::Int(i) => Some(*i as f64),
        AttributeValue::Double(d) => Some(*d),
        AttributeValue::Str(s) => s.parse().ok(),
        AttributeValue::Bool(_) => None,
    }
}

fn count(stat: &RtcStat, key: &str) -> Option<u64> {
    num(stat, key).map(|v| v.max(0.0) as u64)
}

/// A duration member (seconds, per webrtc-stats) as whole milliseconds.
fn ms(stat: &RtcStat, key: &str) -> Option<u64> {
    num(stat, key).map(|s| (s.max(0.0) * 1000.0) as u64)
}

fn text(stat: &RtcStat, key: &str) -> Option<String> {
    match stat.members.get(key)? {
        AttributeValue::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// The samples in one track's `getStats()` report: one per `outbound-rtp` (tagged with its
/// layer) or `inbound-rtp` entry of `direction`, codec resolved through `codecId`, RTT from the
/// `remote-inbound-rtp` that reports on the stream (outbound) or the nominated candidate pair
/// (inbound), durations converted from seconds to milliseconds. One mapping for every SDK.
pub(crate) fn samples_from_report(
    track_sid: &str,
    kind: TrackKind,
    direction: StreamDirection,
    report: &[RtcStat],
    timestamp_ns: Option<u64>,
) -> Vec<RtcStatsSample> {
    let index = Index::new(report);
    index
        .rtp
        .iter()
        .filter(|(_, d)| *d == direction)
        .map(|(stat, direction)| index.sample(stat, track_sid, kind, *direction, timestamp_ns))
        .collect()
}

/// The samples in a whole peer connection's `getStats()` report: every RTP stream whose track
/// is in `tracks` (MediaStreamTrack id → track sid; `trackIdentifier` of an `inbound-rtp`, or of
/// the `media-source` an `outbound-rtp` names), kind from the stream's `kind`. Streams of
/// unknown tracks are skipped. One call per peer connection per poll: no platform walks its
/// participants and tracks.
pub(crate) fn samples_from_peer_report(
    report: &[RtcStat],
    tracks: &HashMap<String, String>,
    timestamp_ns: Option<u64>,
) -> Vec<RtcStatsSample> {
    let index = Index::new(report);
    index
        .rtp
        .iter()
        .filter_map(|(stat, direction)| {
            let track_id = match direction {
                StreamDirection::Inbound => text(stat, "trackIdentifier"),
                StreamDirection::Outbound => text(stat, "mediaSourceId")
                    .and_then(|id| index.by_id.get(id.as_str()).copied())
                    .filter(|source| source.kind == "media-source")
                    .and_then(|source| text(source, "trackIdentifier")),
            }?;
            let sid = tracks.get(&track_id)?;
            let kind = match text(stat, "kind").or_else(|| text(stat, "mediaType"))?.as_str() {
                "audio" => TrackKind::Audio,
                "video" => TrackKind::Video,
                _ => return None,
            };
            Some(index.sample(stat, sid, kind, *direction, timestamp_ns))
        })
        .collect()
}

/// A report indexed once: entries by id, the `remote-inbound-rtp` reporting on each outbound
/// stream, the nominated candidate pair's RTT, the RTP streams with their direction. Resolving a
/// stream's codec, RTT or media source is then a lookup, not a scan of the report.
struct Index<'a> {
    by_id: HashMap<&'a str, &'a RtcStat>,
    remote_by_local: HashMap<String, &'a RtcStat>,
    pair_rtt_ms: Option<f64>,
    rtp: Vec<(&'a RtcStat, StreamDirection)>,
}

impl<'a> Index<'a> {
    fn new(report: &'a [RtcStat]) -> Self {
        let mut index = Self {
            by_id: HashMap::with_capacity(report.len()),
            remote_by_local: HashMap::new(),
            pair_rtt_ms: None,
            rtp: Vec::new(),
        };
        for stat in report {
            index.by_id.insert(stat.id.as_str(), stat);
            match stat.kind.as_str() {
                "outbound-rtp" => index.rtp.push((stat, StreamDirection::Outbound)),
                "inbound-rtp" => index.rtp.push((stat, StreamDirection::Inbound)),
                "remote-inbound-rtp" => {
                    if let Some(local) = text(stat, "localId") {
                        index.remote_by_local.insert(local, stat);
                    }
                }
                "candidate-pair" if index.pair_rtt_ms.is_none() => {
                    let nominated =
                        matches!(stat.members.get("nominated"), Some(AttributeValue::Bool(true)))
                            || text(stat, "state").as_deref() == Some("succeeded");
                    if nominated {
                        index.pair_rtt_ms = num(stat, "currentRoundTripTime").map(|s| s * 1000.0);
                    }
                }
                _ => {}
            }
        }
        index
    }

    /// One RTP stream entry as a sample.
    fn sample(
        &self,
        stat: &RtcStat,
        track_sid: &str,
        kind: TrackKind,
        direction: StreamDirection,
        timestamp_ns: Option<u64>,
    ) -> RtcStatsSample {
        let mut sample = RtcStatsSample::new(track_sid, kind, direction);
        sample.codec = text(stat, "codecId")
            .and_then(|id| self.by_id.get(id.as_str()).copied())
            .filter(|codec| codec.kind == "codec")
            .and_then(|codec| text(codec, "mimeType"));
        sample.timestamp_ns = timestamp_ns;
        match direction {
            StreamDirection::Outbound => {
                sample.layer = Some(text(stat, "rid").unwrap_or_else(|| stat.id.clone()));
                sample.bytes = count(stat, "bytesSent");
                sample.packets = count(stat, "packetsSent");
                sample.frames_per_second = num(stat, "framesPerSecond");
                sample.quality_limitation_bandwidth_ms =
                    ms(stat, "qualityLimitationDurations.bandwidth");
                sample.quality_limitation_cpu_ms = ms(stat, "qualityLimitationDurations.cpu");
                sample.quality_limitation_other_ms = ms(stat, "qualityLimitationDurations.other");
                sample.rtt_ms = self
                    .remote_by_local
                    .get(&stat.id)
                    .and_then(|remote| num(remote, "roundTripTime"))
                    .map(|s| s * 1000.0);
            }
            StreamDirection::Inbound => {
                sample.bytes = count(stat, "bytesReceived");
                sample.packets = count(stat, "packetsReceived");
                sample.packets_lost = count(stat, "packetsLost");
                sample.freeze_count = count(stat, "freezeCount");
                sample.freezes_duration_ms = ms(stat, "totalFreezesDuration");
                sample.pause_count = count(stat, "pauseCount");
                sample.pauses_duration_ms = ms(stat, "totalPausesDuration");
                sample.concealed_samples = count(stat, "concealedSamples");
                sample.silent_concealed_samples = count(stat, "silentConcealedSamples");
                sample.concealment_events = count(stat, "concealmentEvents");
                sample.interruption_count = count(stat, "interruptionCount");
                sample.interruptions_duration_ms = ms(stat, "totalInterruptionDuration");
                sample.jitter_buffer_delay_ms = ms(stat, "jitterBufferDelay");
                sample.jitter_buffer_emitted_count = count(stat, "jitterBufferEmittedCount");
                sample.jitter_ms = num(stat, "jitter").map(|s| s * 1000.0);
                sample.frames_per_second = num(stat, "framesPerSecond");
                sample.audio_level = num(stat, "audioLevel");
                sample.rtt_ms = self.pair_rtt_ms;
            }
        }
        sample
    }
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
            quality_limitation_other_ms: None,
            pause_count: None,
            pauses_duration_ms: None,
            silent_concealed_samples: None,
            interruption_count: None,
            interruptions_duration_ms: None,
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
    session: Arc<ScopeState>,
    /// The window's owner, captured when it opened: the session's project and its correlation
    /// attributes then. A change to either closes the window first (see `StatsWindows::split`).
    route: Option<String>,
    custom: Vec<Attribute>,
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
    fn open(start_ns: u64, first: RtcStatsSample, session: Arc<ScopeState>) -> Self {
        let mut window = Self {
            route: session.route(),
            custom: session.custom_snapshot(),
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

    /// The closed window as a queued record, with the owner it captured when it opened.
    fn into_queued(self, end_ns: u64) -> Queued {
        let (session, route, custom) =
            (self.session.clone(), self.route.clone(), self.custom.clone());
        let mut event = self.into_event(end_ns);
        for attribute in custom {
            if !event.attributes.iter().any(|a| a.key == attribute.key) {
                event.attributes.push(attribute);
            }
        }
        Queued { event, session, route }
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
            ("lk.rtc.quality_limitation.other_ms", last.quality_limitation_other_ms),
            ("lk.rtc.pause_count", last.pause_count),
            ("lk.rtc.pauses_duration_ms", last.pauses_duration_ms),
            ("lk.rtc.silent_concealed_samples", last.silent_concealed_samples),
            ("lk.rtc.interruption_count", last.interruption_count),
            ("lk.rtc.interruptions_duration_ms", last.interruptions_duration_ms),
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
/// The platform pushes raw `getStats()` readings when the core asks; the core ships one
/// `lk.rtc.stats.sample` per window (60 s by default, stretched under device pressure) — on
/// device: 1 Hz raw sampling across ~100k concurrent participants would be the same ~100k
/// records/s fleet-wide.
#[derive(Default)]
pub(crate) struct StatsWindows {
    windows: HashMap<TrackKey, Window>,
    /// Last cumulative `qualityLimitationDurations.cpu` per outbound track.
    limitation: HashMap<TrackKey, u64>,
    cpu_limited_until: Option<Instant>,
    /// Last cumulative counters per simulcast layer, per outbound track: the track's counters
    /// are their sum, so a suspended top layer cannot freeze them.
    layers: HashMap<TrackKey, HashMap<String, LayerCounters>>,
    /// The opt-out, checked under the lock that guards the windows (see `Store`).
    revoked: Arc<std::sync::atomic::AtomicBool>,
}

/// One track in one direction *in one session*: two rooms in a process may subscribe to the same
/// published track (same sid, same direction), and their readings must never merge.
type TrackKey = ([u8; 16], String, StreamDirection);

fn track_key(session: &ScopeState, sample: &RtcStatsSample) -> TrackKey {
    (session.trace_id, sample.track_sid.clone(), sample.direction)
}

#[derive(Debug, Clone, Copy, Default)]
struct LayerCounters {
    bytes: Option<u64>,
    packets: Option<u64>,
    fps: Option<f64>,
    limitation_bandwidth_ms: Option<u64>,
    limitation_cpu_ms: Option<u64>,
    limitation_other_ms: Option<u64>,
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
    /// Tie the windows to a pipeline's opt-out.
    pub fn with_consent(revoked: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { revoked, ..Self::default() }
    }

    pub fn record_in(&mut self, mut sample: RtcStatsSample, session: &Arc<ScopeState>) {
        if self.revoked.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let key = track_key(session, &sample);
        self.fold_layers(&key, &mut sample);
        self.track_limitation(&key, &sample);
        let timestamp = *sample.timestamp_ns.get_or_insert_with(now_unix_nanos);
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
    fn fold_layers(&mut self, key: &TrackKey, sample: &mut RtcStatsSample) {
        let Some(layer) = sample.layer.clone() else { return };
        let layers = self.layers.entry(key.clone()).or_default();
        layers.insert(
            layer,
            LayerCounters {
                bytes: sample.bytes,
                packets: sample.packets,
                fps: sample.frames_per_second,
                limitation_bandwidth_ms: sample.quality_limitation_bandwidth_ms,
                limitation_cpu_ms: sample.quality_limitation_cpu_ms,
                limitation_other_ms: sample.quality_limitation_other_ms,
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
        sample.quality_limitation_other_ms =
            max_u64(layers.values().map(|l| l.limitation_other_ms));
    }

    /// Close every open window into its event, filed under the window's session, and start fresh.
    /// A track that produced no reading for a whole window has left (unpublished, unsubscribed,
    /// its room gone): its per-layer and CPU counters are retired with it, so memory follows the
    /// live tracks rather than every track the process ever saw.
    pub fn close(&mut self) -> Vec<Queued> {
        let windows = &self.windows;
        self.layers.retain(|key, _| windows.contains_key(key));
        self.limitation.retain(|key, _| windows.contains_key(key));
        let end = now_unix_nanos();
        self.windows.drain().map(|(_, window)| window.into_queued(end)).collect()
    }

    /// A track left `session` (`track_sid`), or the session ended (`None`): close its windows now
    /// — the last partial window still ships — and forget everything kept for it.
    pub fn retire(&mut self, session: &ScopeState, track_sid: Option<&str>) -> Vec<Queued> {
        let gone = |(trace_id, sid, _): &TrackKey| {
            *trace_id == session.trace_id && track_sid.is_none_or(|t| t == sid)
        };
        self.layers.retain(|key, _| !gone(key));
        self.limitation.retain(|key, _| !gone(key));
        let keys: Vec<TrackKey> = self.windows.keys().filter(|k| gone(k)).cloned().collect();
        let end = now_unix_nanos();
        keys.into_iter()
            .filter_map(|key| self.windows.remove(&key))
            .map(|window| window.into_queued(end))
            .collect()
    }

    /// Close `session`'s open windows now — their records then carry the attributes they were
    /// captured under — keeping each track's cumulative state for the windows that follow.
    pub fn split(&mut self, session: &ScopeState) -> Vec<Queued> {
        let keys: Vec<TrackKey> = self
            .windows
            .keys()
            .filter(|(trace_id, ..)| *trace_id == session.trace_id)
            .cloned()
            .collect();
        let end = now_unix_nanos();
        keys.into_iter()
            .filter_map(|key| self.windows.remove(&key))
            .map(|window| window.into_queued(end))
            .collect()
    }

    /// How many tracks the accumulator holds state for (windows, layers, CPU counters).
    #[cfg(test)]
    pub fn tracked(&self) -> (usize, usize, usize) {
        (self.windows.len(), self.layers.len(), self.limitation.len())
    }

    #[cfg(test)]
    pub fn record(&mut self, sample: RtcStatsSample) {
        self.record_in(sample, &test_session());
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
    fn track_limitation(&mut self, key: &TrackKey, sample: &RtcStatsSample) {
        if sample.direction != StreamDirection::Outbound {
            return;
        }
        let current = sample.quality_limitation_cpu_ms.unwrap_or(0);
        if let Some(previous) = self.limitation.insert(key.clone(), current) {
            if current > previous {
                self.cpu_limited_until = Some(Instant::now() + CPU_LIMITED_HOLD);
            }
        }
    }

    /// Drop everything (opt-out); returns how many open windows went.
    pub fn clear(&mut self) -> u64 {
        let open = self.windows.len() as u64;
        *self = Self::with_consent(self.revoked.clone());
        open
    }
}

/// The one session every `StatsWindows::record` in a unit test files under.
#[cfg(test)]
fn test_session() -> Arc<ScopeState> {
    ScopeState::with_trace_id([7; 16])
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

    #[test]
    fn a_report_maps_to_samples_with_codec_rtt_layers_and_milliseconds() {
        let stat = |kind: &str, id: &str, members: &[(&str, AttributeValue)]| RtcStat {
            kind: kind.into(),
            id: id.into(),
            members: members.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        };
        let s = |v: &str| AttributeValue::Str(v.into());
        let report = vec![
            stat("codec", "C1", &[("mimeType", s("video/VP8"))]),
            stat(
                "outbound-rtp",
                "OUT_f",
                &[
                    ("rid", s("f")),
                    ("codecId", s("C1")),
                    ("bytesSent", AttributeValue::Int(7_000)),
                    ("packetsSent", s("70")),
                    ("framesPerSecond", AttributeValue::Double(29.5)),
                    ("qualityLimitationDurations.cpu", AttributeValue::Double(1.25)),
                ],
            ),
            stat(
                "outbound-rtp",
                "OUT_h",
                &[("rid", s("h")), ("bytesSent", AttributeValue::Int(500))],
            ),
            stat(
                "remote-inbound-rtp",
                "RI_f",
                &[("localId", s("OUT_f")), ("roundTripTime", AttributeValue::Double(0.042))],
            ),
        ];
        let out = samples_from_report(
            "TR_1",
            TrackKind::Video,
            StreamDirection::Outbound,
            &report,
            Some(5),
        );
        assert_eq!(out.len(), 2);
        let f = out.iter().find(|x| x.layer.as_deref() == Some("f")).expect("layer f");
        assert_eq!(f.codec.as_deref(), Some("video/VP8"));
        assert_eq!((f.bytes, f.packets, f.frames_per_second), (Some(7_000), Some(70), Some(29.5)));
        assert_eq!(f.quality_limitation_cpu_ms, Some(1_250));
        assert_eq!(f.rtt_ms, Some(42.0));
        assert_eq!(f.timestamp_ns, Some(5));
        assert_eq!(
            out.iter().find(|x| x.layer.as_deref() == Some("h")).and_then(|x| x.rtt_ms),
            None
        );

        let report = vec![
            stat(
                "candidate-pair",
                "CP",
                &[
                    ("nominated", AttributeValue::Bool(true)),
                    ("currentRoundTripTime", AttributeValue::Double(0.1)),
                ],
            ),
            stat(
                "inbound-rtp",
                "IN",
                &[
                    ("bytesReceived", AttributeValue::Int(12)),
                    ("packetsLost", AttributeValue::Int(-3)),
                    ("jitter", AttributeValue::Double(0.02)),
                    ("totalFreezesDuration", AttributeValue::Double(2.5)),
                    ("jitterBufferDelay", AttributeValue::Double(3.0)),
                    ("audioLevel", AttributeValue::Double(0.5)),
                ],
            ),
        ];
        let out =
            samples_from_report("TR_2", TrackKind::Audio, StreamDirection::Inbound, &report, None);
        assert_eq!(out.len(), 1);
        let x = &out[0];
        assert_eq!((x.bytes, x.packets_lost, x.rtt_ms), (Some(12), Some(0), Some(100.0)));
        assert_eq!(
            (x.jitter_ms, x.freezes_duration_ms, x.jitter_buffer_delay_ms),
            (Some(20.0), Some(2_500), Some(3_000))
        );
        assert_eq!(x.layer, None);
    }

    /// Finding 19: two rooms in one process subscribed to the same published track keep two
    /// windows, each filed under its own session.
    #[test]
    fn the_same_track_in_two_sessions_never_merges() {
        let mut windows = StatsWindows::default();
        let (a, b) = (ScopeState::new(), ScopeState::new());
        let reading = |bytes| RtcStatsSample {
            bytes: Some(bytes),
            ..RtcStatsSample::new("TR_shared", TrackKind::Audio, StreamDirection::Inbound)
        };
        windows.record_in(reading(100), &a);
        windows.record_in(reading(900), &b);
        windows.record_in(reading(200), &a);
        let closed = windows.close();
        assert_eq!(closed.len(), 2, "one window per session");
        for (session, bytes) in [(&a, 200), (&b, 900)] {
            let window = closed.iter().find(|q| q.session == *session).expect("own window");
            assert_eq!(attr(&window.event, "lk.rtc.bytes"), Some(AttributeValue::Int(bytes)));
        }
    }

    /// Finding 14: per-track state follows the live tracks — retired when a track leaves, when
    /// its session ends, and when it goes a whole window without a reading.
    #[test]
    fn per_track_state_is_retired_with_the_track() {
        let mut windows = StatsWindows::default();
        let (a, b) = (ScopeState::new(), ScopeState::new());
        let outbound = |sid: &str| RtcStatsSample {
            bytes: Some(1),
            quality_limitation_cpu_ms: Some(0),
            layer: Some("f".into()),
            ..RtcStatsSample::new(sid, TrackKind::Video, StreamDirection::Outbound)
        };
        for sid in ["TR_1", "TR_2"] {
            windows.record_in(outbound(sid), &a);
        }
        windows.record_in(outbound("TR_3"), &b);
        assert_eq!(windows.tracked(), (3, 3, 3));

        let last = windows.retire(&a, Some("TR_1"));
        assert_eq!(last.len(), 1, "the unpublished track's partial window ships now");
        assert_eq!(windows.tracked(), (2, 2, 2));

        let last = windows.retire(&a, None);
        assert_eq!(last.len(), 1, "the session's last window ships at disconnect");
        assert_eq!(windows.tracked(), (1, 1, 1), "the other session keeps its track");

        windows.close();
        assert_eq!(windows.tracked(), (0, 1, 1), "counters survive an ordinary window…");
        windows.close();
        assert_eq!(windows.tracked(), (0, 0, 0), "…not a whole window without a reading");
    }

    #[test]
    fn a_peer_connection_report_maps_to_its_tracks() {
        let stat = |kind: &str, id: &str, members: &[(&str, &str)]| RtcStat {
            kind: kind.into(),
            id: id.into(),
            members: members
                .iter()
                .map(|(k, v)| (k.to_string(), AttributeValue::Str(v.to_string())))
                .collect(),
        };
        let report = vec![
            stat("media-source", "MS1", &[("trackIdentifier", "local-cam")]),
            stat(
                "outbound-rtp",
                "O1",
                &[("kind", "video"), ("mediaSourceId", "MS1"), ("rid", "f"), ("bytesSent", "10")],
            ),
            stat(
                "outbound-rtp",
                "O2",
                &[("kind", "video"), ("mediaSourceId", "MS1"), ("rid", "h"), ("bytesSent", "5")],
            ),
            stat(
                "inbound-rtp",
                "I1",
                &[("kind", "audio"), ("trackIdentifier", "TR_remote"), ("bytesReceived", "7")],
            ),
            stat(
                "inbound-rtp",
                "I2",
                &[("kind", "audio"), ("trackIdentifier", "unknown"), ("bytesReceived", "1")],
            ),
        ];
        let tracks = HashMap::from([
            ("local-cam".to_owned(), "TR_cam".to_owned()),
            ("TR_remote".to_owned(), "TR_remote".to_owned()),
        ]);
        let out = samples_from_peer_report(&report, &tracks, None);
        assert_eq!(out.len(), 3, "two camera layers and the known remote track");
        assert!(out
            .iter()
            .filter(|s| s.track_sid == "TR_cam")
            .all(|s| { s.kind == TrackKind::Video && s.direction == StreamDirection::Outbound }));
        let remote = out.iter().find(|s| s.track_sid == "TR_remote").expect("remote");
        assert_eq!((remote.kind, remote.bytes), (TrackKind::Audio, Some(7)));
    }
}
