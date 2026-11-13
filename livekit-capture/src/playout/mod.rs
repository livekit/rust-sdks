// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.

//! Audio playout sinks.

#[cfg(feature = "source-gstreamer")]
mod gstreamer;

#[cfg(feature = "source-gstreamer")]
pub use gstreamer::{
    GStreamerAudioPlayout, GStreamerAudioPlayoutConfig, GStreamerAudioPlayoutControl,
    GStreamerAudioPlayoutError, GStreamerAudioPlayoutStats,
};
