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

use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use crate::encoded_video_source::native::NativeEncodedVideoSource;
use crate::encoded_video_source::{EncodedFrameInfo, KeyFrameRequestCallback, VideoCodecType};
use crate::imp::video_source as vs_imp;

#[derive(Debug, Clone)]
pub struct VideoResolution {
    pub width: u32,
    pub height: u32,
}

impl Default for VideoResolution {
    // Default to 720p
    fn default() -> Self {
        VideoResolution { width: 1280, height: 720 }
    }
}

/// Describes a single simulcast layer for encoded publishing.
#[derive(Debug, Clone)]
pub struct EncodedSimulcastLayer {
    /// The encoded video source for this layer.
    pub source: NativeEncodedVideoSource,
    /// Frame width for this layer.
    pub width: u32,
    /// Frame height for this layer.
    pub height: u32,
    /// Maximum bitrate in bps for this layer.
    pub max_bitrate: u64,
    /// Maximum framerate for this layer.
    pub max_framerate: f64,
}

/// Bundles multiple `NativeEncodedVideoSource` instances (one per simulcast
/// layer) into a single publishable unit.  Layers are ordered from lowest
/// quality (index 0, RID "q") to highest quality (last index, RID "f").
#[derive(Clone)]
pub struct SimulcastEncodedVideoSource {
    layers: Vec<EncodedSimulcastLayer>,
}

impl Debug for SimulcastEncodedVideoSource {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("SimulcastEncodedVideoSource")
            .field("num_layers", &self.layers.len())
            .field("primary_resolution", &self.video_resolution())
            .field("codec", &self.codec_type())
            .finish()
    }
}

impl SimulcastEncodedVideoSource {
    /// Create a new simulcast encoded video source from a list of layers.
    ///
    /// Layers must be ordered from lowest quality to highest quality
    /// (e.g., `[q=320x180, h=640x360, f=1280x720]`).
    /// All layers must use the same codec.
    pub fn new(layers: Vec<EncodedSimulcastLayer>) -> Self {
        assert!(!layers.is_empty(), "at least one layer is required");
        assert!(
            layers.len() <= 3,
            "at most 3 simulcast layers are supported (q, h, f)"
        );
        // Verify all layers use the same codec
        let codec = layers[0].source.codec_type();
        for layer in &layers[1..] {
            assert_eq!(
                layer.source.codec_type(),
                codec,
                "all simulcast layers must use the same codec"
            );
        }
        Self { layers }
    }

    /// Returns the layers (ordered low to high quality).
    pub fn layers(&self) -> &[EncodedSimulcastLayer] {
        &self.layers
    }

    /// Returns the primary (highest quality) source — used to create the
    /// underlying WebRTC video track.
    pub fn primary(&self) -> &NativeEncodedVideoSource {
        &self.layers.last().unwrap().source
    }

    /// Returns the video resolution of the primary (highest quality) layer.
    pub fn video_resolution(&self) -> VideoResolution {
        let layer = self.layers.last().unwrap();
        VideoResolution {
            width: layer.width,
            height: layer.height,
        }
    }

    /// Returns the codec type (same for all layers).
    pub fn codec_type(&self) -> VideoCodecType {
        self.layers[0].source.codec_type()
    }

    /// Capture a frame for a specific simulcast layer.
    pub fn capture_frame(&self, layer_index: usize, info: &EncodedFrameInfo) -> bool {
        self.layers[layer_index].source.capture_frame(info)
    }

    /// Set the keyframe request callback for all layers.
    pub fn set_keyframe_request_callback(
        &mut self,
        callback: Arc<dyn KeyFrameRequestCallback>,
    ) {
        for layer in &mut self.layers {
            layer.source.set_keyframe_request_callback(callback.clone());
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RtcVideoSource {
    // TODO(theomonnom): Web video sources (eq. to tracks on browsers?)
    #[cfg(not(target_arch = "wasm32"))]
    Native(native::NativeVideoSource),
    #[cfg(not(target_arch = "wasm32"))]
    Encoded(NativeEncodedVideoSource),
    /// Multiple pre-encoded video sources bundled for simulcast publishing.
    #[cfg(not(target_arch = "wasm32"))]
    SimulcastEncoded(SimulcastEncodedVideoSource),
}

impl RtcVideoSource {
    pub fn video_resolution(&self) -> VideoResolution {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Self::Native(s) => s.video_resolution(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Encoded(s) => s.video_resolution(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::SimulcastEncoded(s) => s.video_resolution(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::fmt::{Debug, Formatter};

    use super::*;
    use crate::video_frame::{VideoBuffer, VideoFrame};

    #[derive(Clone)]
    pub struct NativeVideoSource {
        pub(crate) handle: vs_imp::NativeVideoSource,
    }

    impl Debug for NativeVideoSource {
        fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
            f.debug_struct("NativeVideoSource").finish()
        }
    }

    impl Default for NativeVideoSource {
        fn default() -> Self {
            Self::new(VideoResolution::default())
        }
    }

    impl NativeVideoSource {
        pub fn new(resolution: VideoResolution) -> Self {
            Self { handle: vs_imp::NativeVideoSource::new(resolution) }
        }

        pub fn capture_frame<T: AsRef<dyn VideoBuffer>>(&self, frame: &VideoFrame<T>) {
            self.handle.capture_frame(frame)
        }

        pub fn video_resolution(&self) -> VideoResolution {
            self.handle.video_resolution()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod web {}
