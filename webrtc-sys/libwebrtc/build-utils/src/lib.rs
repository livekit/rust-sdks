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

use fs2::FileExt;
use std::{env, error::Error, ffi::OsStr, fs, io::Write, path};

// env var
pub const LK_WEBRTC_DEBUG: &str = "LK_RTC_DEBUG";
pub const LK_WEBRTC_PATH: &str = "LK_RTC_PATH";

pub const SCRATH_PATH: &str = "livekit_rtc";
pub const WEBRTC_TAG: &str = "webrtc-b951613-4";
pub const DOWNLOAD_URL: &str = "https://github.com/livekit/rust-sdks/releases/download/{}/{}.zip";

// dir structure of the webrtc build looks like:
// mac-arm64-release
//   - include
//   - lib
//   - LICENSE.md

pub fn target_os() -> &'static str {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target = env::var("TARGET").unwrap();
    let simulator = target.ends_with("-sim");

    match target_os.as_str() {
        "windows" => "win",
        "macos" => "mac",
        "android" => "android",
        "linux" => "linux",
        "ios" => {
            if simulator {
                "ios-simulator"
            } else {
                "ios-device"
            }
        }
        _ => panic!("unsupported rtc target_os: {}", target_os),
    }
}

pub fn target_arch() -> &'static str {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    match target_arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => panic!("unsupported rtc target_arch: {}", target_arch),
    }
}

pub fn rtc_debug_enabled() -> bool {
    let debug_var = env::var(LK_WEBRTC_DEBUG);
    debug_var.is_ok() && (debug_var.clone().unwrap() == "1" || debug_var.unwrap() == "true")
}

pub fn webrtc_triple(target_os: &str, target_arch: &str, debug: bool) -> String {
    let profile = if debug { "debug" } else { "release" };
    format!("{}-{}-{}", target_os, target_arch, profile)
}

pub fn custom_rtc_directory() -> Option<path::PathBuf> {
    let custom_path = env::var(LK_WEBRTC_PATH);
    if let Ok(custom_path) = custom_path {
        return Some(path::Path::new(&custom_path).to_path_buf());
    }
    None
}

pub fn is_using_custom_webrtc() -> bool {
    custom_rtc_directory().is_some()
}

pub fn rtc_directory() -> path::PathBuf {
    let target_dir = scratch::path(SCRATH_PATH);
    let rtc_triple = webrtc_triple(target_os(), target_arch(), rtc_debug_enabled());
    path::Path::new(&target_dir)
        .join(format!("livekit/{}-{}/{}", rtc_triple, WEBRTC_TAG, rtc_triple))
}

/// Make sure the JNI symbols of libwebrtc are correctly exported and visible in the final shared library.
/// Calling this function should only be needed when statically linking livekit_rtc.
pub fn export_jni_symbols() -> Result<(), Box<dyn Error>> {
    let jni_symbols = include_str!("../jni_symbols.txt");
    let jni_symbols = jni_symbols.lines().collect::<Vec<_>>();

    for symbol in &jni_symbols {
        println!("cargo:rustc-link-arg=-Wl,--undefined={}", symbol);
    }

    let out = env::var("OUT_DIR").unwrap();
    let out_dir = path::Path::new(&out);

    let vs_path = out_dir.join("livekit_rtc_jni.map");
    let mut vs_file = fs::File::create(&vs_path).unwrap();

    let jni_symbols = jni_symbols.join("; ");
    write!(vs_file, "JNI_WEBRTC {{\n\tglobal: {}; \n}};", jni_symbols).unwrap();

    println!("cargo:rustc-link-arg=-Wl,--version-script={}", vs_path.display());
    Ok(())
}

pub fn copy_dylib_to_target(rtc_path: &path::PathBuf) -> Result<(), Box<dyn Error>> {
    if let Some(target_dir) = find_target_dir() {
        let build_mode = env::var("PROFILE").unwrap();
        let source_dylib = rtc_path.join("lib").join("liblivekit_rtc.dylib");
        println!("cargo:rerun-if-changed={}", source_dylib.display());
        let target_dylib = target_dir.join(build_mode).join("liblivekit_rtc.dylib");
        fs::copy(source_dylib, target_dylib)?;
        Ok(())
    } else {
        Err("could not find target dir".into())
    }
}

// from https://github.com/dtolnay/cxx/blob/1449ffbc412a5d5039714f398ae589c214335181/gen/build/src/target.rs#L10
fn find_target_dir() -> Option<path::PathBuf> {
    if let Some(target_dir) = env::var_os("CARGO_TARGET_DIR") {
        let target_dir = path::PathBuf::from(target_dir);
        if target_dir.is_absolute() {
            return Some(target_dir);
        } else {
            return None;
        };
    }

    let out_dir = path::PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let mut dir = out_dir.to_owned();
    loop {
        if dir.join(".rustc_info.json").exists()
            || dir.join("CACHEDIR.TAG").exists()
            || dir.file_name() == Some(OsStr::new("target"))
                && dir.parent().map_or(false, |parent| parent.join("Cargo.toml").exists())
        {
            return Some(dir);
        }

        if dir.pop() {
            continue;
        }

        return None;
    }
}

pub fn link_shared_library(rtc_path: &path::PathBuf) {
    let lib_search_path = rtc_path.join("lib");

    println!("cargo:rerun-if-env-changed={}", LK_WEBRTC_DEBUG);
    println!("cargo:rerun-if-env-changed={}", LK_WEBRTC_PATH);
    println!("cargo:rustc-link-search=native={}", lib_search_path.display());
    println!("cargo:rustc-link-lib=dylib=livekit_rtc");
}

pub fn link_static_library(rtc_path: &path::PathBuf) {
    let lib_search_path = rtc_path.join("lib");

    println!("cargo:rerun-if-env-changed={}", LK_WEBRTC_DEBUG);
    println!("cargo:rerun-if-env-changed={}", LK_WEBRTC_PATH);
    println!("cargo:rustc-link-search=native={}", lib_search_path.display());
    println!("cargo:rustc-link-lib=static=livekit_rtc");

    let target_os = target_os();
    // TODO(theomonnom): some of these libraries may not be needed
    match target_os {
        "windows" => {
            println!("cargo:rustc-link-lib=dylib=msdmo");
            println!("cargo:rustc-link-lib=dylib=wmcodecdspuuid");
            println!("cargo:rustc-link-lib=dylib=dmoguids");
            println!("cargo:rustc-link-lib=dylib=crypt32");
            println!("cargo:rustc-link-lib=dylib=iphlpapi");
            println!("cargo:rustc-link-lib=dylib=ole32");
            println!("cargo:rustc-link-lib=dylib=secur32");
            println!("cargo:rustc-link-lib=dylib=winmm");
            println!("cargo:rustc-link-lib=dylib=ws2_32");
            println!("cargo:rustc-link-lib=dylib=strmiids");
            println!("cargo:rustc-link-lib=dylib=d3d11");
            println!("cargo:rustc-link-lib=dylib=gdi32");
            println!("cargo:rustc-link-lib=dylib=dxgi");
            println!("cargo:rustc-link-lib=dylib=dwmapi");
            println!("cargo:rustc-link-lib=dylib=shcore");
        }
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=Xext");
            println!("cargo:rustc-link-lib=dylib=X11");
            println!("cargo:rustc-link-lib=dylib=GL");
            println!("cargo:rustc-link-lib=dylib=rt");
            println!("cargo:rustc-link-lib=dylib=dl");
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=m");
        }
        "macos" => {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=AVFoundation");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=Appkit");
            println!("cargo:rustc-link-lib=framework=CoreMedia");
            println!("cargo:rustc-link-lib=framework=CoreGraphics");
            println!("cargo:rustc-link-lib=framework=VideoToolbox");
            println!("cargo:rustc-link-lib=framework=CoreVideo");
            println!("cargo:rustc-link-lib=framework=OpenGL");
            println!("cargo:rustc-link-lib=framework=Metal");
            println!("cargo:rustc-link-lib=framework=MetalKit");
            println!("cargo:rustc-link-lib=framework=QuartzCore");
            println!("cargo:rustc-link-lib=framework=IOKit");
            println!("cargo:rustc-link-lib=framework=IOSurface");
        }
        "ios" => {
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=framework=AVFoundation");
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=UIKit");
            println!("cargo:rustc-link-lib=framework=CoreVideo");
            println!("cargo:rustc-link-lib=framework=CoreGraphics");
            println!("cargo:rustc-link-lib=framework=CoreMedia");
            println!("cargo:rustc-link-lib=framework=VideoToolbox");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=OpenGLES");
            println!("cargo:rustc-link-lib=framework=GLKit");
            println!("cargo:rustc-link-lib=framework=Metal");
            println!("cargo:rustc-link-lib=framework=MetalKit");
            println!("cargo:rustc-link-lib=framework=Network");
            println!("cargo:rustc-link-lib=framework=QuartzCore");
        }
        "android" => {
            println!("cargo:rustc-link-lib=EGL");
            println!("cargo:rustc-link-lib=c++abi");
            println!("cargo:rustc-link-lib=OpenSLES");
        }
        _ => {
            panic!("Unsupported target, {}", target_os);
        }
    }
}

/// dylib and static library are on different arifacts (for file size reasons)
fn download_url(debug: bool, dylib: bool) -> String {
    let lib_type = if dylib { "dylib" } else { "static" };
    format!(
        "https://github.com/livekit/rust-sdks/releases/download/{}/{}-{}.zip",
        WEBRTC_TAG,
        format!("webrtc-{}", webrtc_triple(target_os(), target_arch(), debug)),
        lib_type
    )
}

pub fn download_webrtc_if_needed(debug: bool, dylib: bool) -> Result<(), Box<dyn Error>> {
    let dir = scratch::path(SCRATH_PATH);
    let flock = fs::File::create(dir.join(".lock"))?;
    flock.lock_exclusive()?;

    let rtc_dir = rtc_directory();
    if rtc_dir.exists() {
        return Ok(());
    }

    let resp = ureq::get(&download_url(debug, dylib)).call()?;
    if resp.status() != 200 {
        return Err(format!("failed to download webrtc: {}", resp.status()).into());
    }

    let mut reader = resp.into_reader();
    let tmp_path = env::var("OUT_DIR").unwrap();
    let tmp_path = path::Path::new(&tmp_path).join("webrtc.zip");
    let mut file =
        fs::File::options().write(true).read(true).create(true).open(tmp_path.clone())?;
    std::io::copy(&mut reader, &mut file)?;

    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(rtc_dir.parent().unwrap())?;
    drop(archive);

    fs::remove_file(tmp_path)?;
    Ok(())
}
