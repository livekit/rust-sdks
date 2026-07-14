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

use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(libargus_available)");
    println!("cargo:rerun-if-env-changed=JETSON_MULTIMEDIA_API_DIR");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_os != "linux" || target_arch != "aarch64" {
        return;
    }

    let mmapi_root = std::env::var_os("JETSON_MULTIMEDIA_API_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/src/jetson_multimedia_api"));
    let argus_include = mmapi_root.join("argus/include");
    let mmapi_include = mmapi_root.join("include");

    if !argus_include.exists() || !mmapi_include.exists() {
        println!(
            "cargo:warning=Argus headers not found under {}; skipping libargus capture shim",
            mmapi_root.display()
        );
        return;
    }

    println!("cargo:rerun-if-changed=src/lk_argus.cpp");

    cc::Build::new()
        .cpp(true)
        .file("src/lk_argus.cpp")
        .include(&argus_include)
        .include(&mmapi_include)
        .flag("-std=c++14")
        .flag("-Wno-deprecated-declarations")
        .compile("lk_argus");

    println!("cargo:rustc-cfg=libargus_available");
    println!("cargo:rustc-link-lib=dylib=nvargus_socketclient");
    println!("cargo:rustc-link-lib=dylib=nvbufsurface");

    let tegra_lib_dir = PathBuf::from("/usr/lib/aarch64-linux-gnu/tegra");
    if tegra_lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", tegra_lib_dir.display());
    }
    println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");

    // Communicate availability to dependent crates via `DEP_LK_ARGUS_AVAILABLE`.
    println!("cargo:available=1");
}
