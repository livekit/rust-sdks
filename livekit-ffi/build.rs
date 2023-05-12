use std::io::Result;

fn main() -> Result<()> {
    let mut prost_build = prost_build::Config::new();
    prost_build.protoc_arg("--experimental_allow_proto3_optional");
    prost_build.compile_protos(
        &[
            "protocol/ffi.proto",
            "protocol/handle.proto",
            "protocol/room.proto",
            "protocol/track.proto",
            "protocol/participant.proto",
            "protocol/video_frame.proto",
            "protocol/audio_frame.proto",
        ],
        &["protocol/"],
    )?;
    Ok(())
}
