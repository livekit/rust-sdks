use livekit::{events::TrackSubscribedEvent, room::SimulateScenario};

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect { url: String, token: String },
    SimulateScenario { scenario: SimulateScenario }
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
