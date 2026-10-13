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

//! Encoded video capture from a GStreamer pipeline.
//!
//! [`GStreamerVideoSource`] owns a pipeline that ends in an appsink and
//! yields the pipeline's encoded output as access units.
//!
//! With `source-gstreamer`, GStreamer is linked into the binary and must be
//! installed to build and launch it. With `source-gstreamer-dynamic` alone,
//! the source is loaded at runtime from the `livekit-capture-gstreamer` shim
//! library, so GStreamer is only needed once a source is created. When both
//! features are enabled, the linked source is used.

use crate::{encoded::EncodedVideoCodec, primitive::VideoResolution};

#[doc(hidden)]
pub mod abi;

#[cfg(feature = "source-gstreamer")]
mod linked;
#[cfg(feature = "source-gstreamer")]
pub use linked::{GStreamerPipelineError, GStreamerVideoSource, GStreamerVideoSourceError};

#[cfg(all(feature = "source-gstreamer-dynamic", not(feature = "source-gstreamer")))]
mod loader;
#[cfg(all(feature = "source-gstreamer-dynamic", not(feature = "source-gstreamer")))]
pub use loader::{GStreamerVideoSource, GStreamerVideoSourceError};

/// Configuration for a GStreamer encoded video source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerVideoSourceConfig {
    /// GStreamer launch description for the encoded producer pipeline.
    ///
    /// The pipeline must contain `appsink name=lk_appsink`, or leave exactly
    /// one encoded video source pad unlinked. The source then attaches an
    /// appsink to that pad.
    pub pipeline: String,

    /// Codec expected from the pipeline. When omitted, the codec is
    /// inferred from the pipeline caps.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub codec: Option<EncodedVideoCodec>,

    /// Encoded frame resolution.
    ///
    /// When omitted, the resolution is discovered from the first sample, so
    /// construction waits for the pipeline to produce data. When set,
    /// construction returns without waiting, and the first sample is
    /// verified against this value.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub resolution: Option<VideoResolution>,

    /// Forwards WebRTC rate-control targets to an encoder element's bitrate
    /// property. Without this, the pipeline encodes at a fixed bitrate.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub rate_control: Option<GStreamerRateControlConfig>,
}

/// Binding from WebRTC rate-control targets to a GStreamer encoder property.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerRateControlConfig {
    /// Name of the encoder element in the pipeline (for example
    /// `lk_encoder`).
    pub element: String,

    /// Bitrate property to set on the element (for example `bitrate` for
    /// x264enc, or `target-bitrate` for vp8enc/vp9enc).
    pub property: String,

    /// Unit the property expects.
    pub unit: GStreamerBitrateUnit,
}

/// Bitrate unit used by a GStreamer encoder property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum GStreamerBitrateUnit {
    /// The encoder property expects bits per second.
    #[cfg_attr(feature = "serde", serde(rename = "bps"))]
    BitsPerSecond,
    /// The encoder property expects kilobits per second.
    #[cfg_attr(feature = "serde", serde(rename = "kbps"))]
    KilobitsPerSecond,
}
