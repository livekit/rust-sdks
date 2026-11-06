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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use tokio::time::Instant;

use crate::{
    event::now_unix_nanos, scope::ScopeState, store::QueuedEvent, AttributeValue, TelemetryEvent,
};

/// Audio or video.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    /// An audio track; sent as `audio`.
    Audio,
    /// A video track; sent as `video`.
    Video,
}

/// Received (inbound) or sent (outbound) media.
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamDirection {
    /// Media this device receives; sent as `inbound`.
    Inbound,
    /// Media this device sends; sent as `outbound`.
    Outbound,
}

/// One reading of a track's RTP statistics, as `getStats()` reports them.
///
/// Cumulative counters (`bytes`, `packets`, `freeze_count`, …) are passed through as reported —
/// monotonic, paired with their denominators (the W3C webrtc-stats model) so any layer can
/// recompute rates and a dropped window never corrupts the next. An outbound track whose readings
/// name their RTP streams (`layer`) ships its bytes and packets as a running total of each
/// stream's growth instead, so a stream that comes and goes is never counted twice. Gauges (`jitter_ms`, `rtt_ms`,
/// `frames_per_second`, `audio_level`) are summarised per window as min/max/avg. Fields a
/// platform or direction does not have stay `None` and are omitted from the wire.
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[derive(Debug, Clone, PartialEq)]
pub struct RtcStatsSample {
    /// The track's sid (`TR_…`).
    pub track_sid: String,
    /// Audio or video.
    pub kind: TrackKind,
    /// Sent (outbound) or received (inbound).
    pub direction: StreamDirection,
    /// The codec's `mimeType` (`video/VP8`), resolved through the stream's `codecId`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub codec: Option<String>,
    // Cumulative counters.
    /// `bytesSent` (outbound) or `bytesReceived` (inbound).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub bytes: Option<u64>,
    /// `packetsSent` (outbound) or `packetsReceived` (inbound).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub packets: Option<u64>,
    /// `packetsLost`, inbound.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub packets_lost: Option<u64>,
    /// `freezeCount`, inbound video: short stalls, a frame shown at least 3× the average frame
    /// time, and at least 150 ms more than it, after the one before. A longer gap is a pause
    /// instead.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub freeze_count: Option<u64>,
    /// `totalFreezesDuration` in milliseconds, inbound video.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub freezes_duration_ms: Option<u64>,
    /// `concealedSamples`, inbound audio: samples the receiver made up for lost or late packets.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub concealed_samples: Option<u64>,
    /// `concealmentEvents`, inbound audio: how many separate runs of concealment started (one loss
    /// burst is one event, however many samples it conceals).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub concealment_events: Option<u64>,
    /// `jitterBufferDelay` in milliseconds, inbound: the total time emitted samples or frames spent
    /// in the jitter buffer. Divide by `jitter_buffer_emitted_count` for the average.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_buffer_delay_ms: Option<u64>,
    /// `jitterBufferEmittedCount`, inbound: samples or frames that left the jitter buffer.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_buffer_emitted_count: Option<u64>,
    /// `qualityLimitationDurations.bandwidth` in milliseconds, outbound video.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_bandwidth_ms: Option<u64>,
    /// `qualityLimitationDurations.cpu` in milliseconds, outbound video: time the encoder was
    /// limited by the CPU.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_cpu_ms: Option<u64>,
    /// `qualityLimitationDurations.other` in milliseconds, outbound video.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub quality_limitation_other_ms: Option<u64>,
    /// `pauseCount`, inbound video: gaps of more than 5 s without a frame (usually the sender
    /// paused). A shorter stall is a freeze instead.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub pause_count: Option<u64>,
    /// `totalPausesDuration` in milliseconds, inbound video.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub pauses_duration_ms: Option<u64>,
    /// `silentConcealedSamples`, inbound audio: the concealed samples that were silence, which a
    /// listener cannot hear. Part of `concealed_samples`.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub silent_concealed_samples: Option<u64>,
    /// `interruptionCount`, inbound audio: concealment runs of 150 ms or more, long enough to hear.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub interruption_count: Option<u64>,
    /// `totalInterruptionDuration` in milliseconds, inbound audio.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub interruptions_duration_ms: Option<u64>,
    // Gauges.
    /// `jitter` in milliseconds, inbound.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub jitter_ms: Option<f64>,
    /// Round-trip time in milliseconds: the `remote-inbound-rtp` `roundTripTime` (outbound), the
    /// selected candidate pair's `currentRoundTripTime` (inbound).
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub rtt_ms: Option<f64>,
    /// `framesPerSecond`, video.
    #[cfg_attr(feature = "uniffi", uniffi(default))]
    pub frames_per_second: Option<f64>,
    /// `audioLevel`, inbound audio: 0.0 to 1.0, linear (not dB).
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
    /// The stat's `type`: `inbound-rtp`, `candidate-pair`, `codec`, …
    pub kind: String,
    /// The stat's `id`, which other stats name (`codecId`, `localId`, `mediaSourceId`).
    pub id: String,
    /// Its members by W3C name.
    pub members: HashMap<String, AttributeValue>,
}

/// A numeric member as a finite `f64`: a NaN or an infinity is no reading.
fn num(stat: &RtcStat, key: &str) -> Option<f64> {
    match stat.members.get(key)? {
        AttributeValue::Int(i) => Some(*i as f64),
        AttributeValue::Double(d) => Some(*d),
        AttributeValue::Str(s) => s.parse().ok(),
        AttributeValue::Bool(_) => None,
    }
    .filter(|v: &f64| v.is_finite())
}

/// A counter member as a whole number (negative readings count as 0).
fn count(stat: &RtcStat, key: &str) -> Option<u64> {
    num(stat, key).map(|v| v.max(0.0) as u64)
}

/// A duration member (seconds, per webrtc-stats) as whole milliseconds.
fn ms(stat: &RtcStat, key: &str) -> Option<u64> {
    num(stat, key).map(|s| (s.max(0.0) * 1000.0) as u64)
}

/// A duration member (seconds) as fractional milliseconds, for gauges.
fn ms_f64(stat: &RtcStat, key: &str) -> Option<f64> {
    num(stat, key).map(|s| s * 1000.0)
}

/// A string member.
fn text(stat: &RtcStat, key: &str) -> Option<String> {
    match stat.members.get(key)? {
        AttributeValue::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// The samples in one track's `getStats()` report: one per `outbound-rtp` (tagged with its
/// layer) or `inbound-rtp` entry of `direction`, codec resolved through `codecId`, RTT from the
/// `remote-inbound-rtp` that reports on the stream (outbound) or the selected candidate pair
/// (inbound), durations converted from seconds to milliseconds. One mapping for every SDK.
pub(crate) fn samples_from_report(
    track_sid: &str,
    kind: TrackKind,
    direction: StreamDirection,
    report: &[RtcStat],
    timestamp_ns: Option<u64>,
) -> Vec<RtcStatsSample> {
    // One poll, one time: its layers then fold into one reading (see `Window::add`).
    let timestamp_ns = Some(timestamp_ns.unwrap_or_else(now_unix_nanos));
    let index = ReportIndex::new(report);
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
    let timestamp_ns = Some(timestamp_ns.unwrap_or_else(now_unix_nanos));
    let index = ReportIndex::new(report);
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
/// stream, the selected candidate pair's RTT, the RTP streams with their direction. Resolving a
/// stream's codec, RTT or media source is then a lookup, not a scan of the report.
struct ReportIndex<'a> {
    by_id: HashMap<&'a str, &'a RtcStat>,
    remote_by_local: HashMap<String, &'a RtcStat>,
    pair_rtt_ms: Option<f64>,
    rtp: Vec<(&'a RtcStat, StreamDirection)>,
}

impl<'a> ReportIndex<'a> {
    fn new(report: &'a [RtcStat]) -> Self {
        let mut index = Self {
            by_id: HashMap::with_capacity(report.len()),
            remote_by_local: HashMap::new(),
            pair_rtt_ms: None,
            rtp: Vec::new(),
        };
        // The transport's selected pair, else the first pair that is selected, or nominated and
        // succeeded (not every platform names the selected pair).
        let (mut selected, mut fallback) = (None, None);
        let flag =
            |stat: &RtcStat, key| matches!(stat.members.get(key), Some(AttributeValue::Bool(true)));
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
                "transport" => {
                    selected = selected.or_else(|| text(stat, "selectedCandidatePairId"))
                }
                "candidate-pair" if fallback.is_none() => {
                    let succeeded = text(stat, "state").as_deref() == Some("succeeded");
                    if flag(stat, "selected") || (flag(stat, "nominated") && succeeded) {
                        fallback = Some(stat);
                    }
                }
                _ => {}
            }
        }
        let pair = selected
            .and_then(|id| index.by_id.get(id.as_str()).copied())
            .filter(|pair| pair.kind == "candidate-pair")
            .or(fallback);
        index.pair_rtt_ms = pair.and_then(|pair| ms_f64(pair, "currentRoundTripTime"));
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
                    .and_then(|remote| ms_f64(remote, "roundTripTime"));
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
                sample.jitter_ms = ms_f64(stat, "jitter");
                sample.frames_per_second = num(stat, "framesPerSecond");
                sample.audio_level = num(stat, "audioLevel");
                sample.rtt_ms = self.pair_rtt_ms;
            }
        }
        sample
    }
}

/// Keep the larger of `mine` and `theirs`, whichever is known.
fn larger<T: PartialOrd>(mine: &mut Option<T>, theirs: Option<T>) {
    if let Some(theirs) = theirs {
        if mine.as_ref().is_none_or(|mine| theirs > *mine) {
            *mine = Some(theirs);
        }
    }
}

impl RtcStatsSample {
    /// Merge another stream of the same poll into this reading: each field keeps the larger of
    /// the values known, so a stream without a field never erases it and the report's order
    /// changes nothing.
    fn merge(&mut self, other: RtcStatsSample) {
        larger(&mut self.codec, other.codec);
        larger(&mut self.bytes, other.bytes);
        larger(&mut self.packets, other.packets);
        larger(&mut self.packets_lost, other.packets_lost);
        larger(&mut self.freeze_count, other.freeze_count);
        larger(&mut self.freezes_duration_ms, other.freezes_duration_ms);
        larger(&mut self.concealed_samples, other.concealed_samples);
        larger(&mut self.concealment_events, other.concealment_events);
        larger(&mut self.jitter_buffer_delay_ms, other.jitter_buffer_delay_ms);
        larger(&mut self.jitter_buffer_emitted_count, other.jitter_buffer_emitted_count);
        larger(&mut self.quality_limitation_bandwidth_ms, other.quality_limitation_bandwidth_ms);
        larger(&mut self.quality_limitation_cpu_ms, other.quality_limitation_cpu_ms);
        larger(&mut self.quality_limitation_other_ms, other.quality_limitation_other_ms);
        larger(&mut self.pause_count, other.pause_count);
        larger(&mut self.pauses_duration_ms, other.pauses_duration_ms);
        larger(&mut self.silent_concealed_samples, other.silent_concealed_samples);
        larger(&mut self.interruption_count, other.interruption_count);
        larger(&mut self.interruptions_duration_ms, other.interruptions_duration_ms);
        larger(&mut self.jitter_ms, other.jitter_ms);
        larger(&mut self.rtt_ms, other.rtt_ms);
        larger(&mut self.frames_per_second, other.frames_per_second);
        larger(&mut self.audio_level, other.audio_level);
    }

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

    /// The average over the window; `None` without a reading.
    fn avg(&self) -> Option<f64> {
        (self.n > 0).then(|| self.sum / self.n as f64)
    }

    fn attach(&self, event: TelemetryEvent, key: &str) -> TelemetryEvent {
        let Some(avg) = self.avg() else { return event };
        event
            .with_attribute(format!("{key}.min"), self.min)
            .with_attribute(format!("{key}.max"), self.max)
            .with_attribute(format!("{key}.avg"), avg)
    }
}

/// Samples of one track in one direction accumulated since the window opened.
struct Window {
    /// The session the window ships under. A change to its project or correlation attributes
    /// closes the window first (see `StatsWindows::split`), so the owner at close is the owner the
    /// readings were taken under.
    session: Arc<ScopeState>,
    start_ns: u64,
    /// Readings in the window: one per poll.
    samples: u32,
    /// Cumulative counters at the window's first reading, for the display body's deltas.
    first: RtcStatsSample,
    /// The newest reading.
    last: RtcStatsSample,
    jitter: Gauge,
    rtt: Gauge,
    fps: Gauge,
    audio_level: Gauge,
}

impl Window {
    fn open(start_ns: u64, first: RtcStatsSample, session: Arc<ScopeState>) -> Self {
        let mut window = Self {
            session,
            start_ns,
            samples: 0,
            first: first.clone(),
            last: first,
            jitter: Gauge::default(),
            rtt: Gauge::default(),
            fps: Gauge::default(),
            audio_level: Gauge::default(),
        };
        window.commit();
        window
    }

    /// `video inbound: 1204 kbps, loss 0.4%, rtt 48 ms, 30 fps` — deltas between the window's
    /// first and last reading. The bitrate is over the time between those two readings, and is
    /// left out without two distinct reading times.
    fn summary(&self) -> String {
        let mut parts = Vec::new();
        let delta = |a: Option<u64>, b: Option<u64>| Some(a?.saturating_sub(b?));
        let interval_ms =
            delta(self.last.timestamp_ns, self.first.timestamp_ns).map(|ns| ns / 1_000_000);
        if let (Some(bytes), Some(ms)) =
            (delta(self.last.bytes, self.first.bytes), interval_ms.filter(|ms| *ms > 0))
        {
            parts.push(format!("{} kbps", bytes * 8 / ms));
        }
        if let (Some(lost), Some(packets)) = (
            delta(self.last.packets_lost, self.first.packets_lost),
            delta(self.last.packets, self.first.packets),
        ) {
            if lost + packets > 0 {
                parts.push(format!("loss {:.1}%", lost as f64 * 100.0 / (lost + packets) as f64));
            }
        }
        if let Some(rtt) = self.rtt.avg() {
            parts.push(format!("rtt {rtt:.0} ms"));
        }
        if let Some(fps) = self.fps.avg() {
            parts.push(format!("{fps:.0} fps"));
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

    /// The closed window as a record captured now (see `QueuedEvent::new`).
    fn into_queued(self, end_ns: u64) -> QueuedEvent {
        let session = Arc::clone(&self.session);
        QueuedEvent::new(self.into_event(end_ns), session)
    }

    /// One more poll's reading.
    fn add(&mut self, reading: RtcStatsSample) {
        self.last = reading;
        self.commit();
    }

    /// Put the newest reading into the gauges.
    fn commit(&mut self) {
        self.samples += 1;
        self.jitter.add(self.last.jitter_ms);
        self.rtt.add(self.last.rtt_ms);
        self.fps.add(self.last.frames_per_second);
        self.audio_level.add(self.last.audio_level);
    }

    /// The `lk.rtc.stats.sample` event for this window, stamped at `end_ns`. Its body is the
    /// one-line human summary a log view shows (OTel: an event's body is its display message);
    /// the attributes carry the numbers.
    fn into_event(self, end_ns: u64) -> TelemetryEvent {
        let window_ms = end_ns.saturating_sub(self.start_ns) / 1_000_000;
        let body = self.summary();
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
    /// Per track: the poll in progress and the counting state across polls.
    tracks: HashMap<TrackKey, Track>,
    /// Tracks with a reading since the last [`close`](Self::close): only these keep their stream
    /// and CPU state. A window closed early (a split) does not retire its track.
    seen: HashSet<TrackKey>,
    /// The opt-out, checked under the lock that guards the windows (see `Store`).
    revoked: Arc<std::sync::atomic::AtomicBool>,
}

/// One track in one direction *in one session*: two rooms in a process may subscribe to the same
/// published track (same sid, same direction), and their readings must never merge.
type TrackKey = ([u8; 16], String, StreamDirection);

fn track_key(session: &ScopeState, sample: &RtcStatsSample) -> TrackKey {
    (session.trace_id, sample.track_sid.clone(), sample.direction)
}

/// One track's state across polls.
#[derive(Default)]
struct Track {
    /// The poll being gathered: a report's streams arrive one by one, and the poll is read as a
    /// whole once the next poll comes or its window closes.
    poll: Option<Poll>,
    /// The previous poll's bytes and packets per RTP stream (`layer`): one report's streams at
    /// most, no older ones.
    previous: HashMap<String, StreamCounts>,
    /// A layered outbound track's bytes and packets so far: every poll's growth, summed.
    total: StreamCounts,
}

/// One poll's readings, as the streams of one report came in.
struct Poll {
    session: Arc<ScopeState>,
    timestamp_ns: u64,
    readings: Vec<RtcStatsSample>,
}

/// A stream's, or a track's, bytes and packets.
#[derive(Debug, Clone, Copy, Default)]
struct StreamCounts {
    bytes: Option<u64>,
    packets: Option<u64>,
}

impl Track {
    /// A poll's readings as one: each field the largest value its streams report, so their
    /// order changes nothing. On a layered outbound track (streams named by `layer`), the bytes
    /// and packets are the track's running total instead: each named stream adds its growth since
    /// the previous poll. Any other track passes its counters through as reported. A stream that was not in the previous poll only sets its starting point,
    /// so a new or returning stream's first poll is not counted (an undercount, never a double
    /// count), and an absent counter keeps the starting point it had.
    fn reading(&mut self, readings: Vec<RtcStatsSample>) -> Option<RtcStatsSample> {
        // A track's readings all share its direction (it is part of the track's key).
        let named =
            readings.iter().any(|r| r.direction == StreamDirection::Outbound && r.layer.is_some());
        let mut streams: HashMap<String, StreamCounts> = HashMap::new();
        let mut reading: Option<RtcStatsSample> = None;
        for sample in readings {
            if named {
                let stream = streams.entry(sample.layer.clone().unwrap_or_default()).or_default();
                larger(&mut stream.bytes, sample.bytes);
                larger(&mut stream.packets, sample.packets);
            }
            match &mut reading {
                Some(reading) => reading.merge(sample),
                None => reading = Some(sample),
            }
        }
        let mut reading = reading?;
        reading.layer = None;
        if named {
            let mut grown = StreamCounts::default();
            for (id, now) in &mut streams {
                let before = self.previous.get(id).copied().unwrap_or_default();
                grow(&mut grown.bytes, now.bytes, before.bytes);
                grow(&mut grown.packets, now.packets, before.packets);
                now.bytes = now.bytes.or(before.bytes);
                now.packets = now.packets.or(before.packets);
            }
            self.previous = streams;
            add(&mut self.total.bytes, grown.bytes);
            add(&mut self.total.packets, grown.packets);
            reading.bytes = self.total.bytes;
            reading.packets = self.total.packets;
        }
        Some(reading)
    }
}

/// Count a stream's growth into `grown`: known `now` against a known `before`. A stream with
/// `now` known still makes the poll's count known (`Some(0)` at least), so a track's first poll
/// reports a total.
fn grow(grown: &mut Option<u64>, now: Option<u64>, before: Option<u64>) {
    let Some(now) = now else { return };
    let growth = before.map_or(0, |before| now.saturating_sub(before));
    *grown = Some(grown.unwrap_or(0) + growth);
}

/// Add a poll's growth to a running total.
fn add(total: &mut Option<u64>, grown: Option<u64>) {
    if let Some(grown) = grown {
        *total = Some(total.unwrap_or(0) + grown);
    }
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

    /// Add one reading to its track's poll in progress. A reading with another timestamp starts
    /// the next poll, and the finished one goes into the track's window as one reading.
    pub fn record_in(&mut self, mut sample: RtcStatsSample, session: &Arc<ScopeState>) {
        if self.revoked.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let key = track_key(session, &sample);
        self.seen.insert(key.clone());
        let timestamp_ns = *sample.timestamp_ns.get_or_insert_with(now_unix_nanos);
        let track = self.tracks.entry(key.clone()).or_default();
        if track.poll.as_ref().is_some_and(|poll| poll.timestamp_ns != timestamp_ns) {
            self.end_poll(&key);
        }
        self.check_limitation(&key, &sample);
        let track = self.tracks.entry(key).or_default();
        let poll = track.poll.get_or_insert_with(|| Poll {
            session: session.clone(),
            timestamp_ns,
            readings: Vec::new(),
        });
        poll.readings.push(sample);
    }

    /// Read the track's poll in progress as one reading into its window, opening the window if
    /// needed.
    fn end_poll(&mut self, key: &TrackKey) {
        let Some(track) = self.tracks.get_mut(key) else { return };
        let Some(Poll { session, timestamp_ns, readings }) = track.poll.take() else { return };
        let Some(reading) = track.reading(readings) else { return };
        self.set_limitation_baseline(key, &reading);
        match self.windows.get_mut(key) {
            Some(window) => window.add(reading),
            None => {
                self.windows.insert(key.clone(), Window::open(timestamp_ns, reading, session));
            }
        }
    }

    /// End every poll in progress that `ended` selects.
    fn end_polls(&mut self, ended: impl Fn(&TrackKey) -> bool) {
        let keys: Vec<TrackKey> = self
            .tracks
            .iter()
            .filter(|(key, track)| track.poll.is_some() && ended(key))
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            self.end_poll(&key);
        }
    }

    /// Close every open window into its event, filed under the window's session, and start fresh.
    /// A track that produced no reading for a whole window has left (unpublished, unsubscribed,
    /// its room gone): its stream and CPU counters are retired with it, so memory follows the
    /// live tracks rather than every track the process ever saw.
    pub fn close(&mut self) -> Vec<QueuedEvent> {
        self.end_polls(|_| true);
        let seen = std::mem::take(&mut self.seen);
        self.tracks.retain(|key, _| seen.contains(key));
        self.limitation.retain(|key, _| seen.contains(key));
        let end = now_unix_nanos();
        self.windows.drain().map(|(_, window)| window.into_queued(end)).collect()
    }

    /// A track left `session` (`track_sid`), or the session ended (`None`): close its windows now
    /// — the last partial window still ships — and forget everything kept for it.
    pub fn retire(&mut self, session: &ScopeState, track_sid: Option<&str>) -> Vec<QueuedEvent> {
        let gone = |(trace_id, sid, _): &TrackKey| {
            *trace_id == session.trace_id && track_sid.is_none_or(|t| t == sid)
        };
        self.end_polls(gone);
        self.tracks.retain(|key, _| !gone(key));
        self.limitation.retain(|key, _| !gone(key));
        self.seen.retain(|key| !gone(key));
        let keys: Vec<TrackKey> = self.windows.keys().filter(|k| gone(k)).cloned().collect();
        let end = now_unix_nanos();
        keys.into_iter()
            .filter_map(|key| self.windows.remove(&key))
            .map(|window| window.into_queued(end))
            .collect()
    }

    /// Close `session`'s open windows now — their records then carry the attributes they were
    /// captured under — keeping each track's cumulative state for the windows that follow.
    pub fn split(&mut self, session: &ScopeState) -> Vec<QueuedEvent> {
        self.end_polls(|(trace_id, ..)| *trace_id == session.trace_id);
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

    /// How many tracks the accumulator holds state for (windows, tracks, CPU counters), with the
    /// polls recorded so far read into their windows.
    #[cfg(test)]
    pub fn tracked(&mut self) -> (usize, usize, usize) {
        self.end_polls(|_| true);
        (self.windows.len(), self.tracks.len(), self.limitation.len())
    }

    /// Record a reading under the unit tests' one session.
    #[cfg(test)]
    pub fn record(&mut self, sample: RtcStatsSample) {
        self.record_in(sample, &test_session());
    }

    /// Close every window and keep only the events.
    #[cfg(test)]
    pub fn close_events(&mut self) -> Vec<TelemetryEvent> {
        self.close().into_iter().map(|q| q.event).collect()
    }

    /// The encoder was CPU-starved within the last [`CPU_LIMITED_HOLD`]: stretch the cadence,
    /// like thermal pressure.
    pub fn cpu_limited(&self) -> bool {
        self.cpu_limited_until.is_some_and(|t| Instant::now() < t)
    }

    /// A cpu limitation counter that grew since the previous poll means the encoder was starved
    /// in between. Checked as each reading arrives, against the previous poll only, so the order
    /// of a poll's streams changes nothing.
    fn check_limitation(&mut self, key: &TrackKey, sample: &RtcStatsSample) {
        if sample.direction != StreamDirection::Outbound {
            return;
        }
        let (Some(current), Some(&previous)) =
            (sample.quality_limitation_cpu_ms, self.limitation.get(key))
        else {
            return;
        };
        if current > previous {
            self.cpu_limited_until = Some(Instant::now() + CPU_LIMITED_HOLD);
        }
    }

    /// A finished poll's cpu limitation counter is the baseline for the next poll. A poll without
    /// the counter says nothing: it must not reset the baseline.
    fn set_limitation_baseline(&mut self, key: &TrackKey, reading: &RtcStatsSample) {
        if reading.direction != StreamDirection::Outbound {
            return;
        }
        if let Some(current) = reading.quality_limitation_cpu_ms {
            self.limitation.insert(key.clone(), current);
        }
    }

    /// Drop everything (opt-out); returns how many RTC records went, each once: the open windows,
    /// and the first polls that had not opened their window yet.
    pub fn clear(&mut self) -> u64 {
        let first_polls = self
            .tracks
            .iter()
            .filter(|(key, track)| track.poll.is_some() && !self.windows.contains_key(*key))
            .count();
        let open = (self.windows.len() + first_polls) as u64;
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

    /// Simulcast layers make one series per track: its bytes grow by every layer's growth, so a
    /// stalled layer does not stall the track.
    #[test]
    fn simulcast_layers_fold_into_one_track_series() {
        let mut windows = StatsWindows::default();
        let layer = |id: &str, bytes: u64, fps: f64, second: u64| RtcStatsSample {
            bytes: Some(bytes),
            frames_per_second: Some(fps),
            layer: Some(id.into()),
            timestamp_ns: Some(second * 1_000_000_000),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        windows.record(layer("h", 1_000, 30.0, 1));
        windows.record(layer("f", 4_000, 30.0, 1));
        // The top layer stalls; the half layer keeps sending.
        windows.record(layer("h", 3_000, 30.0, 2));
        windows.record(layer("f", 4_000, 0.0, 2));
        let events = windows.close_events();
        assert_eq!(events.len(), 1, "one window per track, not per layer");
        assert_eq!(attr(&events[0], "lk.rtc.bytes"), Some(AttributeValue::Int(2_000)), "h grew");
        let body = events[0].body.clone().unwrap_or_default();
        assert!(body.contains("16 kbps"), "a stalled layer is not a stalled track: {body}");
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

    #[tokio::test(start_paused = true)]
    async fn a_reading_without_the_cpu_counter_keeps_the_baseline() {
        let mut windows = StatsWindows::default();
        let reading = |cpu_ms| RtcStatsSample {
            quality_limitation_cpu_ms: cpu_ms,
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        for cpu_ms in [Some(5_000), None, Some(5_000)] {
            windows.record(reading(cpu_ms));
        }
        assert!(!windows.cpu_limited(), "the counter did not grow");
    }

    #[test]
    fn a_split_window_keeps_its_tracks_state_through_the_next_close() {
        let mut windows = StatsWindows::default();
        let session = test_session();
        windows.record_in(
            RtcStatsSample {
                bytes: Some(1_000),
                quality_limitation_cpu_ms: Some(0),
                layer: Some("f".into()),
                ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
            },
            &session,
        );
        windows.split(&session);
        windows.close();
        assert_eq!(windows.tracked(), (0, 1, 1), "the track is live: it read since the last close");
        windows.close();
        assert_eq!(windows.tracked(), (0, 0, 0), "a whole window without a reading retires it");
    }

    fn stat(kind: &str, id: &str, members: &[(&str, AttributeValue)]) -> RtcStat {
        RtcStat {
            kind: kind.into(),
            id: id.into(),
            members: members.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        }
    }

    #[test]
    fn inbound_rtt_comes_from_the_selected_candidate_pair() {
        let inbound = stat("inbound-rtp", "IN", &[("bytesReceived", AttributeValue::Int(1))]);
        let pair = |id: &str, rtt: f64, nominated: bool| {
            stat(
                "candidate-pair",
                id,
                &[
                    ("nominated", AttributeValue::Bool(nominated)),
                    ("state", AttributeValue::Str("succeeded".into())),
                    ("currentRoundTripTime", AttributeValue::Double(rtt)),
                ],
            )
        };
        let rtt = |report: Vec<RtcStat>| {
            samples_from_report("TR_1", TrackKind::Audio, StreamDirection::Inbound, &report, None)
                [0]
            .rtt_ms
        };
        // Without a name from the transport: the first pair that is nominated and succeeded.
        let fallback = vec![inbound.clone(), pair("A", 0.3, false), pair("B", 0.04, true)];
        assert_eq!(rtt(fallback), Some(40.0), "a stale succeeded pair listed first is skipped");
        // The pair the transport names wins over any other.
        let transport =
            stat("transport", "T", &[("selectedCandidatePairId", AttributeValue::Str("B".into()))]);
        let named = vec![inbound, pair("A", 0.3, true), pair("B", 0.04, false), transport];
        assert_eq!(rtt(named), Some(40.0));
    }

    #[test]
    fn non_finite_numbers_are_no_reading() {
        let report = [stat(
            "inbound-rtp",
            "IN",
            &[
                ("jitter", AttributeValue::Str("NaN".into())),
                ("bytesReceived", AttributeValue::Double(f64::INFINITY)),
            ],
        )];
        let sample =
            &samples_from_report("TR_1", TrackKind::Audio, StreamDirection::Inbound, &report, None)
                [0];
        assert_eq!((sample.jitter_ms, sample.bytes), (None, None));
    }

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
                    ("state", AttributeValue::Str("succeeded".into())),
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

    /// Two rooms in one process subscribed to the same published track keep two windows, each filed
    /// under its own session.
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
            let id = Some(AttributeValue::Str(session.hex().to_owned()));
            assert_eq!(attr(&window.event, "session.id"), id, "decorated at capture");
        }
    }

    /// Per-track state follows the live tracks — retired when a track leaves, when its session
    /// ends, and when it goes a whole window without a reading.
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

    /// The bitrate is the bytes between the window's first and last reading over the time between
    /// them, not over the window: readings twice per window end before the window does.
    #[test]
    fn the_bitrate_spans_the_readings_not_the_window() {
        let mut first = RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound);
        first.bytes = Some(0);
        first.timestamp_ns = Some(0);
        let mut last = first.clone();
        last.bytes = Some(300_000);
        last.timestamp_ns = Some(30_000_000_000);
        let mut window = Window::open(0, first.clone(), test_session());
        window.add(last);
        let body = window.into_event(60_000_000_000).body.unwrap_or_default();
        assert!(body.contains("80 kbps"), "{body}");
        let alone = Window::open(0, first, test_session()).into_event(60_000_000_000);
        let body = alone.body.unwrap_or_default();
        assert!(!body.contains("kbps"), "one reading has no bitrate: {body}");
    }

    /// One poll's simulcast layers are one reading: the order the report lists them in does not
    /// change the frame-rate gauges.
    #[test]
    fn the_order_of_a_reports_layers_does_not_change_the_gauges() {
        let gauges = |order: [(&str, f64); 2]| {
            let mut windows = StatsWindows::default();
            for (layer, fps) in order {
                let mut sample =
                    RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound);
                sample.layer = Some(layer.into());
                sample.frames_per_second = Some(fps);
                sample.timestamp_ns = Some(1);
                windows.record(sample);
            }
            let event = windows.close_events().pop().expect("a window");
            let fps: Vec<_> =
                event.attributes.into_iter().filter(|a| a.key.starts_with("lk.rtc.fps")).collect();
            (fps, event.body)
        };
        let (h_first, body) = gauges([("h", 10.0), ("f", 30.0)]);
        assert_eq!(h_first, gauges([("f", 30.0), ("h", 10.0)]).0);
        assert!(body.unwrap_or_default().contains("30 fps"));
    }

    /// A poll's layers make one reading in any report order: a layer without an RTT does not
    /// erase the RTT another layer has, and of two RTTs the larger counts.
    #[test]
    fn a_polls_reading_keeps_what_any_layer_reports_in_either_order() {
        let layer = |rid: &str| {
            stat("outbound-rtp", &format!("OUT_{rid}"), &[("rid", AttributeValue::Str(rid.into()))])
        };
        let rtt = |rid: &str, secs: f64| {
            stat(
                "remote-inbound-rtp",
                &format!("RI_{rid}"),
                &[
                    ("localId", AttributeValue::Str(format!("OUT_{rid}"))),
                    ("roundTripTime", AttributeValue::Double(secs)),
                ],
            )
        };
        let rtt_avg = |report: Vec<RtcStat>| {
            let mut windows = StatsWindows::default();
            let outbound = StreamDirection::Outbound;
            for sample in samples_from_report("TR_1", TrackKind::Video, outbound, &report, Some(1))
            {
                windows.record(sample);
            }
            attr(&windows.close_events().pop().expect("a window"), "lk.rtc.rtt_ms.avg")
        };
        let (f, h, q) = (layer("f"), layer("h"), layer("q"));
        for report in [vec![f.clone(), h.clone()], vec![h, f.clone()]] {
            let report = [report, vec![rtt("f", 0.042)]].concat();
            assert_eq!(rtt_avg(report), Some(AttributeValue::Double(42.0)), "only `f` has one");
        }
        for report in [vec![f.clone(), q.clone()], vec![q, f]] {
            let report = [report, vec![rtt("f", 0.042), rtt("q", 0.05)]].concat();
            assert_eq!(rtt_avg(report), Some(AttributeValue::Double(50.0)), "the larger one");
        }
    }

    /// One outbound reading: stream `id` with `bytes` bytes and as many packets, at `second`.
    fn stream(id: &str, bytes: Option<u64>, second: u64) -> RtcStatsSample {
        RtcStatsSample {
            layer: Some(id.into()),
            bytes,
            packets: bytes,
            timestamp_ns: Some(second * 1_000_000_000),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        }
    }

    /// The track's bytes and packets in the window `windows` closes now.
    fn totals(windows: &mut StatsWindows) -> (Option<AttributeValue>, Option<AttributeValue>) {
        let event = windows.close_events().pop().expect("a window");
        (attr(&event, "lk.rtc.bytes"), attr(&event, "lk.rtc.packets"))
    }

    fn both(n: i64) -> (Option<AttributeValue>, Option<AttributeValue>) {
        (Some(AttributeValue::Int(n)), Some(AttributeValue::Int(n)))
    }

    /// A stream that leaves and reports again only sets its starting point on its return, so
    /// nothing it sent before is counted twice; what it sends after counts.
    #[test]
    fn a_stream_that_reports_again_adds_only_its_growth() {
        let mut windows = StatsWindows::default();
        for id in 0..9 {
            windows.record(stream(&format!("layer-{id}"), Some(1), id + 1));
        }
        assert_eq!(totals(&mut windows), both(0), "first polls only set starting points");
        windows.record(stream("layer-0", Some(1), 10));
        assert_eq!(totals(&mut windows), both(0), "no stream sent another byte");
        windows.record(stream("layer-0", Some(3), 11));
        assert_eq!(totals(&mut windows), both(2), "what it sent since counts");
    }

    /// A counter a stream leaves out never becomes a starting point of zero: the next value
    /// known only sets the starting point, and growth after it counts.
    #[test]
    fn an_absent_counter_never_becomes_a_zero_starting_point() {
        let mut windows = StatsWindows::default();
        for id in 0..9 {
            windows.record(stream(&format!("layer-{id}"), Some(1), id + 1));
        }
        windows.close_events();
        windows.record(stream("layer-0", None, 10));
        windows.close_events();
        windows.record(stream("layer-0", Some(1), 11));
        assert_eq!(totals(&mut windows), both(0), "no stream sent anything more");
        windows.record(stream("layer-0", None, 12));
        windows.record(stream("layer-0", Some(4), 13));
        assert_eq!(totals(&mut windows), both(3), "growth across a poll without the counter");
    }

    /// The same poll in any order gives the same totals, here ten streams at once.
    #[test]
    fn a_polls_totals_do_not_depend_on_its_order() {
        let totals_for = |reverse: bool| {
            let mut windows = StatsWindows::default();
            let mut ids: Vec<u64> = (1..=10).collect();
            if reverse {
                ids.reverse();
            }
            for second in [1, 2] {
                for &id in &ids {
                    windows.record(stream(&format!("layer-{id}"), Some(id * second), second));
                }
            }
            totals(&mut windows)
        };
        assert_eq!(totals_for(false), totals_for(true), "same polls in reverse order");
        assert_eq!(totals_for(false), both(55), "each stream grew by its own count");
    }

    /// Every order of a poll's streams gives the same window: totals, gauges and body.
    #[test]
    fn every_order_of_a_poll_gives_the_same_window() {
        let poll = |second: u64| {
            [("f", 3_000, 30.0, 0.05), ("h", 2_000, 15.0, 0.04), ("q", 1_000, 7.0, 0.06)].map(
                |(id, bytes, fps, rtt)| RtcStatsSample {
                    frames_per_second: Some(fps / second as f64),
                    rtt_ms: Some(rtt * 1_000.0),
                    ..stream(id, Some(bytes * second), second)
                },
            )
        };
        let orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let windows: Vec<_> = orders
            .iter()
            .map(|order| {
                let mut windows = StatsWindows::default();
                for second in [1, 2] {
                    let readings = poll(second);
                    for &i in order {
                        windows.record(readings[i].clone());
                    }
                }
                let event = windows.close_events().pop().expect("a window");
                let mut attributes: Vec<_> = event
                    .attributes
                    .into_iter()
                    // All but the window's length, which depends on when it closes.
                    .filter(|a| a.key.starts_with("lk.rtc.") && a.key != "lk.rtc.window_ms")
                    .map(|a| format!("{}={:?}", a.key, a.value))
                    .collect();
                attributes.sort();
                (attributes, event.body)
            })
            .collect();
        assert!(windows.iter().all(|w| *w == windows[0]), "{windows:#?}");
    }

    /// A falling frame rate comes from the current poll only, in either order of its layers.
    #[test]
    fn a_falling_frame_rate_comes_from_the_current_poll_only() {
        let reading = |layer: &str, fps, timestamp_ns| RtcStatsSample {
            layer: Some(layer.into()),
            frames_per_second: Some(fps),
            timestamp_ns: Some(timestamp_ns),
            ..RtcStatsSample::new("TR_1", TrackKind::Video, StreamDirection::Outbound)
        };
        for order in [[("h", 5.0), ("f", 10.0)], [("f", 10.0), ("h", 5.0)]] {
            let mut windows = StatsWindows::default();
            windows.record(reading("f", 30.0, 1));
            windows.record(reading("h", 10.0, 1));
            windows.close_events();
            for (layer, fps) in order {
                windows.record(reading(layer, fps, 2));
            }
            let event = windows.close_events().pop().expect("a window");
            assert_eq!(attr(&event, "lk.rtc.fps.avg"), Some(AttributeValue::Double(10.0)));
        }
    }

    /// A track keeps one report's streams at most, however many stream ids it went through.
    #[test]
    fn a_track_keeps_one_reports_streams() {
        let mut windows = StatsWindows::default();
        for id in 0..1000 {
            windows.record(stream(&format!("outbound-rtp-{id}"), Some(1), id + 1));
            windows.close_events();
        }
        let kept = windows.tracks.values().next().expect("the track").previous.len();
        assert_eq!(kept, 1, "the previous poll's one stream");
    }

    /// Clearing forgets every stream, and the track starts over afterwards.
    #[test]
    fn clear_forgets_every_stream() {
        let mut windows = StatsWindows::default();
        for _ in 0..2 {
            for id in 0..9 {
                windows.record(stream(&format!("layer-{id}"), Some(1), id + 1));
            }
            windows.clear();
            assert_eq!(windows.tracked(), (0, 0, 0));
        }
    }

    /// A retired track's last window holds its session until it is sent, and clearing lets every
    /// session go.
    #[test]
    fn retire_and_clear_let_the_session_go() {
        let mut windows = StatsWindows::default();
        let inbound =
            |sid: &str| RtcStatsSample::new(sid, TrackKind::Audio, StreamDirection::Inbound);
        let scope = ScopeState::new();
        let session = Arc::downgrade(&scope);
        windows.record_in(inbound("TR_1"), &scope);
        let queued = windows.retire(&scope, None);
        drop(scope);
        assert!(session.upgrade().is_some(), "the last window holds it");
        drop(queued);
        assert!(session.upgrade().is_none(), "gone with the window");
        let scope = ScopeState::new();
        let session = Arc::downgrade(&scope);
        windows.record_in(inbound("TR_2"), &scope);
        drop(scope);
        windows.clear();
        assert!(session.upgrade().is_none(), "nothing kept after clear");
    }

    /// The running total is a layered outbound track's: an inbound track passes its counters
    /// through as reported, whatever stream ids its readings carry, while an outbound track with
    /// stream ids counts each stream's growth.
    #[test]
    fn only_a_layered_outbound_track_keeps_a_running_total() {
        let totals_for = |direction| {
            let mut windows = StatsWindows::default();
            [123, 200].map(|bytes| {
                windows
                    .record(RtcStatsSample { direction, ..stream("ssrc-1", Some(bytes), bytes) });
                totals(&mut windows)
            })
        };
        assert_eq!(totals_for(StreamDirection::Inbound), [both(123), both(200)], "as reported");
        assert_eq!(totals_for(StreamDirection::Outbound), [both(0), both(77)], "growth");
    }

    /// Clearing counts every RTC record it drops, each once: an open window, and a first poll
    /// that has not opened its window yet.
    #[test]
    fn clear_counts_every_window_and_first_poll_once() {
        let mut windows = StatsWindows::default();
        windows.record(stream("f", Some(100), 1));
        assert_eq!(windows.clear(), 1, "a pending first poll is one record");
        // A window with a pending poll of its own, and another track with only a pending poll.
        windows.record(stream("f", Some(100), 1));
        windows.record(stream("f", Some(200), 2));
        windows.record(RtcStatsSample { track_sid: "TR_2".into(), ..stream("f", Some(100), 1) });
        assert_eq!(windows.clear(), 2, "one per track, its pending poll included");
    }
}
