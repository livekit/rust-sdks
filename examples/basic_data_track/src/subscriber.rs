use anyhow::Result;
use futures_util::StreamExt;
use livekit::prelude::*;
use std::env;
use tokio::{signal, sync::mpsc::UnboundedReceiver};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let token = env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN is not set");

    let (_, rx) = Room::connect(&url, &token, RoomOptions::default()).await?;

    tokio::select! {
        _ = handle_publications(rx) => {}
        _ = signal::ctrl_c() => {}
    }
    Ok(())
}

/// Subscribes to any published data tracks.
async fn handle_publications(mut rx: UnboundedReceiver<RoomEvent>) -> Result<()> {
    while let Some(event) = rx.recv().await {
        let RoomEvent::RemoteDataTrackPublished(track) = event else {
            continue;
        };
        subscribe(track).await?
    }
    Ok(())
}

/// Subscribes to the given data track and logs received frames.
async fn subscribe(track: RemoteDataTrack) -> Result<()> {
    log::info!(
        "Subscribing to '{}' published by '{}'",
        track.info().name(),
        track.publisher_identity()
    );
    let mut subscription = track.subscribe().await?;
    while let Some(frame) = subscription.next().await {
        log::info!("Received frame ({} bytes)", frame.payload().len());

        if let Some(duration) = frame.duration_since_timestamp() {
            log::info!("Latency: {:?}", duration);
        }
    }
    log::info!("Unsubscribed");
    Ok(())
}
