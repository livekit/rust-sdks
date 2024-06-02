// Copyright 2024 LiveKit, Inc.
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

use std::env;

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    let use_debug_lib = libwebrtc_build::rtc_debug_enabled();
    let use_dylib = !cfg!(feature = "static");
    let use_custom_rtc = libwebrtc_build::is_using_custom_webrtc();

    if !use_custom_rtc {
        libwebrtc_build::download_webrtc_if_needed(use_debug_lib, use_dylib).unwrap();
    }

    let mut rtc_path = libwebrtc_build::rtc_directory();
    if use_custom_rtc {
        rtc_path = libwebrtc_build::custom_rtc_directory().unwrap();
    }

    if use_dylib {
        if let Err(e) = libwebrtc_build::copy_dylib_to_target(&rtc_path) {
            println!("cargo:warning=failed to copy livekit_rtc dylib to target: {}", e);
        }

        libwebrtc_build::link_shared_library(&rtc_path);
    } else {
        libwebrtc_build::link_static_library(&rtc_path);
    }
}
