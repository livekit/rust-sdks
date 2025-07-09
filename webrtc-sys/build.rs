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

use std::path::Path;
use std::path::PathBuf;
use std::{env, path, process::Command};

fn setup_system_abseil() -> PathBuf {
    // Use system-installed Abseil
    if let Ok(abseil_dir) = env::var("ABSEIL_DIR") {
        let abseil_path = PathBuf::from(abseil_dir);
        println!("Using system Abseil from: {}", abseil_path.display());

        // Verify the installation exists
        let absl_subdir = abseil_path.join("absl");
        if !absl_subdir.exists() {
            panic!("Abseil headers not found at: {}", absl_subdir.display());
        }

        return abseil_path;
    }

    // Auto-detect system Abseil installation
    let common_paths = [
        "/usr/include",
        "/usr/local/include",
        "/opt/abseil/include",
    ];

    for path in &common_paths {
        let abseil_path = PathBuf::from(path);
        let absl_subdir = abseil_path.join("absl");
        if absl_subdir.exists() {
            println!("Found system Abseil at: {}", abseil_path.display());
            return abseil_path;
        }
    }

    panic!("Could not find system Abseil installation. Please install libabsl-dev or set ABSEIL_DIR");
}

fn setup_custom_abseil() -> PathBuf {
    let out_dir = env::var("OUT_DIR").unwrap();
    let abseil_dir = PathBuf::from(&out_dir).join("abseil-cpp");

    // Check if we already have the right version
    let version_file = abseil_dir.join(".version");
    let target_version = "20240722.0";

    let needs_download = if version_file.exists() {
        std::fs::read_to_string(&version_file)
            .map(|v| v.trim() != target_version)
            .unwrap_or(true)
    } else {
        true
    };

    if needs_download {
        println!("Setting up Abseil version: {}", target_version);

        // Remove existing directory if it exists
        if abseil_dir.exists() {
            std::fs::remove_dir_all(&abseil_dir).unwrap();
        }

        // Clone the specific version
        let status = Command::new("git")
            .args(&[
                "clone",
                "--depth",
                "1",
                "--branch",
                &format!("{}", target_version),
                "https://github.com/abseil/abseil-cpp.git",
            ])
            .arg(&abseil_dir)
            .status();

        match status {
            Ok(status) if status.success() => {
                // Write version file
                std::fs::write(&version_file, target_version).unwrap();
                println!("Successfully cloned Abseil {}", target_version);
            }
            _ => {
                panic!("Failed to clone Abseil version {}", target_version);
            }
        }
    } else {
        println!("Using cached Abseil version: {}", target_version);
    }

    abseil_dir
}

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }

    println!("cargo:rerun-if-env-changed=LK_DEBUG_WEBRTC");
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");
    println!("cargo:rerun-if-env-changed=USE_CUSTOM_ABSEIL");
    println!("cargo:rerun-if-env-changed=ABSEIL_DIR");
    println!("cargo:rerun-if-env-changed=ABSEIL_LIB_DIR");

    let mut builder = cxx_build::bridges([
        "src/peer_connection.rs",
        "src/peer_connection_factory.rs",
        "src/media_stream.rs",
        "src/media_stream_track.rs",
        "src/audio_track.rs",
        "src/video_track.rs",
        "src/data_channel.rs",
        "src/frame_cryptor.rs",
        "src/jsep.rs",
        "src/candidate.rs",
        "src/rtp_parameters.rs",
        "src/rtp_sender.rs",
        "src/rtp_receiver.rs",
        "src/rtp_transceiver.rs",
        "src/rtc_error.rs",
        "src/webrtc.rs",
        "src/video_frame.rs",
        "src/video_frame_buffer.rs",
        "src/helper.rs",
        "src/yuv_helper.rs",
        "src/audio_resampler.rs",
        "src/prohibit_libsrtp_initialization.rs",
    ]);

    builder.files(&[
        "src/peer_connection.cpp",
        "src/peer_connection_factory.cpp",
        "src/media_stream.cpp",
        "src/media_stream_track.cpp",
        "src/audio_track.cpp",
        "src/video_track.cpp",
        "src/data_channel.cpp",
        "src/jsep.cpp",
        "src/candidate.cpp",
        "src/rtp_receiver.cpp",
        "src/rtp_sender.cpp",
        "src/rtp_transceiver.cpp",
        "src/rtp_parameters.cpp",
        "src/rtc_error.cpp",
        "src/webrtc.cpp",
        "src/video_frame.cpp",
        "src/video_frame_buffer.cpp",
        "src/video_encoder_factory.cpp",
        "src/video_decoder_factory.cpp",
        "src/audio_device.cpp",
        "src/audio_resampler.cpp",
        "src/frame_cryptor.cpp",
        "src/global_task_queue.cpp",
        "src/prohibit_libsrtp_initialization.cpp",
    ]);

    let webrtc_dir = webrtc_sys_build::webrtc_dir();
    let webrtc_include = webrtc_dir.join("include");
    let webrtc_lib = webrtc_dir.join("lib");

    if !webrtc_dir.exists() {
        webrtc_sys_build::download_webrtc().unwrap();
    }

    // Determine which Abseil to use
    let abseil_include = if env::var("USE_CUSTOM_ABSEIL").is_ok() {
        // Check if user wants system Abseil or downloaded Abseil
        if env::var("ABSEIL_DIR").is_ok() || env::var("USE_SYSTEM_ABSEIL").is_ok() {
            setup_system_abseil()
        } else {
            setup_custom_abseil()
        }
    } else {
        println!("Using WebRTC's bundled Abseil");
        webrtc_include.join("third_party/abseil-cpp/")
    };

    builder.includes(&[
        path::PathBuf::from("./include"),
        webrtc_include.clone(),
        abseil_include,
        webrtc_include.join("third_party/libyuv/include/"),
        webrtc_include.join("third_party/libc++/"),
    ]);

    // Configure Abseil behavior for custom/system installation
    if env::var("USE_CUSTOM_ABSEIL").is_ok() {
        // For system Abseil (version 20210324), use more conservative settings
        if env::var("ABSEIL_DIR").is_ok() || env::var("USE_SYSTEM_ABSEIL").is_ok() {
            println!("Configuring for system Abseil (Ubuntu 22.04 version)");

            // Use absl::optional instead of std::optional for older Abseil
            builder.define("ABSL_OPTION_USE_STD_OPTIONAL", Some("0"));

            // Add symbol isolation to prevent conflicts
            builder.define("ABSL_OPTION_USE_INLINE_NAMESPACE", Some("1"));
            builder.define("ABSL_OPTION_INLINE_NAMESPACE_NAME", Some("webrtc_absl"));
        } else {
            // For newer downloaded Abseil
            builder.define("ABSL_OPTION_USE_STD_OPTIONAL", Some("2"));
            builder.define("ABSL_USES_STD_OPTIONAL", None);
        }

        // Link against system Abseil libraries if library directory is specified
        if let Ok(abseil_lib_dir) = env::var("ABSEIL_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", abseil_lib_dir);

            // Link essential Abseil libraries (Ubuntu package names)
            println!("cargo:rustc-link-lib=dylib=absl_base");
            println!("cargo:rustc-link-lib=dylib=absl_strings");
            println!("cargo:rustc-link-lib=dylib=absl_synchronization");
            println!("cargo:rustc-link-lib=dylib=absl_time");
            println!("cargo:rustc-link-lib=dylib=absl_hash");
            println!("cargo:rustc-link-lib=dylib=absl_debugging");
        }
    }

    println!("cargo:rustc-link-search=native={}", webrtc_lib.to_str().unwrap());

    for (key, value) in webrtc_sys_build::webrtc_defines() {
        let value = value.as_deref();
        builder.define(key.as_str(), value);
    }

    // Link webrtc library
    println!("cargo:rustc-link-lib=static=webrtc");

    // Linux-specific libraries
    println!("cargo:rustc-link-lib=dylib=rt");
    println!("cargo:rustc-link-lib=dylib=dl");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=m");

    // Linux-specific C++ flags
    builder.flag("-std=c++2a");

    // Add linker flag to handle potential symbol conflicts gracefully
    println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");

    // TODO(theomonnom) Only add this define when building tests
    builder.define("LIVEKIT_TEST", None);
    builder.warnings(false).compile("webrtcsys-cxx");

    for entry in glob::glob("./src/**/*.cpp").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }

    for entry in glob::glob("./include/**/*.h").unwrap() {
        println!("cargo:rerun-if-changed={}", entry.unwrap().display());
    }
}
