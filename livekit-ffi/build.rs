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

fn main() {
    // livekit-ffi/core-modules is pure Rust: no libwebrtc download, no linker
    // configuration (`configure_linker` panics on visionOS/tvOS/Mac Catalyst,
    // which the Swift package builds) and no protobuf generation.
    //
    // The whole module is gated rather than each item in it, and it has to be a
    // `cfg` rather than a `CARGO_FEATURE_ROOM_APIS` lookup: `prost-build` and
    // `webrtc-sys-build` are optional build-dependencies gated on the same
    // feature, so under core-modules the paths that this code names do not
    // resolve at all.
    //
    // The gate is `__room-surface`, the base of both room builds: the
    // host-transport build links the same libwebrtc and generates the same
    // protobuf code as `room-apis`.
    #[cfg(feature = "__room-surface")]
    if std::env::var("DOCS_RS").is_err() {
        room_apis::run_build_steps();
    }
}

/// The build steps that only the room-apis surface needs.
#[cfg(feature = "__room-surface")]
mod room_apis {
    use std::{env, path::Path};

    const PROTO_SRC_DIR: &str = "protocol";

    pub fn run_build_steps() {
        download_webrtc();
        copy_webrtc_license();
        configure_linker();
        generate_protobuf();
        uniffi_build::generate_scaffolding("src/core_objects.udl").expect("core object UDL");
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
        let mut config = prost_build::Config::new();
        config
            .derive_from_variants("livekit.proto.FfiRequest.message")
            .derive_from_variants("livekit.proto.FfiResponse.message")
            .derive_from_variants("livekit.proto.FfiEvent.message")
            .derive_from_variants("livekit.proto.RoomEvent.message")
            .from_variants_skip_field("livekit.proto.RoomEvent.message.room_updated")
            .from_variants_skip_field("livekit.proto.RoomEvent.message.moved")
            .derive_from_variants("livekit.proto.AudioStreamEvent.message")
            .derive_from_variants("livekit.proto.TextStreamReaderEvent.detail")
            .derive_from_variants("livekit.proto.ByteStreamReaderEvent.detail")
            .derive_from_variants("livekit.proto.DataTrackStreamEvent.detail");
        let descriptors = config.load_fds(&paths, &[PROTO_SRC_DIR]).expect("Protobuf descriptors");
        // Export the existing stats representation, not a second hand-maintained field mapping.
        // This adds UniFFI metadata only; protobuf field numbers and the C callback stay unchanged.
        let stats = descriptors
            .file
            .iter()
            .find(|file| file.name() == "stats.proto")
            .expect("stats.proto descriptor");
        for message in &stats.message_type {
            // message_attribute also covers nested messages.
            config.message_attribute(
                format!(".{}.{}", stats.package(), message.name()),
                "#[derive(uniffi::Record)]",
            );
        }
        // Protobuf nesting is flattened by UniFFI. Prefix its generic payload names so
        // Track/Stream/Codec cannot shadow the host SDK's existing public types.
        for message in &stats.message_type {
            for nested in &message.nested_type {
                config.message_attribute(
                    format!(".{}.{}.{}", stats.package(), message.name(), nested.name()),
                    format!("#[uniffi(name = \"RtcStats{}\")]", nested.name()),
                );
            }
        }
        config.enum_attribute(
            ".livekit.proto.RtcStats.stats",
            "#[derive(uniffi::Enum)]\n#[uniffi(name = \"RtcStatsKind\")]",
        );
        config.compile_fds(descriptors).expect("Protobuf generation failed");
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
}
