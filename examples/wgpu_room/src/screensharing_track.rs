use image::RgbaImage;
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::video_source::RtcVideoSource;
use livekit::webrtc::video_source::VideoResolution;
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::native::I420BufferExt,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::native::NativeVideoSource,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;

struct TrackHandle {
    close_tx: oneshot::Sender<()>,
    track: LocalVideoTrack,
    task: JoinHandle<()>,
}

pub struct ScreensharingTrack {
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

impl ScreensharingTrack {
    pub fn new(room: Arc<Room>) -> Self {
        Self { room, handle: None }
    }

    pub fn is_published(&self) -> bool {
        self.handle.is_some()
    }

    pub async fn publish(&mut self) -> Result<(), RoomError> {
        self.unpublish().await?;

        let rtc_source = NativeVideoSource::new(VideoResolution {
            width: WIDTH as u32,
            height: HEIGHT as u32,
        });

        let track = LocalVideoTrack::create_video_track(
            "native-screen-sharing",
            RtcVideoSource::Native(rtc_source.clone()),
        );

        let (close_tx, close_rx) = oneshot::channel();
        let task = tokio::spawn(Self::track_task(close_rx, rtc_source));

        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Screenshare,
                    ..Default::default()
                },
            )
            .await?;

        let handle = TrackHandle {
            close_tx,
            task,
            track,
        };

        self.handle = Some(handle);
        Ok(())
    }

    pub async fn unpublish(&mut self) -> Result<(), RoomError> {
        if let Some(handle) = self.handle.take() {
            let _ = handle.close_tx.send(());
            let _ = handle.task.await;

            self.room
                .local_participant()
                .unpublish_track(&handle.track.sid())
                .await?;
        }
        Ok(())
    }

    async fn track_task(mut close_rx: oneshot::Receiver<()>, rtc_source: NativeVideoSource) {
        const PIXEL_SIZE: usize = 4;
        const FPS: usize = 15;

        let video_frame = Arc::new(Mutex::new(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            buffer: I420Buffer::new(WIDTH as u32, HEIGHT as u32),
            timestamp_us: 0,
        }));

        let mut interval = tokio::time::interval(Duration::from_millis(1000 / FPS as u64));
        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    return;
                }
                _ = interval.tick() => {}
            }

            tokio::task::spawn_blocking({
                // TODO: Use `spawn_blocking()` here to actually capture the native frame.
                let image: RgbaImage = RgbaImage::new(WIDTH as u32, HEIGHT as u32);

                let (source, frame) = (rtc_source.clone(), video_frame.clone());
                move || {
                    let mut video_frame = frame.lock();
                    let i420_buffer = &mut video_frame.buffer;
                    let (stride_y, stride_u, stride_v) = i420_buffer.strides();
                    let (data_y, data_u, data_v) = i420_buffer.data_mut();

                    yuv_helper::abgr_to_i420(
                        &*image,
                        (WIDTH * PIXEL_SIZE) as u32,
                        data_y,
                        stride_y,
                        data_u,
                        stride_u,
                        data_v,
                        stride_v,
                        WIDTH as i32,
                        HEIGHT as i32,
                    );

                    source.capture_frame(&*video_frame);
                }
            })
            .await
            .unwrap();
        }
    }
}

impl Drop for ScreensharingTrack {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.close_tx.send(());
        }
    }
}
