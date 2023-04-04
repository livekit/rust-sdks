use super::ServiceBase;

pub struct RoomClient {
    base: ServiceBase,
    client: TwirpClient,
}

impl RoomClient {
    pub fn new() -> Self {
        Self {
            client: TwirpClient::new(),
        }
    }
}
