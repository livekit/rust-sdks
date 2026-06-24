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

//! Capture helpers for publishing decoded, DMA-BUF, and encoded video with LiveKit.

pub mod device;
pub mod dmabuf;
pub mod encoded;
mod error;
pub mod metadata;
pub mod platform;
pub mod source;
pub mod sources;
pub mod track;

#[allow(deprecated)]
pub use device::CapturePixelFormat;
pub use device::{
    CaptureBackend, CaptureDeviceInfo, CaptureDeviceQueryError, CaptureDeviceSelector,
    CaptureFormat, CaptureFormatRequest, CaptureFrameFormat, CapturePath, CaptureResolution,
};
pub use dmabuf::{DmaBufFrame, DmaBufPixelFormat, DmaBufPlane};
pub use encoded::{
    ingress::{EncodedAccessUnitSource, EncodedIngress, EncodedIngressError},
    CodecSpecific, EncodedAccessUnit, EncodedFragment, EncodedFrameType, EncodedLayerInfo,
    EncodedPayload, EncodedVideoCodec, EncodedWireFormat, H264PacketizationMode,
    OwnedEncodedAccessUnit,
};
pub use error::CaptureError;
pub use metadata::FrameMetadata;
pub use source::{
    CaptureFrame, CaptureFrameSource, CaptureMetadataOptions, CaptureSourceError,
    CaptureSourceOptions, CaptureTimestampSource, EncodedCaptureFrameSource,
    EncodedFrameSourceError, RawVideoFrame, VideoCaptureSource,
};
pub use track::VideoCaptureTrack;
