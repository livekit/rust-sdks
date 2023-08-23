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
use crate::server::room::FfiRoom;
use livekit::options::{AudioEncoding, TrackPublishOptions, VideoEncoding};
use livekit::prelude::*;

impl From<ConnectionQuality> for proto::ConnectionQuality {
    fn from(value: ConnectionQuality) -> Self {
        match value {
            ConnectionQuality::Excellent => Self::QualityExcellent,
            ConnectionQuality::Good => Self::QualityGood,
            ConnectionQuality::Poor => Self::QualityPoor,
        }
    }
}

impl From<ConnectionState> for proto::ConnectionState {
    fn from(value: ConnectionState) -> Self {
        match value {
            ConnectionState::Connected => Self::ConnConnected,
            ConnectionState::Reconnecting => Self::ConnReconnecting,
            ConnectionState::Disconnected => Self::ConnDisconnected,
        }
    }
}

impl From<proto::RoomOptions> for RoomOptions {
    fn from(value: proto::RoomOptions) -> Self {
        Self {
            adaptive_stream: value.adaptive_stream,
            auto_subscribe: value.auto_subscribe,
            dynacast: value.dynacast,
            e2ee_options: None, //TODO: convert
        }
    }
}

impl From<proto::DataPacketKind> for DataPacketKind {
    fn from(value: proto::DataPacketKind) -> Self {
        match value {
            proto::DataPacketKind::KindReliable => Self::Reliable,
            proto::DataPacketKind::KindLossy => Self::Lossy,
        }
    }
}

impl From<DataPacketKind> for proto::DataPacketKind {
    fn from(value: DataPacketKind) -> Self {
        match value {
            DataPacketKind::Reliable => Self::KindReliable,
            DataPacketKind::Lossy => Self::KindLossy,
        }
    }
}

impl From<proto::TrackPublishOptions> for TrackPublishOptions {
    fn from(opts: proto::TrackPublishOptions) -> Self {
        Self {
            video_codec: opts.video_codec().into(),
            source: opts.source().into(),
            video_encoding: opts.video_encoding.map(Into::into),
            audio_encoding: opts.audio_encoding.map(Into::into),
            dtx: opts.dtx,
            red: opts.red,
            simulcast: opts.simulcast,
        }
    }
}

impl From<proto::VideoEncoding> for VideoEncoding {
    fn from(opts: proto::VideoEncoding) -> Self {
        Self {
            max_bitrate: opts.max_bitrate,
            max_framerate: opts.max_framerate,
        }
    }
}

impl From<proto::AudioEncoding> for AudioEncoding {
    fn from(opts: proto::AudioEncoding) -> Self {
        Self {
            max_bitrate: opts.max_bitrate,
        }
    }
}

impl proto::RoomInfo {
    pub fn from(handle: proto::FfiOwnedHandle, ffi_room: &FfiRoom) -> Self {
        let room = &ffi_room.inner.room;
        Self {
            handle: Some(handle),
            sid: room.sid().into(),
            name: room.name(),
            metadata: room.metadata(),
        }
    }
}
