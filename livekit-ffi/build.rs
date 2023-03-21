use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "protocol/ffi.proto",
            "protocol/handle.proto",
            "protocol/room.proto",
            "protocol/track.proto",
            "protocol/participant.proto",
        ],
        &["protocol/"],
    )?;
    Ok(())
}
