use livekit::{
    options::TrackPublishOptions,
    prelude::*,
    track::{LocalTrack, LocalVideoTrack, TrackSource},
    webrtc::video_frame::{native::NativeBuffer, VideoBuffer},
    webrtc::video_source::{native::NativeVideoSource, RtcVideoSource, VideoResolution},
    Room, RoomError,
};
use std::sync::Arc;
use tokio::{sync::oneshot, task::JoinHandle};

struct TrackHandle {
    close_tx: oneshot::Sender<()>,
    track: LocalVideoTrack,
    task: JoinHandle<()>,
}

pub struct ScreenshareTrack {
    rtc_source: NativeVideoSource,
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

impl ScreenshareTrack {
    pub fn new(room: Arc<Room>) -> Self {
        Self {
            rtc_source: NativeVideoSource::new(VideoResolution { width: 3024, height: 1964 }),
            room,
            handle: None,
        }
    }

    pub fn is_published(&self) -> bool {
        self.handle.is_some()
    }

    pub async fn publish(&mut self) -> Result<(), RoomError> {
        self.unpublish().await?;

        let (close_tx, close_rx) = oneshot::channel();
        let track = LocalVideoTrack::create_video_track(
            "screenshare",
            RtcVideoSource::Native(self.rtc_source.clone()),
        );

        let task = tokio::spawn(Self::track_task(close_rx, self.rtc_source.clone()));

        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions { source: TrackSource::Screenshare, ..Default::default() },
            )
            .await?;

        let handle = TrackHandle { close_tx, task, track };

        self.handle = Some(handle);
        Ok(())
    }

    async fn track_task(mut close_rx: oneshot::Receiver<()>, rtc_source: NativeVideoSource) {}

    pub async fn unpublish(&mut self) -> Result<(), RoomError> {
        if let Some(handle) = self.handle.take() {
            let _ = handle.close_tx.send(());
            let _ = handle.task.await;

            self.room.local_participant().unpublish_track(&handle.track.sid()).await?;
        }
        Ok(())
    }
}

impl Drop for ScreenshareTrack {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.close_tx.send(());
        }
    }
}
