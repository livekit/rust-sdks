use livekit::room::Room;
use std::sync::{Arc, Mutex};
use tracing::{info, trace};

const URL: &str = "ws://localhost:7880";
const TOKEN : &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY0NzMsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJuYXRpdmUiLCJuYmYiOjE2NjQ4MDY0NzMsInN1YiI6Im5hdGl2ZSIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.BgVdBnq3XFD3_BQHoe1azqjifYysubgFl6Qlzu9IQGI";

// eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjIzODQ4MDY3MzAsImlzcyI6IkFQSXpLYkFTaUNWYWtnSiIsIm5hbWUiOiJ3ZWIiLCJuYmYiOjE2NjQ4MDY3MzAsInN1YiI6IndlYiIsInZpZGVvIjp7InJvb21DcmVhdGUiOnRydWUsInJvb21Kb2luIjp0cnVlfX0.VbDoULjX1CVGZu2sPy3SvWYlVZUBXxQVPmdB9BnmlN4

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let room = Room::new();
    room.on_participant_connected(async |participant| {


    })
}
