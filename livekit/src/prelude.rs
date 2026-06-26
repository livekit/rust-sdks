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

pub use livekit_protocol::{AudioTrackFeature, PacketTrailerFeature};

pub use crate::{
    data_track::{
        DataTrackFrame, DataTrackInfo, DataTrackOptions, DataTrackSid, DataTrackStream,
        DataTrackSubscribeError, DataTrackSubscribeOptions, LocalDataTrack, PublishError,
        PushFrameError, PushFrameErrorReason, RemoteDataTrack, RemoteDataTrackPipelineOptions,
    },
    id::*,
    participant::{
        ClientCapability, ConnectionQuality, DisconnectReason, LocalParticipant, Participant,
        PerformRpcData, RemoteParticipant, RpcError, RpcErrorCode, RpcInvocationData,
    },
    publication::{LocalTrackPublication, RemoteTrackPublication, TrackPublication},
    track::{
        AudioTrack, LocalAudioTrack, LocalTrack, LocalVideoTrack, PublishTimingEvent,
        PublishTimingEventStream, PublishTimingStage, PublishingLayer, PublishingLayerQuality,
        RemoteAudioTrack, RemoteTrack, RemoteVideoTrack, StreamState, SubscribeTimingEvent,
        SubscribeTimingEventStream, SubscribeTimingStage, Track, TrackDimension, TrackKind,
        TrackSource, VideoTrack,
    },
    ConnectionState, DataPacket, DataPacketKind, Room, RoomError, RoomEvent, RoomOptions,
    RoomResult, RoomSdkOptions, SipDTMF, Transcription, TranscriptionSegment,
};

// Platform audio device management (native platforms only)
#[cfg(not(target_arch = "wasm32"))]
pub use crate::platform_audio::{
    AudioError, AudioProcessingOptions, AudioProcessingType, AudioResult, PlatformAudio,
    PlayoutDeviceId, PlayoutDeviceInfo, RecordingDeviceId, RecordingDeviceInfo, RtcAudioSource,
};
