use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "../protocol/livekit_egress.proto",
            "../protocol/livekit_room.proto",
            "../protocol/livekit_webhook.proto",
            "../protocol/livekit_models.proto",
        ],
        &["../protocol/"],
    )?;
    Ok(())
}
