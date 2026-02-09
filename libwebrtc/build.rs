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
use std::process::Command;

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let _target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let _is_desktop = target_os == "linux" || target_os == "windows" || target_os == "macos";

    println!("cargo:rerun-if-env-changed=LK_DEBUG_WEBRTC");
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");

    let use_dylib = !cfg!(feature = "static");
    let webrtc_dir = webrtc_sys_build::webrtc_dir();

    if use_dylib {
        if let Err(e) = webrtc_sys_build::copy_dylib_to_target(&webrtc_dir) {
            println!("cargo:warning=failed to copy livekit_rtc dylib to target: {}", e);
        }
    }

    if use_dylib {
        webrtc_sys_build::link_shared_library(&webrtc_dir);
    } else {
        match target_os.as_str() {
            "macos" => {
                configure_darwin_satic_link_flags();
            }
            "ios" => {
                configure_darwin_satic_link_flags();
            }
            //TODO(duan): add other OS specific flags here
            _ => {}
        }
        webrtc_sys_build::link_static_library(&webrtc_dir);
    }
}

fn configure_darwin_satic_link_flags() {
    let target_os = webrtc_sys_build::target_os();

    println!("cargo:rustc-link-arg=-ObjC");
    println!("cargo:rustc-link-arg=-std=c++20");
    println!("cargo:rustc-link-lib={}", "c++");

    match target_os.as_str() {
        "mac" => {
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
            println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
        }
        "ios-device" | "ios-simulator" => {
            println!("cargo:rustc-link-lib=framework=Foundation");
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
        _ => {}
    }

    let clang_rt = match target_os.as_str() {
        "mac" => "clang_rt.osx",
        "ios-device" => "clang_rt.ios",
        "ios-simulator" => "clang_rt.iossim",
        _ => panic!("Unsupported target_os: {}", target_os),
    };

    let sdk = match target_os.as_str() {
        "mac" => "macosx",
        "ios-device" => "iphoneos",
        "ios-simulator" => "iphonesimulator",
        _ => panic!("Unsupported target_os: {}", target_os),
    };

    println!("cargo:rustc-link-lib={}", clang_rt);
    let sysroot = Command::new("xcrun").args(["--sdk", sdk, "--show-sdk-path"]).output().unwrap();
    let sysroot = String::from_utf8_lossy(&sysroot.stdout);
    let sysroot = sysroot.trim();
    let search_dirs = Command::new("cc").arg("--print-search-dirs").output().unwrap();
    let search_dirs = String::from_utf8_lossy(&search_dirs.stdout);
    for line in search_dirs.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1).unwrap();
            let path = format!("{}/lib/darwin", path);
            println!("cargo:rustc-link-search={}", path);
        }
    }
    println!("cargo:rustc-link-arg=--sysroot={}", sysroot);
}
