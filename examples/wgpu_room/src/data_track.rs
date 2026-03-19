use futures::StreamExt;
use livekit::prelude::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const TIME_WINDOW: Duration = Duration::from_secs(30);
pub const MAX_VALUE: f32 = 512.0;

pub struct LocalDataTrackTile {
    track: LocalDataTrack,
    pub slider_value: i32,
    pub points: Arc<Mutex<VecDeque<(Instant, i32)>>>,
    pub name: String,
}

impl LocalDataTrackTile {
    pub fn new(track: LocalDataTrack) -> Self {
        let name = track.info().name().to_string();
        Self { track, slider_value: 0, points: Arc::new(Mutex::new(VecDeque::new())), name }
    }

    pub fn push_value(&self) {
        let frame = DataTrackFrame::new(self.slider_value.to_string().into_bytes());
        let _ = self.track.try_push(frame);
        self.points.lock().push_front((Instant::now(), self.slider_value));
    }
}

pub struct RemoteDataTrackTile {
    pub points: Arc<Mutex<VecDeque<(Instant, i32)>>>,
    pub publisher_identity: String,
    pub name: String,
}

impl RemoteDataTrackTile {
    pub fn new(async_handle: &tokio::runtime::Handle, track: RemoteDataTrack) -> Self {
        let points = Arc::new(Mutex::new(VecDeque::new()));
        let points_ref = points.clone();
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
                let payload = frame.payload();
                let Ok(s) = std::str::from_utf8(&payload) else { continue };
                let Ok(value) = s.parse::<i32>() else { continue };
                points_ref.lock().push_front((Instant::now(), value));
            }
        });

        Self { points, publisher_identity, name }
    }
}
