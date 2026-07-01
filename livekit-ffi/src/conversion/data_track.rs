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
        DataTrackFrame, DataTrackFrameEncoding, DataTrackInfo, DataTrackOptions,
        DataTrackSchemaEncoding, DataTrackSchemaId, DataTrackSubscribeError, PublishError,
        PushFrameError, PushFrameErrorReason, RemoteDataTrackPipelineOptions,
    },
    prelude::DataTrackSubscribeOptions,
};

impl From<proto::DataTrackOptions> for DataTrackOptions {
    fn from(options: proto::DataTrackOptions) -> Self {
        let frame_encoding = options.frame_encoding.map(Into::into);
        let mut result = Self::new(options.name);
        if let Some(schema) = options.schema {
            result = result.with_schema(schema.into());
        }
        if let Some(frame_encoding) = frame_encoding {
            result = result.with_frame_encoding(frame_encoding);
        }
        result
    }
}

impl From<DataTrackInfo> for proto::DataTrackInfo {
    fn from(info: DataTrackInfo) -> Self {
        Self {
            name: info.name().to_string(),
            sid: info.sid().to_string(),
            uses_e2ee: info.uses_e2ee(),
            schema: info.schema().cloned().map(Into::into),
            frame_encoding: info.frame_encoding().cloned().map(Into::into),
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
        options
    }
}

impl From<proto::DataTrackSchemaEncoding> for DataTrackSchemaEncoding {
    fn from(encoding: proto::DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Encoding, WellKnownSchemaEncoding as WellKnown};
        match encoding.encoding {
            Some(Encoding::WellKnown(value)) => match WellKnown::try_from(value) {
                Ok(WellKnown::Protobuf) => Self::Protobuf,
                Ok(WellKnown::Flatbuffer) => Self::Flatbuffer,
                Ok(WellKnown::Ros1Msg) => Self::Ros1Msg,
                Ok(WellKnown::Ros2Msg) => Self::Ros2Msg,
                Ok(WellKnown::Ros2Idl) => Self::Ros2Idl,
                Ok(WellKnown::OmgIdl) => Self::OmgIdl,
                Ok(WellKnown::JsonSchema) => Self::JsonSchema,
                Ok(WellKnown::Unspecified) | Err(_) => Self::Other,
            },
            Some(Encoding::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackSchemaEncoding> for proto::DataTrackSchemaEncoding {
    fn from(encoding: DataTrackSchemaEncoding) -> Self {
        use proto::data_track_schema_encoding::{Encoding, WellKnownSchemaEncoding as WellKnown};
        let encoding = match encoding {
            DataTrackSchemaEncoding::Protobuf => Encoding::WellKnown(WellKnown::Protobuf as i32),
            DataTrackSchemaEncoding::Flatbuffer => Encoding::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackSchemaEncoding::Ros1Msg => Encoding::WellKnown(WellKnown::Ros1Msg as i32),
            DataTrackSchemaEncoding::Ros2Msg => Encoding::WellKnown(WellKnown::Ros2Msg as i32),
            DataTrackSchemaEncoding::Ros2Idl => Encoding::WellKnown(WellKnown::Ros2Idl as i32),
            DataTrackSchemaEncoding::OmgIdl => Encoding::WellKnown(WellKnown::OmgIdl as i32),
            DataTrackSchemaEncoding::JsonSchema => Encoding::WellKnown(WellKnown::JsonSchema as i32),
            DataTrackSchemaEncoding::Custom(name) => Encoding::Custom(name),
            DataTrackSchemaEncoding::Other => Encoding::WellKnown(WellKnown::Unspecified as i32),
            // `DataTrackSchemaEncoding` is `#[non_exhaustive]`; map any future
            // variant to the unspecified encoding.
            _ => Encoding::WellKnown(WellKnown::Unspecified as i32),
        };
        Self { encoding: Some(encoding) }
    }
}

impl From<proto::DataTrackFrameEncoding> for DataTrackFrameEncoding {
    fn from(encoding: proto::DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Encoding, WellKnownFrameEncoding as WellKnown};
        match encoding.encoding {
            Some(Encoding::WellKnown(value)) => match WellKnown::try_from(value) {
                Ok(WellKnown::Ros1) => Self::Ros1,
                Ok(WellKnown::Cdr) => Self::Cdr,
                Ok(WellKnown::Protobuf) => Self::Protobuf,
                Ok(WellKnown::Flatbuffer) => Self::Flatbuffer,
                Ok(WellKnown::Cbor) => Self::Cbor,
                Ok(WellKnown::Msgpack) => Self::Msgpack,
                Ok(WellKnown::Json) => Self::Json,
                Ok(WellKnown::Unspecified) | Err(_) => Self::Other,
            },
            Some(Encoding::Custom(name)) => Self::Custom(name),
            None => Self::Other,
        }
    }
}

impl From<DataTrackFrameEncoding> for proto::DataTrackFrameEncoding {
    fn from(encoding: DataTrackFrameEncoding) -> Self {
        use proto::data_track_frame_encoding::{Encoding, WellKnownFrameEncoding as WellKnown};
        let encoding = match encoding {
            DataTrackFrameEncoding::Ros1 => Encoding::WellKnown(WellKnown::Ros1 as i32),
            DataTrackFrameEncoding::Cdr => Encoding::WellKnown(WellKnown::Cdr as i32),
            DataTrackFrameEncoding::Protobuf => Encoding::WellKnown(WellKnown::Protobuf as i32),
            DataTrackFrameEncoding::Flatbuffer => Encoding::WellKnown(WellKnown::Flatbuffer as i32),
            DataTrackFrameEncoding::Cbor => Encoding::WellKnown(WellKnown::Cbor as i32),
            DataTrackFrameEncoding::Msgpack => Encoding::WellKnown(WellKnown::Msgpack as i32),
            DataTrackFrameEncoding::Json => Encoding::WellKnown(WellKnown::Json as i32),
            DataTrackFrameEncoding::Custom(name) => Encoding::Custom(name),
            DataTrackFrameEncoding::Other => Encoding::WellKnown(WellKnown::Unspecified as i32),
            // `DataTrackFrameEncoding` is `#[non_exhaustive]`; map any future
            // variant to the unspecified encoding.
            _ => Encoding::WellKnown(WellKnown::Unspecified as i32),
        };
        Self { encoding: Some(encoding) }
    }
}

impl From<proto::DataTrackSchemaId> for DataTrackSchemaId {
    fn from(msg: proto::DataTrackSchemaId) -> Self {
        let encoding = msg.encoding.map(Into::into).unwrap_or(DataTrackSchemaEncoding::Other);
        DataTrackSchemaId::new(msg.name, encoding)
    }
}

impl From<DataTrackSchemaId> for proto::DataTrackSchemaId {
    fn from(id: DataTrackSchemaId) -> Self {
        Self {
            name: id.name().to_string(),
            encoding: Some(id.encoding().clone().into()),
        }
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
