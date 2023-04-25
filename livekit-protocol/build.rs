use std::io::Result;

fn main() -> Result<()> {
    let mut prost_build = prost_build::Config::new();
    prost_build.compile_protos(
        &[
            "protocol/livekit_egress.proto",
            "protocol/livekit_rtc.proto",
            "protocol/livekit_room.proto",
            "protocol/livekit_webhook.proto",
            "protocol/livekit_models.proto",
        ],
        &["protocol/"],
    )?;
    Ok(())
}
