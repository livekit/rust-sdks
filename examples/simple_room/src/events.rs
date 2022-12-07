#[derive(Debug)]
pub enum AsyncCmd {
    RoomConnect { url: String, token: String },
}

#[derive(Debug)]
pub enum UiCmd {
    ConnectResult,
}
