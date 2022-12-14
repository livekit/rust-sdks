use livekit::events::TrackSubscribedEvent;

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect { url: String, token: String },
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult {
        result: livekit::room::RoomResult<()>,
    },
    TrackSubscribed {
        event: TrackSubscribedEvent,
    },
}
