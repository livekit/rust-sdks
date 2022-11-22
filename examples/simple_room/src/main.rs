use livekit::room::track::TrackTrait;
use livekit::room::{track::remote_track::RemoteTrackHandle, Room};
use std::sync::{Arc, Mutex};
use tracing::{event_enabled, info, trace};

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY3MzAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ4MDY3MzAsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.VbDoULjX1CVGZu2sPy3SvWYlVZUBXxQVPmdB9BnmlN4

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let room = Room::new();
    room.events()
        .on_participant_connected(|event| async move {});

    room.events().on_track_subscribed(|event| async move {
        let track = event.publication.track().unwrap();
        if let RemoteTrackHandle::Video(video_track) = track {
            let rtc_track = video_track.rtc_track();
            rtc_track.on_frame(Box::new(|frame| { Box::pin(async move {
                
            


            }) }))
        }
    });
}
