// Copyright 2023 LiveKit, Inc.
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

    let webrtc_dir = webrtc_sys_build::webrtc_dir();
    if !webrtc_dir.exists() {
        webrtc_sys_build::download_webrtc().unwrap();
    }

    // Skip license copying altogether
    // (previously tried to copy LICENSE.md -> WEBRTC_LICENSE.md)

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    match target_os.as_str() {
        "windows" => {}
        "linux" => {
            println!("cargo:rustc-link-lib=static=webrtc");
        }
        "android" => {
            webrtc_sys_build::configure_jni_symbols().unwrap();
        }
        "macos" | "ios" => {
            println!("cargo:rustc-link-arg=-ObjC");
        }
        _ => {
            println!("cargo:warning=Unsupported target OS: {}", target_os);
        }
    }
}
