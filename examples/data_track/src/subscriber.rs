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
        Some(track) = wait_for_publication(rx) => subscribe(track).await?,
        _ = signal::ctrl_c() => {}
    }
    Ok(())
}

/// Waits for the first data track to be published and returns it.
async fn wait_for_publication(mut rx: UnboundedReceiver<RoomEvent>) -> Option<RemoteDataTrack> {
    while let Some(event) = rx.recv().await {
        match event {
            RoomEvent::RemoteDataTrackPublished(track) => return Some(track),
            _ => continue,
        }
    }
    None
}

/// Subscribes to the given data track and logs received frames.
async fn subscribe(track: RemoteDataTrack) -> Result<()> {
    log::info!(
        "Subscribing to '{}' published by '{}'",
        track.info().name(),
        track.publisher_identity()
    );
    let mut frame_steam = track.subscribe().await?;
    while let Some(frame) = frame_steam.next().await {
        log::info!("Received {} bytes", frame.payload().len());
    }
    Ok(())
}
