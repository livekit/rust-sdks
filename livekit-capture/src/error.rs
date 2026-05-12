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

use thiserror::Error;

/// Errors that can occur during capture.
#[derive(Debug, Error)]
pub enum CaptureError {
    /// The requested device could not be opened or doesn't exist.
    #[error("device unavailable: {0}")]
    DeviceUnavailable(String),

    /// The negotiated/requested format is not supported.
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    /// A backend-specific failure occurred while reading a frame.
    #[error("frame read failed: {0}")]
    FrameRead(String),

    /// Conversion to I420 failed.
    #[error("conversion failed: {0}")]
    Conversion(String),

    /// Catch-all for backend errors that don't have a dedicated variant.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
