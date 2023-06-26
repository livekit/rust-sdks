use std::{env, io::Result, path::PathBuf};

fn main() -> Result<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let descriptor_path = PathBuf::from(out_dir).join("proto_descriptor.bin");

    let proto_files = &[
        "protocol/livekit_egress.proto",
        "protocol/livekit_rtc.proto",
        "protocol/livekit_room.proto",
        "protocol/livekit_webhook.proto",
        "protocol/livekit_models.proto",
    ];

    for file in proto_files {
        println!("cargo:rerun-if-changed={}", file);
    }

    let mut prost_build = prost_build::Config::new();

    if cfg!(feature = "json") {
        prost_build.extern_path(".google.protobuf", "::pbjson_types");
    }

    prost_build
        .file_descriptor_set_path(&descriptor_path) // Needed for pbjson
        .compile_well_known_types()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(proto_files, &["protocol/"])?;

    if cfg!(feature = "json") {
        let descriptor_set = std::fs::read(descriptor_path)?;
        pbjson_build::Builder::new()
            .register_descriptors(&descriptor_set)?
            .build(&[".livekit"])?;
    }

    Ok(())
}
