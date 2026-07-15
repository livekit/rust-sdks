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
    sync::{Arc, LazyLock},
    task::{Context, Poll},
};

use dashmap::DashMap;
use futures_util::Stream;
use livekit::{
    webrtc::{audio_stream::native::NativeAudioStream, prelude::AudioFrame},
    AudioFilterAudioStream, AudioFilterPlugin, PluginError,
};
use tokio::sync::watch;

static REGISTERED_PLUGINS: LazyLock<DashMap<String, Arc<FfiAudioFilterPlugin>>> =
    LazyLock::new(DashMap::new);

/// Registers an audio filter plugin under `module_id`, both here (for
/// initialization tracking) and with the SDK (which forwards token refreshes).
pub(crate) fn register_audio_filter_plugin(module_id: String, plugin: Arc<AudioFilterPlugin>) {
    let ffi_plugin = Arc::new(FfiAudioFilterPlugin::new(plugin.clone()));
    REGISTERED_PLUGINS.insert(module_id.clone(), ffi_plugin);
    livekit::register_audio_filter_plugin(module_id, plugin);
}

/// Returns the plugin registered under `module_id`, if any.
pub(crate) fn registered_audio_filter_plugin(module_id: &str) -> Option<Arc<FfiAudioFilterPlugin>> {
    REGISTERED_PLUGINS.get(module_id).map(|entry| entry.value().clone())
}

/// Background task that initializes registered audio filter plugins for a
/// newly connected room, keeping their potentially blocking `on_load` off
/// the connect path. Failures are non-fatal: the plugin is left disabled.
pub(crate) struct AudioFilterInitTask {
    pub url: String,
    pub token: String,
}

impl AudioFilterInitTask {
    /// Initializes each registered plugin in sequence.
    pub async fn run(self) {
        let plugins: Vec<_> = REGISTERED_PLUGINS.iter().map(|entry| entry.value().clone()).collect();
        for plugin in plugins {
            plugin.initialize(&self.url, &self.token).await;
        }
    }
}

/// Actionable hint appended to `on_load` failure logs.
fn on_load_error_hint(e: &PluginError) -> &'static str {
    match e {
        PluginError::OnLoad(_) => " — ensure you are connecting to LiveKit Cloud and that the filter is configured correctly",
        PluginError::Library(_) => " — the filter dylib could not be loaded",
        PluginError::NotImplemented(_) => " — the filter dylib is missing a required entry point",
    }
}

/// Tracks whether a plugin's `on_load` entry point has run.
#[derive(Debug, Clone)]
enum InitState {
    Pending,
    Initializing,
    Initialized,
    Failed(String),
}

/// A registered audio filter plugin with initialization tracking: `on_load`
/// runs in the background (see [`AudioFilterInitTask`]) while streams wait
/// for it via [`Self::wait_until_initialized`].
pub(crate) struct FfiAudioFilterPlugin {
    pub(crate) plugin: Arc<AudioFilterPlugin>,
    init_state: watch::Sender<InitState>,
}

impl FfiAudioFilterPlugin {
    fn new(plugin: Arc<AudioFilterPlugin>) -> Self {
        Self { plugin, init_state: watch::channel(InitState::Pending).0 }
    }

    /// Runs the plugin's opaque, potentially blocking `on_load` on the
    /// blocking pool, recording and logging the outcome.
    async fn initialize(&self, url: &str, token: &str) {
        self.init_state.send_replace(InitState::Initializing);

        let plugin = self.plugin.clone();
        let (url, token) = (url.to_owned(), token.to_owned());
        let result = tokio::task::spawn_blocking(move || plugin.on_load(&url, &token)).await;

        // Flatten panic (join error) and plugin error into a single reason.
        let result = result
            .map_err(|join_err| format!("on_load panicked: {join_err}"))
            .and_then(|result| result.map_err(|e| format!("{e}{}", on_load_error_hint(&e))));

        let state = match result {
            Ok(()) => InitState::Initialized,
            Err(reason) => {
                log::error!("audio filter disabled, continuing without it: {reason}");
                InitState::Failed(reason)
            }
        };
        self.init_state.send_replace(state);
    }

    /// Waits until `on_load` has completed, returning the failure reason on
    /// error.
    ///
    /// Never resolves if initialization is never attempted; callers should
    /// apply a timeout.
    pub(crate) async fn wait_until_initialized(&self) -> Result<(), String> {
        let mut rx = self.init_state.subscribe();
        loop {
            match &*rx.borrow_and_update() {
                InitState::Initialized => return Ok(()),
                InitState::Failed(reason) => return Err(reason.clone()),
                InitState::Pending | InitState::Initializing => {}
            }
            if rx.changed().await.is_err() {
                // Unreachable: the sender lives in `self`.
                return Err("plugin was dropped".to_owned());
            }
        }
    }
}

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
