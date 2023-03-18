use livekit::prelude::*;
use livekit::{RoomResult, SimulateScenario};

#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect { url: String, token: String },
    RoomDisconnect,
    SimulateScenario { scenario: SimulateScenario },
    ToggleLogo, // Unpublish/Publish a logo track
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult { result: RoomResult<()> },
    RoomEvent { event: RoomEvent },
}
