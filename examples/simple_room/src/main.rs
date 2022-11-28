use std::time::Duration;

use livekit::room::RoomError;
use livekit::room::{track::remote_track::RemoteTrackHandle, Room};
use tokio::time::sleep;

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY3MzAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ4MDY3MzAsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.VbDoULjX1CVGZu2sPy3SvWYlVZUBXxQVPmdB9BnmlN4

#[tokio::main]
async fn main() -> Result<(), RoomError> {
    tracing_subscriber::fmt::init();

    let mut room = Room::new();
    room.events()
        .on_participant_connected(|_event| async move {});

    room.events().on_track_subscribed(|event| async move {
        let track = event.publication.track().unwrap();
        if let RemoteTrackHandle::Video(video_track) = track {
            let rtc_track = video_track.rtc_track();
            rtc_track.set_should_receive(true);
            rtc_track.on_frame(Box::new(|_frame, _buffer| {
                // called on libwebrtc worker_thread
                println!("Received frame");
            }));
        }
    });

    room.connect(URL, TOKEN).await?;

    sleep(Duration::from_secs(200)).await;
    Ok(())
}
