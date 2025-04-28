// Copyright 2023 LiveKit, Inc.
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

use crate::proto;
use livekit_protocol as protocol;

impl From<protocol::MetricLabel> for proto::MetricLabel {
    fn from(value: protocol::MetricLabel) -> Self {
        match value {
            protocol::MetricLabel::AgentsLlmTtft => Self::AgentsLlmTtft,
            protocol::MetricLabel::AgentsSttTtft => Self::AgentsSttTtft,
            protocol::MetricLabel::AgentsTtsTtfb => Self::AgentsTtsTtfb,
            protocol::MetricLabel::ClientVideoSubscriberFreezeCount => Self::ClientVideoSubscriberFreezeCount,
            protocol::MetricLabel::ClientVideoSubscriberTotalFreezeDuration => Self::ClientVideoSubscriberTotalFreezeDuration,
            protocol::MetricLabel::ClientVideoSubscriberPauseCount => Self::ClientVideoSubscriberPauseCount,
            protocol::MetricLabel::ClientVideoSubscriberTotalPausesDuration => Self::ClientVideoSubscriberTotalPausesDuration,
            protocol::MetricLabel::ClientAudioSubscriberConcealedSamples => Self::ClientAudioSubscriberConcealedSamples,
            protocol::MetricLabel::ClientAudioSubscriberSilentConcealedSamples => Self::ClientAudioSubscriberSilentConcealedSamples,
            protocol::MetricLabel::ClientAudioSubscriberConcealmentEvents => Self::ClientAudioSubscriberConcealmentEvents,
            protocol::MetricLabel::ClientAudioSubscriberInterruptionCount => Self::ClientAudioSubscriberInterruptionCount,
            protocol::MetricLabel::ClientAudioSubscriberTotalInterruptionDuration => Self::ClientAudioSubscriberTotalInterruptionDuration,
            protocol::MetricLabel::ClientSubscriberJitterBufferDelay => Self::ClientSubscriberJitterBufferDelay,
            protocol::MetricLabel::ClientSubscriberJitterBufferEmittedCount => Self::ClientSubscriberJitterBufferEmittedCount,
            protocol::MetricLabel::ClientVideoPublisherQualityLimitationDurationBandwidth => Self::ClientVideoPublisherQualityLimitationDurationBandwidth,
            protocol::MetricLabel::ClientVideoPublisherQualityLimitationDurationCpu => Self::ClientVideoPublisherQualityLimitationDurationCpu,
            protocol::MetricLabel::ClientVideoPublisherQualityLimitationDurationOther => Self::ClientVideoPublisherQualityLimitationDurationOther,
            protocol::MetricLabel::PublisherRtt => Self::PublisherRtt,
            protocol::MetricLabel::ServerMeshRtt => Self::ServerMeshRtt,
            protocol::MetricLabel::SubscriberRtt => Self::SubscriberRtt,
            protocol::MetricLabel::PredefinedMaxValue => Self::MetricLabelPredefinedMaxValue,
        }
    }
}

impl From<protocol::MetricSample> for proto::MetricSample {
    fn from(value: protocol::MetricSample) -> Self {
        Self {
            timestamp_ms: value.timestamp_ms,
            normalized_timestamp: value.normalized_timestamp.map(Into::into),
            value: value.value,
        }
    }
}

impl From<protocol::TimeSeriesMetric> for proto::TimeSeriesMetric {
    fn from(value: protocol::TimeSeriesMetric) -> Self {
        Self {
            label: value.label,
            participant_identity: value.participant_identity,
            track_sid: value.track_sid,
            samples: value.samples.into_iter().map(Into::into).collect(),
            rid: value.rid,
        }
    }
}

impl From<protocol::EventMetric> for proto::EventMetric {
    fn from(value: protocol::EventMetric) -> Self {
        Self {
            label: value.label,
            participant_identity: value.participant_identity,
            track_sid: value.track_sid,
            start_timestamp_ms: value.start_timestamp_ms,
            end_timestamp_ms: value.end_timestamp_ms,
            normalized_start_timestamp: value.normalized_start_timestamp.map(Into::into),
            normalized_end_timestamp: value.normalized_end_timestamp.map(Into::into),
            metadata: value.metadata,
            rid: value.rid,
        }
    }
}

impl From<protocol::MetricsBatch> for proto::MetricsBatch {
    fn from(value: protocol::MetricsBatch) -> Self {
        Self {
            timestamp_ms: value.timestamp_ms,
            normalized_timestamp: value.normalized_timestamp.map(Into::into),
            str_data: value.str_data,
            time_series: value.time_series.into_iter().map(Into::into).collect(),
            events: value.events.into_iter().map(Into::into).collect(),
        }
    }
}

