use futures::StreamExt;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

pub struct LocalDataTrackTile {
    track: LocalDataTrack,
    pub slider_value: i32,
    pub name: String,
}

impl LocalDataTrackTile {
    pub fn new(track: LocalDataTrack) -> Self {
        let name = track.info().name().to_string();
        Self { track, slider_value: 0, name }
    }

    pub fn push_value(&self) {
        let frame = DataTrackFrame::new(self.slider_value.to_string().into_bytes());
        let _ = self.track.try_push(frame);
    }
}

pub struct RemoteDataTrackTile {
    latest_payload: Arc<Mutex<Option<Vec<u8>>>>,
    pub publisher_identity: String,
    pub name: String,
}

impl RemoteDataTrackTile {
    pub fn new(async_handle: &tokio::runtime::Handle, track: RemoteDataTrack) -> Self {
        let latest_payload = Arc::new(Mutex::new(None));
        let payload_ref = latest_payload.clone();
        let publisher_identity = track.publisher_identity().to_string();
        let name = track.info().name().to_string();

        async_handle.spawn(async move {
            let mut stream = match track.subscribe().await {
                Ok(s) => s,
                Err(err) => {
                    log::error!("Failed to subscribe to data track: {err}");
                    return;
                }
            };
            while let Some(frame) = stream.next().await {
                *payload_ref.lock() = Some(frame.payload().to_vec());
            }
        });

        Self { latest_payload, publisher_identity, name }
    }

    pub fn latest_value_str(&self) -> Option<String> {
        let payload = self.latest_payload.lock();
        String::from_utf8(payload.as_ref()?.clone()).ok()
    }
}
