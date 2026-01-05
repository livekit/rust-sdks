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

use std::env;

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let is_desktop = target_os == "linux" || target_os == "windows" || target_os == "macos";

    println!("cargo:rerun-if-env-changed=LK_DEBUG_WEBRTC");
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");

    //let use_debug_lib = webrtc_sys_build::rtc_debug_enabled();
    let use_dylib = !cfg!(feature = "static");
    //let use_custom_rtc = webrtc_sys_build::is_using_custom_webrtc();

    let webrtc_dir = webrtc_sys_build::webrtc_dir();
    //let webrtc_include = webrtc_dir.join("include");
    //let webrtc_lib = webrtc_dir.join("lib");

    if use_dylib {
        if let Err(e) = webrtc_sys_build::copy_dylib_to_target(&webrtc_dir) {
            println!("cargo:warning=failed to copy livekit_rtc dylib to target: {}", e);
        }
    }

    if use_dylib {
        webrtc_sys_build::link_shared_library(&webrtc_dir);
    } else {
        webrtc_sys_build::link_static_library(&webrtc_dir);
    }
}
