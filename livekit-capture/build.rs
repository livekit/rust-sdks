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

fn main() {
    println!("cargo:rustc-check-cfg=cfg(livekit_capture_argus)");

    // `libargus-sys` sets `DEP_LK_ARGUS_AVAILABLE` (via its `links` metadata)
    // when the native Argus shim was compiled and linked for this target.
    if std::env::var_os("DEP_LK_ARGUS_AVAILABLE").is_some() {
        println!("cargo:rustc-cfg=livekit_capture_argus");
    }
}
