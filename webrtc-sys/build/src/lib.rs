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

use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, BufRead, Write},
    path,
    process::Command,
};

use fs2::FileExt;
use regex::Regex;
use reqwest::StatusCode;

pub const SCRATH_PATH: &str = "livekit_webrtc";
pub const WEBRTC_TAG: &str = "webrtc-b951613-4";
pub const IGNORE_DEFINES: [&str; 2] = ["CR_CLANG_REVISION", "CR_XCODE_VERSION"];

pub fn target_os() -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target = env::var("TARGET").unwrap();
    let is_simulator = target.ends_with("-sim");

    match target_os.as_str() {
        "windows" => "win",
        "macos" => "mac",
        "ios" => {
            if is_simulator {
                "ios-simulator"
            } else {
                "ios-device"
            }
        }
        _ => &target_os,
    }
    .to_string()
}

pub fn target_arch() -> String {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    match target_arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => &target_arch,
    }
    .to_owned()
}

/// The full name of the webrtc library
/// e.g. mac-x64-release (Same name on GH releases)
pub fn webrtc_triple() -> String {
    let profile = if use_debug() { "debug" } else { "release" };
    format!("{}-{}-{}", target_os(), target_arch(), profile)
}

/// Using debug builds of webrtc is still experimental for now
/// On Windows, Rust doesn't link against libcmtd on debug, which is an issue
/// Default to false (even on cargo debug)
pub fn use_debug() -> bool {
    let var = env::var("LK_DEBUG_WEBRTC");
    var.is_ok() && var.unwrap() == "true"
}

/// The location of the custom build is defined by the user
pub fn custom_dir() -> Option<path::PathBuf> {
    if let Ok(path) = env::var("LK_CUSTOM_WEBRTC") {
        return Some(path::PathBuf::from(path));
    }
    None
}

/// Location of the downloaded webrtc binaries
/// The reason why we don't use OUT_DIR is because we sometimes need to share the same binaries
/// across multiple crates without dependencies constraints
/// This also has the benefit of not re-downloading the binaries for each crate
pub fn prebuilt_dir() -> path::PathBuf {
    let target_dir = scratch::path(SCRATH_PATH);
    path::Path::new(&target_dir).join(format!(
        "livekit/{}-{}/{}",
        webrtc_triple(),
        WEBRTC_TAG,
        webrtc_triple()
    ))
}

pub fn download_url() -> String {
    format!(
        "https://github.com/livekit/client-sdk-rust/releases/download/{}/{}.zip",
        WEBRTC_TAG,
        format!("webrtc-{}", webrtc_triple())
    )
}

/// Used location of libwebrtc depending on whether it's a custom build or not
pub fn webrtc_dir() -> path::PathBuf {
    if let Some(path) = custom_dir() {
        return path;
    }

    prebuilt_dir()
}

pub fn webrtc_defines() -> Vec<(String, Option<String>)> {
    // read preprocessor definitions from webrtc.ninja
    let defines_re = Regex::new(r"-D(\w+)(?:=([^\s]+))?").unwrap();
    let webrtc_gni = fs::File::open(webrtc_dir().join("webrtc.ninja")).unwrap();

    let mut defines_line = String::default();
    io::BufReader::new(webrtc_gni).read_line(&mut defines_line).unwrap();

    let mut vec = Vec::default();
    for cap in defines_re.captures_iter(&defines_line) {
        let define_name = &cap[1];
        let define_value = cap.get(2).map(|m| m.as_str());
        if IGNORE_DEFINES.contains(&define_name) {
            continue;
        }

        vec.push((define_name.to_owned(), define_value.map(str::to_string)));
    }

    vec
}

pub fn configure_jni_symbols() -> Result<(), Box<dyn Error>> {
    download_webrtc()?;

    let toolchain = android_ndk_toolchain()?;
    let toolchain_bin = toolchain.join("bin");

    let webrtc_dir = webrtc_dir();
    let webrtc_lib = webrtc_dir.join("lib");

    let out_dir = path::PathBuf::from(env::var("OUT_DIR").unwrap());

    // Find JNI symbols
    let readelf_output = Command::new(toolchain_bin.join("llvm-readelf"))
        .arg("-Ws")
        .arg(webrtc_lib.join("libwebrtc.a"))
        .output()
        .expect("failed to run llvm-readelf");

    let jni_regex = Regex::new(r"(Java_org_webrtc.*)").unwrap();
    let content = String::from_utf8_lossy(&readelf_output.stdout);
    let jni_symbols: Vec<&str> =
        jni_regex.captures_iter(&content).map(|cap| cap.get(1).unwrap().as_str()).collect();

    if jni_symbols.is_empty() {
        return Err("No JNI symbols found".into()); // Shouldn't happen
    }

    // Keep JNI symbols
    for symbol in &jni_symbols {
        println!("cargo:rustc-link-arg=-Wl,--undefined={}", symbol);
    }

    // Version script
    let vs_path = out_dir.join("webrtc_jni.map");
    let mut vs_file = fs::File::create(&vs_path).unwrap();

    let jni_symbols = jni_symbols.join("; ");
    write!(vs_file, "JNI_WEBRTC {{\n\tglobal: {}; \n}};", jni_symbols).unwrap();

    println!("cargo:rustc-link-arg=-Wl,--version-script={}", vs_path.display());

    Ok(())
}

pub fn download_webrtc() -> Result<(), Box<dyn Error>> {
    let dir = scratch::path(SCRATH_PATH);
    let flock = File::create(dir.join(".lock"))?;
    flock.lock_exclusive()?;

    let webrtc_dir = webrtc_dir();
    if webrtc_dir.exists() {
        return Ok(());
    }

    let mut resp = reqwest::blocking::get(download_url())?;
    if resp.status() != StatusCode::OK {
        return Err(format!("failed to download webrtc: {}", resp.status()).into());
    }

    let tmp_path = env::var("OUT_DIR").unwrap() + "/webrtc.zip";
    let tmp_path = path::Path::new(&tmp_path);
    let mut file = fs::File::options().write(true).read(true).create(true).open(tmp_path)?;
    resp.copy_to(&mut file)?;

    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(webrtc_dir.parent().unwrap())?;
    drop(archive);

    fs::remove_file(tmp_path)?;
    Ok(())
}

pub fn android_ndk_toolchain() -> Result<path::PathBuf, &'static str> {
    let host_os = host_os();

    let home = env::var("HOME");
    let local = env::var("LOCALAPPDATA");

    let home = if host_os == Some("linux") {
        path::PathBuf::from(home.unwrap())
    } else if host_os == Some("darwin") {
        path::PathBuf::from(home.unwrap()).join("Library")
    } else if host_os == Some("windows") {
        path::PathBuf::from(local.unwrap())
    } else {
        return Err("Unsupported host OS");
    };

    let ndk_dir = || -> Option<path::PathBuf> {
        let ndk_env = env::var("ANDROID_NDK_HOME");
        if let Ok(ndk_env) = ndk_env {
            return Some(path::PathBuf::from(ndk_env));
        }

        let ndk_dir = home.join("Android/sdk/ndk");
        if !ndk_dir.exists() {
            return None;
        }

        // Find the highest version
        let versions = fs::read_dir(ndk_dir.clone());
        if versions.is_err() {
            return None;
        }

        let version = versions
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|dir| dir.file_name().to_str().map(ToOwned::to_owned))
            .filter_map(|dir| semver::Version::parse(&dir).ok())
            .max_by(semver::Version::cmp);

        version.as_ref()?;

        let version = version.unwrap();
        Some(ndk_dir.join(version.to_string()))
    }();

    if let Some(ndk_dir) = ndk_dir {
        let llvm_dir = if host_os == Some("linux") {
            "linux-x86_64"
        } else if host_os == Some("darwin") {
            "darwin-x86_64"
        } else if host_os == Some("windows") {
            "windows-x86_64"
        } else {
            return Err("Unsupported host OS");
        };

        Ok(ndk_dir.join(format!("toolchains/llvm/prebuilt/{}", llvm_dir)))
    } else {
        Err("Android NDK not found, please set ANDROID_NDK_HOME to your NDK path")
    }
}

fn host_os() -> Option<&'static str> {
    let host = env::var("HOST").unwrap();
    if host.contains("darwin") {
        Some("darwin")
    } else if host.contains("linux") {
        Some("linux")
    } else if host.contains("windows") {
        Some("windows")
    } else {
        None
    }
}
