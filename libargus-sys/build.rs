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

/// Locates the directory containing the Tegra userspace libraries the shim
/// links against, requiring the libraries themselves to be present so that
/// headers alone (e.g. in a partially-provisioned container or on a
/// non-Jetson aarch64 board) never produce a build that fails to link.
fn find_tegra_lib_dir() -> Option<PathBuf> {
    const REQUIRED_LIBS: [&str; 2] = ["libnvargus_socketclient.so", "libnvbufsurface.so"];

    let candidates: Vec<PathBuf> = match std::env::var_os("JETSON_TEGRA_LIB_DIR") {
        Some(dir) => vec![PathBuf::from(dir)],
        None => vec![
            // JetPack 5+ (L4T r35+)
            PathBuf::from("/usr/lib/aarch64-linux-gnu/tegra"),
            // Symlink directory shipped on some releases
            PathBuf::from("/usr/lib/aarch64-linux-gnu/nvidia"),
        ],
    };

    candidates
        .into_iter()
        .find(|dir| REQUIRED_LIBS.iter().all(|lib| dir.join(lib).exists()))
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(libargus_available)");
    println!("cargo:rerun-if-env-changed=JETSON_MULTIMEDIA_API_DIR");
    println!("cargo:rerun-if-env-changed=JETSON_TEGRA_LIB_DIR");

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
            "cargo:warning=Argus headers not found under {}; building libargus-sys without the \
             native shim (set JETSON_MULTIMEDIA_API_DIR to override)",
            mmapi_root.display()
        );
        return;
    }

    // Require the link libraries too: headers can be present without the
    // Tegra runtime (containers, sysroots), and emitting link directives in
    // that state would fail the final link instead of degrading gracefully.
    let Some(tegra_lib_dir) = find_tegra_lib_dir() else {
        println!(
            "cargo:warning=Tegra libraries (libnvargus_socketclient.so, libnvbufsurface.so) not \
             found; building libargus-sys without the native shim (set JETSON_TEGRA_LIB_DIR to \
             override)"
        );
        return;
    };

    println!("cargo:rerun-if-changed=src/lk_argus.cpp");
    println!("cargo:rerun-if-changed=src/lk_argus.h");

    cc::Build::new()
        .cpp(true)
        .file("src/lk_argus.cpp")
        .include("src")
        .include(&argus_include)
        .include(&mmapi_include)
        .flag("-std=c++14")
        .flag("-Wno-deprecated-declarations")
        .compile("lk_argus");

    println!("cargo:rustc-cfg=libargus_available");
    println!("cargo:rustc-link-search=native={}", tegra_lib_dir.display());
    println!("cargo:rustc-link-search=native=/usr/lib/aarch64-linux-gnu");
    println!("cargo:rustc-link-lib=dylib=nvargus_socketclient");
    println!("cargo:rustc-link-lib=dylib=nvbufsurface");

    // Communicate availability to dependent crates via `DEP_LK_ARGUS_AVAILABLE`.
    println!("cargo:available=1");
}
