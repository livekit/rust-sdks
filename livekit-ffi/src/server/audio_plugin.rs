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

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use super::FfiServer;
use futures_util::Stream;
use livekit::{
    registered_audio_filter_plugins,
    webrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame},
    AudioFilterAudioStream,
};

pub trait AudioStream: Stream<Item = AudioFrame<'static>> + Send + Sync + Unpin {
    fn close(&mut self);
}

pub enum AudioStreamKind {
    Native(NativeAudioStream),
    Filtered(AudioFilterAudioStream),
}

impl Stream for AudioStreamKind {
    type Item = AudioFrame<'static>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            AudioStreamKind::Native(native_stream) => Pin::new(native_stream).poll_next(cx),
            AudioStreamKind::Filtered(duration_stream) => Pin::new(duration_stream).poll_next(cx),
        }
    }
}

const AUDIO_FILTER_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Initializes all registered audio-filter plugins for a freshly connected room.
///
/// Best-effort: on timeout or failure the room stays connected with the filter
/// disabled. The [`tokio::task::spawn_blocking`] task cannot be cancelled, so on
/// timeout it is detached and finishes in the background.
///
pub async fn initialize_audio_filters(server: &'static FfiServer, url: String, token: String) {
    let started = std::time::Instant::now();
    let init = server.async_runtime.spawn_blocking(move || {
        for filter in registered_audio_filter_plugins().into_iter() {
            filter.on_load(&url, &token).map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    });

    let outcome = match tokio::time::timeout(AUDIO_FILTER_INIT_TIMEOUT, init).await {
        Ok(join_result) => join_result.map_err(|e| e.to_string()).and_then(|r| r),
        Err(_) => Err(format!("timed out after {:?}", AUDIO_FILTER_INIT_TIMEOUT)),
    };

    if let Err(e) = outcome {
        log::debug!("error while initializing audio filter after {:?}: {}", started.elapsed(), e);
        log::error!(
            "audio filter cannot be enabled: ensure you are connecting to LiveKit Cloud and that the filter is properly configured"
        );
    }
}
