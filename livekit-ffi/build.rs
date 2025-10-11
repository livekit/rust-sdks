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

use std::{env, path::Path};

const PROTO_SRC_DIR: &str = "protocol";

fn main() {
    if env::var("DOCS_RS").is_ok() {
        return;
    }
    download_webrtc();
    copy_webrtc_license();
    configure_linker();
    generate_protobuf();
}

fn download_webrtc() {
    let webrtc_dir = webrtc_sys_build::webrtc_dir();
    if !webrtc_dir.exists() {
        webrtc_sys_build::download_webrtc().unwrap();
    }
}

/// Copy the webrtc license to `CARGO_MANIFEST_DIR`, used by the FFI release action.
fn copy_webrtc_license() {
    let webrtc_dir = webrtc_sys_build::webrtc_dir();
    let license = webrtc_dir.join("LICENSE.md");
    let target_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_file = Path::new(&target_dir).join("WEBRTC_LICENSE.md");
    std::fs::copy(license, out_file).unwrap();
}

fn configure_linker() {
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
            panic!("Unsupported target, {}", target_os);
        }
    }
}

fn generate_protobuf() {
    let paths: Vec<_> = std::fs::read_dir(PROTO_SRC_DIR)
        .expect("Failed to read protobuf source directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "proto"))
        .collect();
    for path in &paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    prost_build::Config::new()
        .derive_from_variants("livekit.proto.FfiRequest.message")
        .derive_from_variants("livekit.proto.FfiResponse.message")
        .derive_from_variants("livekit.proto.FfiEvent.message")
        .derive_from_variants("livekit.proto.RoomEvent.message")
        .from_variants_skip_field("livekit.proto.RoomEvent.message.room_updated")
        .from_variants_skip_field("livekit.proto.RoomEvent.message.moved")
        .derive_from_variants("livekit.proto.AudioStreamEvent.message")
        .derive_from_variants("livekit.proto.TextStreamReaderEvent.detail")
        .derive_from_variants("livekit.proto.ByteStreamReaderEvent.detail")
        .compile_protos(&paths, &[PROTO_SRC_DIR])
        .expect("Protobuf generation failed");
}

trait ProstConfigExt {
    /// Derive [`from_variants::FromVariants`] on a oneof field's generated enum.
    fn derive_from_variants(&mut self, path: impl AsRef<str>) -> &mut Self;

    /// When using [`derive_from_variants`], skip a particular case. This is necessary
    /// for oneofs that contain multiple cases with the same message type.
    fn from_variants_skip_field(&mut self, path: impl AsRef<str>) -> &mut Self;
}

impl ProstConfigExt for prost_build::Config {
    fn derive_from_variants(&mut self, path: impl AsRef<str>) -> &mut Self {
        self.enum_attribute(path, "#[derive(from_variants::FromVariants)]")
    }
    fn from_variants_skip_field(&mut self, path: impl AsRef<str>) -> &mut Self {
        self.field_attribute(path, "#[from_variants(skip)]")
    }
}
