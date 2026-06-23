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

use thiserror::Error;

use crate::encoded::{EncodedVideoCodec, EncodedWireFormat};

/// Error returned by capture helpers.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CaptureError {
    /// Encoded payload is empty.
    #[error("encoded payload is empty")]
    EmptyPayload,
    /// H.265 NAL unit is too short to contain its header.
    #[error("H.265 NAL unit is too short")]
    H265NalTooShort,
    /// DMA-BUF frame did not include any planes.
    #[error("DMA-BUF frame did not include any planes")]
    MissingDmaBufPlane,
    /// Codec is represented by the API but not yet supported by native passthrough.
    #[error("encoded passthrough does not support {0:?} yet")]
    UnsupportedCodec(EncodedVideoCodec),
    /// Encoded payload or transport data is malformed.
    #[error("invalid encoded data: {0}")]
    InvalidEncodedData(&'static str),
    /// Wire format is represented by the API but not supported by this source.
    #[error("encoded wire format is not supported by this source: {0:?}")]
    UnsupportedWireFormat(EncodedWireFormat),
    /// Capture backend is not available on this platform.
    #[error("{0} is not supported on this platform")]
    UnsupportedPlatform(&'static str),
    /// The underlying source rejected the frame.
    #[error("capture source rejected the frame")]
    CaptureFailed,
}
