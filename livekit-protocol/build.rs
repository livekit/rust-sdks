use std::io::Result;

fn main() -> Result<()> {
    // Try to get serde support on our protocol messages
    // This is required for some parts of our infrastructure (e.g webhooks)
    //
    // NOTE: This is not the "best" way to support serde, some advanced protobuf types may not work
    let mut prost_build = prost_build::Config::new();
    prost_build
        .compile_well_known_types() // Required for Timestamp to use the attributes bellow
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(
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
