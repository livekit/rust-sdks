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

//! Integration test for the "plugin registered after a room connects" fix.
//!
//! `on_load` is normally run for every registered audio-filter plugin when a
//! room connects (a one-time snapshot). A plugin registered *after* connect
//! must still be initialized for already-connected rooms, otherwise its
//! `create` fails because the connection was never authenticated. This test
//! exercises exactly that ordering against a real room.
//!
//! Gated behind `#[ignore]`: it needs a live LiveKit server. Run with:
//! ```text
//! LK_TEST_URL=... LK_TEST_API_KEY=... LK_TEST_API_SECRET=... \
//!   cargo test -p livekit-ffi --lib -- --ignored on_load_runs_for_plugin_registered_after_connect
//! ```
//! Requires a C compiler (`cc`) on PATH to build a minimal test plugin.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use livekit_api::access_token::{AccessToken, VideoGrants};

use crate::{proto, server::requests, FFI_SERVER};

/// A minimal audio-filter plugin. Its `on_load` appends the options JSON it
/// receives (which embeds the room url + token) to the file named by
/// `$LK_TEST_ONLOAD_LOG`, so the test can observe whether — and with which
/// url — `on_load` ran. The other exported symbols are the bare minimum
/// required for `AudioFilterPlugin` to load.
const TEST_PLUGIN_SRC: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

int audio_filter_on_load(const char* options) {
    const char* path = getenv("LK_TEST_ONLOAD_LOG");
    if (path) {
        FILE* f = fopen(path, "a");
        if (f) { fprintf(f, "%s\n", options ? options : ""); fclose(f); }
    }
    return 0;
}
void* audio_filter_create(uint32_t sr, const char* opts, const char* si) { return malloc(1); }
void audio_filter_destroy(const void* p) { free((void*)p); }
void audio_filter_process_int16(const void* p, size_t n, const int16_t* in, int16_t* out) {
    memcpy(out, in, n * sizeof(int16_t));
}
void audio_filter_process_float(const void* p, size_t n, const float* in, float* out) {
    memcpy(out, in, n * sizeof(float));
}
void audio_filter_update_stream_info(const void* p, const char* si) {}
"#;

fn build_test_plugin(dir: &Path) -> PathBuf {
    let src = dir.join("test_filter.c");
    std::fs::write(&src, TEST_PLUGIN_SRC).unwrap();

    let ext = if cfg!(target_os = "macos") { "dylib" } else { "so" };
    let out = dir.join(format!("libtestfilter.{ext}"));

    let status = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&out)
        .arg(&src)
        .status()
        .expect("failed to invoke `cc` to build the test plugin");
    assert!(status.success(), "cc failed to build the test plugin");
    out
}

#[test]
#[ignore = "requires a live LiveKit server (LK_TEST_URL / LK_TEST_API_KEY / LK_TEST_API_SECRET) and a C compiler"]
fn on_load_runs_for_plugin_registered_after_connect() {
    let url = std::env::var("LK_TEST_URL").expect("LK_TEST_URL isn't set");
    let api_key = std::env::var("LK_TEST_API_KEY").expect("LK_TEST_API_KEY isn't set");
    let api_secret = std::env::var("LK_TEST_API_SECRET").expect("LK_TEST_API_SECRET isn't set");

    // Per-run temp dir for the plugin dylib and the on_load log.
    let tmp = std::env::temp_dir().join(format!("lk_af_late_reg_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let log_path = tmp.join("on_load.log");
    std::env::set_var("LK_TEST_ONLOAD_LOG", &log_path);
    let plugin_path = build_test_plugin(&tmp);

    let token = AccessToken::with_api_key(&api_key, &api_secret)
        .with_grants(VideoGrants {
            room: "livekit-ffi-af-late-reg".to_string(),
            ..Default::default()
        })
        .with_identity("af_late_reg_test")
        .to_jwt()
        .unwrap();

    // Connect a room. `connect` returns immediately and drives the actual
    // connection on the server runtime; the room is stored in the handle map
    // once connected, so poll `list_rooms()` rather than the event callback.
    let _ = crate::server::room::FfiRoom::connect(
        &FFI_SERVER,
        proto::ConnectRequest { url: url.clone(), token, ..Default::default() },
    );

    let room = wait_for(Duration::from_secs(15), || FFI_SERVER.list_rooms().into_iter().next())
        .expect("room did not connect within 15s");
    let room_handle = room.inner.handle_id;

    // Sanity: the plugin must not be registered yet, and thus no on_load.
    assert!(!log_path.exists() || read_log(&log_path).is_empty());

    // Register the plugin *after* the room is connected. Before the fix, its
    // on_load would never run for this room; the fix re-runs on_load for
    // already-connected rooms on registration.
    let res = requests::handle_request(
        &FFI_SERVER,
        proto::FfiRequest {
            message: Some(proto::ffi_request::Message::LoadAudioFilterPlugin(
                proto::LoadAudioFilterPluginRequest {
                    plugin_path: plugin_path.to_string_lossy().into_owned(),
                    module_id: "test-late-reg".to_string(),
                    dependencies: vec![],
                },
            )),
        },
    )
    .expect("handle_request failed");
    if let Some(proto::ffi_response::Message::LoadAudioFilterPlugin(resp)) = res.message {
        assert!(resp.error.is_none(), "plugin failed to load: {:?}", resp.error);
    } else {
        panic!("unexpected response to LoadAudioFilterPlugin");
    }

    // The re-run happens on a spawned blocking task; poll for the log entry.
    let logged = wait_for(Duration::from_secs(10), || {
        let contents = read_log(&log_path);
        (!contents.is_empty()).then_some(contents)
    })
    .expect("on_load did not run for the already-connected room after registration");

    // on_load received the room's connect url (this is the key that `create`
    // later looks up — it must match).
    assert!(
        logged.contains(&url),
        "on_load ran but not for the connected room's url; got: {logged}"
    );

    // Cleanup: disconnect the room.
    let _ = requests::handle_request(
        &FFI_SERVER,
        proto::FfiRequest {
            message: Some(proto::ffi_request::Message::Disconnect(proto::DisconnectRequest {
                room_handle,
                ..Default::default()
            })),
        },
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

fn read_log(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
