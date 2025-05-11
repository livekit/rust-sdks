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

use std::collections::HashMap;
use std::time::Duration;

use livekit_protocol::{self as proto, data_packet};
use proto::{DataPacket, MetricLabel, MetricSample, MetricsBatch, TimeSeriesMetric};
use tokio::sync::broadcast;

use crate::prelude::LocalTrackPublication;
use crate::room::{Room, RtcEngine};
use libwebrtc::stats::{InboundRtpStats, MediaSourceStats, OutboundRtpStats, RtcStats};

use super::id::{ParticipantIdentity, TrackSid};
use super::DataPacketKind;

type GenericMetricLabel<T> = (MetricLabel, T);

const QUALITY_LIMITATIONS: [GenericMetricLabel<&str>; 3] = [
    (MetricLabel::ClientVideoPublisherQualityLimitationDurationCpu, "cpu"),
    (MetricLabel::ClientVideoPublisherQualityLimitationDurationBandwidth, "bandwidth"),
    (MetricLabel::ClientVideoPublisherQualityLimitationDurationOther, "other"),
];

pub struct RtcMetricsManager {}

impl RtcMetricsManager {
    pub fn new() -> Self {
        Self {}
    }

    fn micro_to_milli(micros: i64) -> i64 {
        micros / 1000
    }

    fn get_or_create_index(&self, strings: &mut Vec<String>, string: String) -> u32 {
        let position: Option<usize> = strings.iter().position(|s| s == &string);

        match position {
            Some(index) => return (index as u32) + (MetricLabel::PredefinedMaxValue as u32),
            None => {
                strings.push(string);
                return ((strings.len() - 1) as u32) + (MetricLabel::PredefinedMaxValue as u32);
            }
        }
    }

    fn create_metric_sample(&self, timestamp_ms: i64, value: f32) -> MetricSample {
        MetricSample { timestamp_ms, value, ..Default::default() }
    }

    fn create_time_series(
        &self,
        label: MetricLabel,
        strings: &mut Vec<String>,
        samples: Vec<MetricSample>,
        identity: Option<&ParticipantIdentity>,
        track_sid: Option<&str>,
        rid: Option<&str>,
    ) -> TimeSeriesMetric {
        let mut time_series = TimeSeriesMetric {
            label: label as u32,
            samples,
            ..Default::default()
        };

        if let Some(id) = identity {
            time_series.participant_identity = self.get_or_create_index(strings, id.to_string());
        }

        if let Some(sid) = track_sid {
            time_series.track_sid = self.get_or_create_index(strings, sid.to_string());
        }

        if let Some(r) = rid {
            time_series.rid = self.get_or_create_index(strings, r.to_string());
        }

        time_series
    }

    fn create_time_series_for_metric(
        &self,
        strings: &mut Vec<String>,
        label: MetricLabel,
        stat_value: f32,
        stat_timestamp_us: i64,
        track_sid: String,
        rid: Option<&str>,
        identity: Option<&ParticipantIdentity>,
    ) -> TimeSeriesMetric {
        let timestamp_ms = Self::micro_to_milli(stat_timestamp_us);
        let sample = self.create_metric_sample(timestamp_ms, stat_value);

        self.create_time_series(label, strings, vec![sample], identity, Some(&track_sid), rid)
    }

    pub(crate) async fn collect_metrics(
        &self,
        room: &Room,
        rtc_engine: &RtcEngine,
        mut close_rx: broadcast::Receiver<()>,
    ) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                _ = close_rx.recv() => {
                    // Room is closing, exit the collection loop
                    return;
                }
            }

            match room.get_stats().await {
                Ok(stats) => {
                    let publisher_stats = stats.publisher_stats;
                    let subscriber_stats = stats.subscriber_stats;

                    tokio::join!(
                        self.collect_publisher_metrics(room, rtc_engine, &publisher_stats),
                        self.collect_subscriber_metrics(room, rtc_engine, &subscriber_stats)
                    );
                }
                Err(e) => {
                    log::error!("Failed to retrieve stats: {:?}", e);
                }
            }
        };
    }

    fn find_publisher_stats(
        &self,
        room: &Room,
        strings: &mut Vec<String>,
        stats: &[RtcStats],
        participant_identity: Option<ParticipantIdentity>,
    ) -> Vec<TimeSeriesMetric> {
        let mut media_sources: Vec<MediaSourceStats> = Vec::new();
        let mut video_tracks: HashMap<String, OutboundRtpStats> = HashMap::new();

        let track_publication = room.local_participant().track_publications();

        for stat in stats {
            if let RtcStats::MediaSource(media_source) = stat {
                // why is flattening the source not working?
                if media_source.source.kind == "video" {
                    media_sources.push(media_source.clone())
                }
            }
        }

        // TODO: would like to do this in a single pass, but sources are not available at the time
        for stat in stats {
            if let RtcStats::OutboundRtp(outbound_rtp) = stat {
                if outbound_rtp.stream.kind == "video" {
                    if let Some(track_sid) = self.get_published_track_sid(
                        &media_sources,
                        &outbound_rtp,
                        &track_publication,
                    ) {
                        video_tracks.insert(track_sid, outbound_rtp.clone());
                    }
                }
            }
        }

        let mut metrics: Vec<TimeSeriesMetric> = Vec::new();
        for (track_sid, outbound_rtp) in video_tracks {
            let durations = &outbound_rtp.outbound.quality_limitation_durations;
            let rid = outbound_rtp.outbound.rid.clone();

            for &(quality_label, key) in &QUALITY_LIMITATIONS {
                if let Some(&duration) = durations.get(key) {
                    let sample = self.create_metric_sample(
                        Self::micro_to_milli(outbound_rtp.rtc.timestamp),
                        duration as f32, // u64 to f32 conversion
                    );

                    metrics.push(self.create_time_series(
                        quality_label,
                        strings,
                        vec![sample],
                        participant_identity.as_ref(),
                        Some(&track_sid),
                        Some(&rid),
                    ));
                }
            }
        }

        metrics
    }

    fn get_published_track_sid(
        &self,
        media_source_stats: &[MediaSourceStats],
        track_stats: &OutboundRtpStats,
        track_publication: &HashMap<TrackSid, LocalTrackPublication>,
    ) -> Option<String> {
        let track_identifier = media_source_stats
            .iter()
            .find(|m| m.rtc.id == track_stats.outbound.media_source_id)?
            .source
            .track_identifier
            .clone();

        track_publication
            .iter()
            .find(|(_, track)| track.track().unwrap().rtc_track().id() == track_identifier)
            .map(|(sid, _)| sid.to_string())
    }

    async fn collect_publisher_metrics(
        &self,
        room: &Room,
        rtc_engine: &RtcEngine,
        stats: &Vec<RtcStats>,
    ) {
        let mut strings = Vec::new();
        let publisher_ts_metrics = self.find_publisher_stats(
            room,
            &mut strings,
            stats,
            room.local_participant().identity().into(),
        );

        self.send_metrics(rtc_engine, strings, publisher_ts_metrics).await;
    }

    fn find_subscriber_stats(
        &self,
        strings: &mut Vec<String>,
        stats: &[RtcStats],
        participant_identity: Option<ParticipantIdentity>,
    ) -> Vec<TimeSeriesMetric> {
        let mut video_tracks: Vec<InboundRtpStats> = Vec::new();
        let mut audio_tracks: Vec<InboundRtpStats> = Vec::new();

        for stat in stats {
            if let RtcStats::InboundRtp(inbound_rtp) = stat {
                match inbound_rtp.stream.kind.as_str() {
                    "audio" => audio_tracks.push(inbound_rtp.clone()),
                    "video" => video_tracks.push(inbound_rtp.clone()),
                    _ => log::warn!("Unknown stream kind: {}", inbound_rtp.stream.kind),
                }
            }
        }

        let mut metrics: Vec<TimeSeriesMetric> = Vec::new();

        for track in video_tracks {
            let timestamp_us = track.rtc.timestamp;
            let sid = track.inbound.track_identifier;
            let rid = None; // SFU only sends a single layer downstream, so no rid involved

            // Forced f32 conversions here, might lose some precision here or even overflow...
            let metrics_to_create: [GenericMetricLabel<f32>; 6] = [
                (MetricLabel::ClientVideoSubscriberFreezeCount, track.inbound.freeze_count as f32),
                (
                    MetricLabel::ClientVideoSubscriberTotalFreezeDuration,
                    track.inbound.total_freeze_duration as f32,
                ),
                (MetricLabel::ClientVideoSubscriberPauseCount, track.inbound.pause_count as f32),
                (
                    MetricLabel::ClientVideoSubscriberTotalPausesDuration,
                    track.inbound.total_pause_duration as f32,
                ),
                (
                    MetricLabel::ClientSubscriberJitterBufferDelay,
                    track.inbound.jitter_buffer_delay as f32,
                ),
                (
                    MetricLabel::ClientSubscriberJitterBufferEmittedCount,
                    track.inbound.jitter_buffer_emitted_count as f32,
                ),
            ];

            for (label, value) in metrics_to_create {
                metrics.push(self.create_time_series_for_metric(
                    strings,
                    label,
                    value,
                    timestamp_us,
                    sid.clone(),
                    rid,
                    participant_identity.as_ref(),
                ));
            }
        }

        for track in audio_tracks {
            let timestamp_us = track.rtc.timestamp;
            let sid = track.inbound.track_identifier;
            let rid = None; // Audio tracks don't need rid

            let metrics_to_create: [GenericMetricLabel<f32>; 5] = [
                (
                    MetricLabel::ClientAudioSubscriberConcealedSamples,
                    track.inbound.concealed_samples as f32,
                ),
                (
                    MetricLabel::ClientAudioSubscriberConcealmentEvents,
                    track.inbound.concealment_events as f32,
                ),
                (
                    MetricLabel::ClientAudioSubscriberSilentConcealedSamples,
                    track.inbound.silent_concealed_samples as f32,
                ),
                (
                    MetricLabel::ClientSubscriberJitterBufferDelay,
                    track.inbound.jitter_buffer_delay as f32,
                ),
                (
                    MetricLabel::ClientSubscriberJitterBufferEmittedCount,
                    track.inbound.jitter_buffer_emitted_count as f32,
                ),
            ];

            for (label, value) in metrics_to_create {
                metrics.push(self.create_time_series_for_metric(
                    strings,
                    label,
                    value,
                    timestamp_us,
                    sid.clone(),
                    rid,
                    participant_identity.as_ref(),
                ));
            }
        }

        metrics
    }

    async fn collect_subscriber_metrics(
        &self,
        room: &Room,
        rtc_engine: &RtcEngine,
        stats: &Vec<RtcStats>,
    ) {
        let mut strings: Vec<String> = Vec::new();
        let subscriber_ts_metrics = self.find_subscriber_stats(
            &mut strings,
            stats,
            room.local_participant().identity().into(),
        );

        self.send_metrics(rtc_engine, strings, subscriber_ts_metrics).await;
    }

    pub async fn send_metrics(
        &self,
        rtc_engine: &RtcEngine,
        strings: Vec<String>,
        metrics: Vec<TimeSeriesMetric>
    ) {
        if metrics.len() > 0 {
            let timestamp = metrics.first()
            .and_then(|metric| metric.samples.first())
            .map(|sample| sample.timestamp_ms)
            .unwrap();
            let data_packet = DataPacket {
                value: Some(data_packet::Value::Metrics(MetricsBatch {
                    str_data: strings,
                    time_series: metrics,
                    timestamp_ms: timestamp,
                    ..Default::default()
                })),
                ..Default::default()
            };

            if let Err(e) = rtc_engine.publish_data(data_packet, DataPacketKind::Reliable).await {
                log::warn!("Failed to publish metrics: {:?}", e);
            };
        }
    }
}