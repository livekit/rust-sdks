use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "protocol/livekit_rtc.proto",
            "protocol/livekit_models.proto",
        ],
        &["protocol/"],
    )?;
    Ok(())
}
