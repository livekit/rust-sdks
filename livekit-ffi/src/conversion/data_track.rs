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

use crate::proto;
use livekit::{
    data_track::{
        DataTrackFrame, DataTrackInfo, DataTrackOptions, DataTrackReliability,
        DataTrackSubscribeError, PublishError, PushFrameError, PushFrameErrorReason,
        RemoteDataTrackPipelineOptions,
    },
    prelude::DataTrackSubscribeOptions,
};

impl From<proto::DataTrackOptions> for DataTrackOptions {
    fn from(options: proto::DataTrackOptions) -> Self {
        let reliability = options
            .reliability
            .and_then(|value| proto::DataTrackReliability::try_from(value).ok())
            .map(DataTrackReliability::from)
            .unwrap_or(DataTrackReliability::Lossy);
        Self::new(options.name).reliability(reliability)
    }
}

impl From<DataTrackInfo> for proto::DataTrackInfo {
    fn from(info: DataTrackInfo) -> Self {
        Self {
            name: info.name().to_string(),
            sid: info.sid().to_string(),
            uses_e2ee: info.uses_e2ee(),
            reliability: Some(proto::DataTrackReliability::from(info.reliability()) as i32),
        }
    }
}

impl From<proto::DataTrackReliability> for DataTrackReliability {
    fn from(reliability: proto::DataTrackReliability) -> Self {
        match reliability {
            proto::DataTrackReliability::DtrReliable => Self::Reliable,
            proto::DataTrackReliability::DtrLossy => Self::Lossy,
        }
    }
}

impl From<DataTrackReliability> for proto::DataTrackReliability {
    fn from(reliability: DataTrackReliability) -> Self {
        match reliability {
            DataTrackReliability::Lossy => Self::DtrLossy,
            DataTrackReliability::Reliable => Self::DtrReliable,
        }
    }
}

impl From<DataTrackFrame> for proto::DataTrackFrame {
    fn from(frame: DataTrackFrame) -> Self {
        Self { payload: frame.payload().into(), user_timestamp: frame.user_timestamp() }
    }
}

impl From<proto::DataTrackFrame> for DataTrackFrame {
    fn from(msg: proto::DataTrackFrame) -> Self {
        let mut frame = Self::new(msg.payload);
        if let Some(timestamp) = msg.user_timestamp {
            frame = frame.with_user_timestamp(timestamp);
        }
        frame
    }
}

impl From<proto::RemoteDataTrackPipelineOptions> for RemoteDataTrackPipelineOptions {
    fn from(msg: proto::RemoteDataTrackPipelineOptions) -> Self {
        let mut options = Self::new();
        if let Some(max_partial_frames) = msg.max_partial_frames {
            options = options.with_max_partial_frames(max_partial_frames as usize);
        }
        options
    }
}

impl From<proto::DataTrackSubscribeOptions> for DataTrackSubscribeOptions {
    fn from(msg: proto::DataTrackSubscribeOptions) -> Self {
        let mut options = Self::new();
        if let Some(buffer_size) = msg.buffer_size {
            options = options.with_buffer_size(buffer_size as usize);
        }
        if let Some(reliability) =
            msg.reliability.and_then(|value| proto::DataTrackReliability::try_from(value).ok())
        {
            options = options.with_reliability(reliability.into());
        }
        options
    }
}

impl From<&PublishError> for proto::PublishDataTrackErrorCode {
    fn from(err: &PublishError) -> Self {
        match err {
            PublishError::DuplicateName => Self::DuplicateName,
            PublishError::Timeout => Self::Timeout,
            PublishError::Disconnected => Self::Disconnected,
            PublishError::NotAllowed => Self::NotAllowed,
            PublishError::InvalidName => Self::InvalidName,
            PublishError::LimitReached => Self::LimitReached,
            PublishError::Internal(_) => Self::Internal,
        }
    }
}

impl From<PublishError> for proto::PublishDataTrackError {
    fn from(err: PublishError) -> Self {
        proto::PublishDataTrackError {
            code: proto::PublishDataTrackErrorCode::from(&err) as i32,
            message: err.to_string(),
        }
    }
}

impl From<PushFrameErrorReason> for proto::LocalDataTrackTryPushErrorCode {
    fn from(reason: PushFrameErrorReason) -> Self {
        match reason {
            PushFrameErrorReason::TrackUnpublished => Self::TrackUnpublished,
            PushFrameErrorReason::QueueFull => Self::QueueFull,
        }
    }
}

impl From<PushFrameError> for proto::LocalDataTrackTryPushError {
    fn from(err: PushFrameError) -> Self {
        let reason = err.reason();
        proto::LocalDataTrackTryPushError {
            code: proto::LocalDataTrackTryPushErrorCode::from(reason) as i32,
            message: err.to_string(),
        }
    }
}

impl From<&DataTrackSubscribeError> for proto::SubscribeDataTrackErrorCode {
    fn from(err: &DataTrackSubscribeError) -> Self {
        match err {
            DataTrackSubscribeError::Unpublished => Self::Unpublished,
            DataTrackSubscribeError::Timeout => Self::Timeout,
            DataTrackSubscribeError::Disconnected => Self::Disconnected,
            DataTrackSubscribeError::Internal(_) => Self::Internal,
        }
    }
}

impl From<DataTrackSubscribeError> for proto::SubscribeDataTrackError {
    fn from(err: DataTrackSubscribeError) -> Self {
        proto::SubscribeDataTrackError {
            code: proto::SubscribeDataTrackErrorCode::from(&err) as i32,
            message: err.to_string(),
        }
    }
}
