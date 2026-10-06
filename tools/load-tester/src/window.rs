use std::{collections::HashMap, time::Instant};

use livekit::{
    prelude::{ConnectionQuality, TrackKind},
    rtc_engine::SessionStats,
    webrtc::stats::{
        dictionaries::RemoteInboundRtpStreamStats, InboundRtpStats, OutboundRtpStats,
        QualityLimitationReason, RtcStats,
    },
};

use crate::{
    media::Ladder,
    record::{
        AudioWindow, Limitation, MediaKind, MediaWindow, ServerQuality, UplinkLayer, VideoWindow,
    },
};

impl From<TrackKind> for MediaKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => MediaKind::Audio,
            TrackKind::Video => MediaKind::Video,
        }
    }
}

impl From<QualityLimitationReason> for Limitation {
    fn from(reason: QualityLimitationReason) -> Self {
        match reason {
            QualityLimitationReason::None => Limitation::None,
            QualityLimitationReason::Cpu => Limitation::Cpu,
            QualityLimitationReason::Bandwidth => Limitation::Bandwidth,
            QualityLimitationReason::Other => Limitation::Other,
        }
    }
}

impl From<ConnectionQuality> for ServerQuality {
    fn from(quality: ConnectionQuality) -> Self {
        match quality {
            ConnectionQuality::Excellent => ServerQuality::Excellent,
            ConnectionQuality::Good => ServerQuality::Good,
            ConnectionQuality::Poor => ServerQuality::Poor,
            ConnectionQuality::Lost => ServerQuality::Lost,
        }
    }
}

fn ms_between(earlier: Instant, later: Instant) -> u32 {
    later.saturating_duration_since(earlier).as_millis() as u32
}

#[derive(Debug, Default)]
pub struct StatsByTrack<'a> {
    inbound: HashMap<&'a str, Inbound<'a>>,
    outbound: HashMap<&'a str, Vec<Outbound<'a>>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Inbound<'a> {
    rtp: &'a InboundRtpStats,
    rtt_ms: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Outbound<'a> {
    rtp: &'a OutboundRtpStats,
    remote: Option<&'a RemoteInboundRtpStreamStats>,
}

impl<'a> StatsByTrack<'a> {
    /// Indexes each peer connection's report separately, since stats ids repeat across them.
    pub fn new(stats: &'a SessionStats) -> Self {
        let mut tracks = Self::default();
        tracks.add(&stats.publisher_stats);
        tracks.add(&stats.subscriber_stats);
        tracks
    }

    pub fn inbound(&self, rtc_id: &str) -> Option<Inbound<'a>> {
        self.inbound.get(rtc_id).copied()
    }

    pub fn outbound(&self, rtc_id: &str) -> &[Outbound<'a>] {
        self.outbound.get(rtc_id).map_or(&[], Vec::as_slice)
    }

    fn add(&mut self, report: &'a [RtcStats]) {
        let mut selected_pairs = HashMap::new();
        let mut rtts = HashMap::new();
        let mut sources = HashMap::new();
        let mut remotes = HashMap::new();
        for stats in report {
            match stats {
                RtcStats::Transport(t) => {
                    selected_pairs
                        .insert(t.rtc.id.as_str(), t.transport.selected_candidate_pair_id.as_str());
                }
                RtcStats::CandidatePair(p) => {
                    let rtt_ms = (p.candidate_pair.current_round_trip_time * 1000.0) as f32;
                    rtts.insert(p.rtc.id.as_str(), rtt_ms);
                }
                RtcStats::MediaSource(m) => {
                    sources.insert(m.rtc.id.as_str(), m.source.track_identifier.as_str());
                }
                RtcStats::RemoteInboundRtp(r) => {
                    remotes.insert(r.remote_inbound.local_id.as_str(), &r.remote_inbound);
                }
                _ => {}
            }
        }
        for stats in report {
            match stats {
                RtcStats::InboundRtp(rtp) => {
                    let pair = selected_pairs.get(rtp.stream.transport_id.as_str());
                    let rtt_ms = pair.and_then(|id| rtts.get(id)).copied().unwrap_or(0.0);
                    let track = rtp.inbound.track_identifier.as_str();
                    self.inbound.entry(track).or_insert(Inbound { rtp, rtt_ms });
                }
                RtcStats::OutboundRtp(rtp) => {
                    let Some(&track) = sources.get(rtp.outbound.media_source_id.as_str()) else {
                        continue;
                    };
                    let remote = remotes.get(rtp.rtc.id.as_str()).copied();
                    self.outbound.entry(track).or_default().push(Outbound { rtp, remote });
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug)]
struct Sample {
    ssrc: u32,
    packets_received: u64,
    // packets_lost is signed (RFC 3550), so it can go backwards without a reset
    packets_lost: i64,
    jb_delay_s: f64,
    jb_emitted: u64,
    samples: u64,
    concealed: u64,
    concealment_events: u64,
    bytes: u64,
    frames_decoded: u32,
    frames_dropped: u32,
    freeze_count: u32,
    stall_s: f64,
    nacks: u32,
    plis: u32,
}

impl Sample {
    fn read(i: &InboundRtpStats) -> Self {
        let inbound = &i.inbound;
        Self {
            ssrc: i.stream.ssrc,
            packets_received: i.received.packets_received,
            packets_lost: i.received.packets_lost,
            jb_delay_s: inbound.jitter_buffer_delay,
            jb_emitted: inbound.jitter_buffer_emitted_count,
            samples: inbound.total_samples_received,
            concealed: inbound.concealed_samples,
            concealment_events: inbound.concealment_events,
            bytes: inbound.bytes_received,
            frames_decoded: inbound.frames_decoded,
            frames_dropped: inbound.frames_dropped,
            freeze_count: inbound.freeze_count,
            stall_s: inbound.total_freeze_duration + inbound.total_pause_duration,
            nacks: inbound.nack_count,
            plis: inbound.pli_count,
        }
    }

    fn is_reset_from(&self, prev: &Self) -> bool {
        self.ssrc != prev.ssrc
            || self.packets_received < prev.packets_received
            || self.jb_delay_s < prev.jb_delay_s
            || self.jb_emitted < prev.jb_emitted
            || self.samples < prev.samples
            || self.concealed < prev.concealed
            || self.concealment_events < prev.concealment_events
            || self.bytes < prev.bytes
            || self.frames_decoded < prev.frames_decoded
            || self.frames_dropped < prev.frames_dropped
            || self.freeze_count < prev.freeze_count
            || self.stall_s < prev.stall_s
            || self.nacks < prev.nacks
            || self.plis < prev.plis
    }
}

#[derive(Clone, Debug)]
pub struct VideoExpectation {
    pub ladder: Ladder,
    pub tile_height: u32,
}

#[derive(Debug)]
enum Media {
    Audio,
    Video { expect: VideoExpectation, unreported_stall_ms: u32 },
}

impl Media {
    fn decoder(&self, inbound: &InboundRtpStats) -> Option<String> {
        match self {
            Media::Audio => None,
            Media::Video { .. } => Some(inbound.inbound.decoder_implementation.clone()),
        }
    }

    fn reset(&mut self) {
        if let Media::Video { unreported_stall_ms, .. } = self {
            *unreported_stall_ms = 0;
        }
    }

    fn window(
        &mut self,
        prev: &Sample,
        now: &Sample,
        inbound: &InboundRtpStats,
        dur_ms: u32,
    ) -> MediaWindow {
        let packets_received = now.packets_received - prev.packets_received;
        let packets_lost = now.packets_lost - prev.packets_lost;
        let jb_delay_s = now.jb_delay_s - prev.jb_delay_s;
        let jb_emitted = now.jb_emitted - prev.jb_emitted;
        match self {
            Media::Audio => MediaWindow::Audio(AudioWindow {
                packets_received,
                packets_lost,
                samples: now.samples - prev.samples,
                concealed: now.concealed - prev.concealed,
                concealment_events: now.concealment_events - prev.concealment_events,
                jb_delay_s,
                jb_emitted,
            }),
            Media::Video { expect, unreported_stall_ms } => {
                let frames_decoded = now.frames_decoded - prev.frames_decoded;
                let stalled_ms = if frames_decoded == 0 {
                    *unreported_stall_ms += dur_ms;
                    dur_ms
                } else {
                    let reported_ms = ((now.stall_s - prev.stall_s) * 1000.0) as u32;
                    let charged = reported_ms.saturating_sub(*unreported_stall_ms).min(dur_ms);
                    *unreported_stall_ms = 0;
                    charged
                };
                let ladder = &expect.ladder;
                let requested = ladder.rung(expect.tile_height);
                let height = inbound.inbound.frame_height;
                let received = (height > 0).then(|| ladder.rung(height));
                MediaWindow::Video(VideoWindow {
                    packets_received,
                    packets_lost,
                    bytes: now.bytes - prev.bytes,
                    frames_decoded,
                    frames_dropped: now.frames_dropped - prev.frames_dropped,
                    freeze_count: now.freeze_count - prev.freeze_count,
                    stalled_ms,
                    height,
                    expected_height: ladder.layer(requested).height,
                    layer_fps: received.map_or(0.0, |r| ladder.layer(r).fps),
                    layers_below: received.map_or(0, |r| r.saturating_sub(requested) as u32),
                    jb_delay_s,
                    jb_emitted,
                    nacks: now.nacks - prev.nacks,
                    plis: now.plis - prev.plis,
                })
            }
        }
    }
}

#[derive(Debug)]
pub enum Observation {
    Nothing,
    FirstMedia { frame: Instant, ttff_ms: u32, decoder: Option<String> },
    Window { dur_ms: u32, rtt_ms: f32, media: MediaWindow },
}

#[derive(Debug)]
enum State {
    Waiting { since: Instant },
    Arrived { frame: Instant, ttff_ms: u32 },
    Flowing { prev: Sample, at: Instant },
}

#[derive(Debug)]
pub struct InboundTracker {
    media: Media,
    state: State,
}

impl InboundTracker {
    pub fn audio(since: Instant) -> Self {
        Self::new(since, Media::Audio)
    }

    pub fn video(since: Instant, expect: VideoExpectation) -> Self {
        Self::new(since, Media::Video { expect, unreported_stall_ms: 0 })
    }

    fn new(since: Instant, media: Media) -> Self {
        Self { media, state: State::Waiting { since } }
    }

    pub fn first_frame(&mut self, frame: Instant) {
        if let State::Waiting { since } = self.state {
            self.state = State::Arrived { frame, ttff_ms: ms_between(since, frame) };
        }
    }

    pub fn observe(&mut self, now: Instant, inbound: Option<Inbound>) -> Observation {
        let Some(Inbound { rtp, rtt_ms }) = inbound else {
            return Observation::Nothing;
        };
        match &mut self.state {
            State::Waiting { .. } => Observation::Nothing,
            State::Arrived { frame, ttff_ms } => {
                let (frame, ttff_ms) = (*frame, *ttff_ms);
                self.state = State::Flowing { prev: Sample::read(rtp), at: now };
                Observation::FirstMedia { frame, ttff_ms, decoder: self.media.decoder(rtp) }
            }
            State::Flowing { prev, at } => {
                let sample = Sample::read(rtp);
                if sample.is_reset_from(prev) {
                    self.media.reset();
                    *prev = sample;
                    *at = now;
                    return Observation::Nothing;
                }
                let dur_ms = ms_between(*at, now);
                let media = self.media.window(prev, &sample, rtp, dur_ms);
                *prev = sample;
                *at = now;
                Observation::Window { dur_ms, rtt_ms, media }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct UplinkWindow {
    pub dur_ms: u32,
    pub layers: Vec<UplinkLayer>,
}

#[derive(Debug, Default)]
pub struct UplinkTracker {
    prev: Option<(Instant, HashMap<String, u64>)>,
}

impl UplinkTracker {
    pub fn observe(&mut self, now: Instant, outbound: &[Outbound]) -> Option<UplinkWindow> {
        let bytes = outbound.iter().map(|o| (o.rtp.outbound.rid.clone(), o.rtp.sent.bytes_sent));
        let (at, before) = self.prev.replace((now, bytes.collect()))?;
        let dur_ms = ms_between(at, now);
        let layers = outbound
            .iter()
            .map(|&Outbound { rtp: out, remote }| {
                let sent = before
                    .get(&out.outbound.rid)
                    .map_or(0, |b| out.sent.bytes_sent.saturating_sub(*b));
                UplinkLayer {
                    rid: out.outbound.rid.clone(),
                    bitrate_bps: if dur_ms > 0 {
                        (sent * 8 * 1000 / dur_ms as u64) as u32
                    } else {
                        0
                    },
                    fps: out.outbound.frames_per_second as f32,
                    height: out.outbound.frame_height,
                    limitation: Limitation::from(out.outbound.quality_limitation_reason),
                    remote_loss: remote.map_or(0.0, |r| r.fraction_lost as f32),
                    rtt_ms: remote.map_or(0.0, |r| (r.round_trip_time * 1000.0) as f32),
                }
            })
            .collect();
        Some(UplinkWindow { dur_ms, layers })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use livekit::webrtc::stats::{
        dictionaries, CandidatePairStats, MediaSourceStats, OutboundRtpStats,
        RemoteInboundRtpStats, TransportStats,
    };

    use super::*;
    use crate::record::NULL_DECODER;

    struct Clock(Instant);

    impl Clock {
        fn new() -> Self {
            Self(Instant::now())
        }

        fn at(&self, ms: u64) -> Instant {
            self.0 + Duration::from_millis(ms)
        }
    }

    fn inbound(kind: &str, ssrc: u32) -> InboundRtpStats {
        InboundRtpStats {
            stream: dictionaries::RtpStreamStats { ssrc, kind: kind.into(), ..Default::default() },
            ..Default::default()
        }
    }

    fn video(ssrc: u32, packets: u64, frames: u32) -> InboundRtpStats {
        let mut i = inbound("video", ssrc);
        i.received.packets_received = packets;
        i.inbound.bytes_received = packets * 1000;
        i.inbound.frames_decoded = frames;
        i.inbound.jitter_buffer_emitted_count = frames as u64;
        i.inbound.frame_height = 720;
        i.inbound.decoder_implementation = NULL_DECODER.into();
        i
    }

    fn with_freeze(mut i: InboundRtpStats, freeze_s: f64, pause_s: f64) -> InboundRtpStats {
        i.inbound.total_freeze_duration = freeze_s;
        i.inbound.total_pause_duration = pause_s;
        i.inbound.freeze_count += 1;
        i
    }

    fn audio(ssrc: u32, packets: u64, samples: u64, concealed: u64) -> InboundRtpStats {
        let mut i = inbound("audio", ssrc);
        i.received.packets_received = packets;
        i.inbound.total_samples_received = samples;
        i.inbound.jitter_buffer_emitted_count = samples;
        i.inbound.concealed_samples = concealed;
        i
    }

    fn seen(i: &InboundRtpStats) -> Option<Inbound<'_>> {
        Some(Inbound { rtp: i, rtt_ms: 40.0 })
    }

    fn expectation() -> VideoExpectation {
        VideoExpectation {
            ladder: Ladder::of(&crate::media::test_profile(720, 30)),
            tile_height: 720,
        }
    }

    fn flowing_video(clock: &Clock) -> InboundTracker {
        let mut t = InboundTracker::video(clock.at(0), expectation());
        t.first_frame(clock.at(400));
        let first = t.observe(clock.at(500), seen(&video(1, 10, 5)));
        assert!(matches!(first, Observation::FirstMedia { .. }), "{first:?}");
        t
    }

    fn video_window(obs: Observation) -> (u32, VideoWindow) {
        match obs {
            Observation::Window { dur_ms, media: MediaWindow::Video(v), .. } => (dur_ms, v),
            other => panic!("expected a video window, got {other:?}"),
        }
    }

    #[test]
    fn stall_in_progress_is_charged_once_across_windows() {
        let clock = Clock::new();
        let mut t = flowing_video(&clock);

        let (dur, w1) = video_window(t.observe(clock.at(5_500), seen(&video(1, 500, 155))));
        assert_eq!(dur, 5_000);
        assert_eq!(w1.stalled_ms, 0);

        let (_, w2) = video_window(t.observe(clock.at(10_500), seen(&video(1, 500, 155))));
        assert_eq!(
            w2.stalled_ms, 5_000,
            "a window with no new frames is stalled for its whole duration"
        );
        assert_eq!(w2.frames_decoded, 0);

        let resumed = with_freeze(video(1, 600, 200), 6.2, 0.0);
        let (_, w3) = video_window(t.observe(clock.at(15_500), seen(&resumed)));
        assert_eq!(w3.stalled_ms, 1_200, "libwebrtc reports 6.2s; 5s was already charged");
        assert_eq!(w3.freeze_count, 1);
    }

    #[test]
    fn ssrc_change_resets_without_emitting_a_window() {
        let clock = Clock::new();
        let mut t = flowing_video(&clock);
        let obs = t.observe(clock.at(5_500), seen(&video(2, 3, 1)));
        assert!(matches!(obs, Observation::Nothing), "{obs:?}");
        let (dur, w) = video_window(t.observe(clock.at(10_500), seen(&video(2, 253, 151))));
        assert_eq!(dur, 5_000, "the next window starts at the reset sample");
        assert_eq!(w.frames_decoded, 150);
        let obs = t.observe(clock.at(15_500), seen(&video(2, 3, 1)));
        assert!(matches!(obs, Observation::Nothing), "counters went backwards: {obs:?}");
    }

    #[test]
    fn stats_join_only_within_their_own_peer_connection_report() {
        let transport = |id: &str, selected: &str| {
            let mut t = TransportStats::default();
            t.rtc.id = id.into();
            t.transport.selected_candidate_pair_id = selected.into();
            RtcStats::Transport(t)
        };
        let pair = |id: &str, rtt_s: f64| {
            let mut p = CandidatePairStats::default();
            p.rtc.id = id.into();
            p.candidate_pair.nominated = true;
            p.candidate_pair.current_round_trip_time = rtt_s;
            RtcStats::CandidatePair(p)
        };
        let source = |id: &str, rtc_id: &str| {
            let mut m = MediaSourceStats::default();
            m.rtc.id = id.into();
            m.source.track_identifier = rtc_id.into();
            RtcStats::MediaSource(m)
        };
        let outbound = |id: &str, source: &str, rid: &str| {
            let mut o = OutboundRtpStats::default();
            o.rtc.id = id.into();
            o.outbound.media_source_id = source.into();
            o.outbound.rid = rid.into();
            RtcStats::OutboundRtp(o)
        };
        let mut remote = RemoteInboundRtpStats::default();
        remote.remote_inbound.local_id = "OT01V1".into();
        let mut received = audio(42, 9, 480, 0);
        received.inbound.track_identifier = "remote-mic".into();
        received.stream.transport_id = "T01".into();

        let stats = SessionStats {
            publisher_stats: vec![
                transport("T01", "CP2"),
                pair("CP1", 0.09),
                pair("CP2", 0.07),
                source("SA1", "mic"),
                source("SV2", "camera"),
                outbound("OT01A1", "SA1", ""),
                outbound("OT01V1", "SV2", "f"),
                outbound("OT01V2", "SV2", "q"),
                RtcStats::RemoteInboundRtp(remote),
            ],
            subscriber_stats: vec![
                transport("T01", "CP1"),
                pair("CP0", 0.3),
                pair("CP1", 0.05),
                RtcStats::InboundRtp(received),
            ],
        };
        let tracks = StatsByTrack::new(&stats);

        let mic = tracks.inbound("remote-mic").expect("routed by trackIdentifier");
        assert_eq!(mic.rtp.stream.ssrc, 42);
        assert_eq!(
            mic.rtt_ms, 50.0,
            "the subscriber's selected CP1, not the earlier nominated CP0 or the publisher's CP1"
        );
        let camera: Vec<(&str, bool)> = tracks
            .outbound("camera")
            .iter()
            .map(|o| (o.rtp.outbound.rid.as_str(), o.remote.is_some()))
            .collect();
        assert_eq!(camera, [("f", true), ("q", false)], "remote-inbound joined by localId");
        assert_eq!(tracks.outbound("mic").len(), 1);
    }
}
