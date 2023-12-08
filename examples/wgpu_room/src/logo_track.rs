use image::ImageFormat;
use image::RgbaImage;
use livekit::options::TrackPublishOptions;
use livekit::options::VideoCodec;
use livekit::prelude::*;
use livekit::webrtc::video_source::RtcVideoSource;
use livekit::webrtc::video_source::VideoResolution;
use livekit::webrtc::{
    native::yuv_helper,
    video_frame::{I420Buffer, VideoFrame, VideoRotation},
    video_source::native::NativeVideoSource,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

// The logo must not be bigger than the framebuffer
const PIXEL_SIZE: usize = 4;
const FRAME_RATE: u64 = 30;
const MOVE_SPEED: i32 = 16;
const FB_WIDTH: usize = 1280;
const FB_HEIGHT: usize = 720;

#[derive(Clone)]
struct FrameData {
    image: Arc<RgbaImage>,
    framebuffer: Arc<Mutex<Vec<u8>>>,
    video_frame: Arc<Mutex<VideoFrame<I420Buffer>>>,
    pos: (i32, i32),
    direction: (i32, i32),
}

struct TrackHandle {
    close_tx: oneshot::Sender<()>,
    track: LocalVideoTrack,
    task: JoinHandle<()>,
}

pub struct LogoTrack {
    rtc_source: NativeVideoSource,
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

impl LogoTrack {
    pub fn new(room: Arc<Room>) -> Self {
        Self {
            rtc_source: NativeVideoSource::new(VideoResolution {
                width: FB_WIDTH as u32,
                height: FB_HEIGHT as u32,
            }),
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
            "livekit_logo",
            RtcVideoSource::Native(self.rtc_source.clone()),
        );

        let task = tokio::spawn(Self::track_task(close_rx, self.rtc_source.clone()));

        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    //simulcast: false,
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
        let mut interval = tokio::time::interval(Duration::from_millis(1000 / FRAME_RATE));

        let image = tokio::task::spawn_blocking(|| {
            image::load_from_memory_with_format(include_bytes!("moving-logo.png"), ImageFormat::Png)
                .unwrap()
                .to_rgba8()
        })
        .await
        .unwrap();

        let mut data = FrameData {
            image: Arc::new(image),
            framebuffer: Arc::new(Mutex::new(vec![0u8; FB_WIDTH * FB_HEIGHT * 4])),
            video_frame: Arc::new(Mutex::new(VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                buffer: I420Buffer::new(FB_WIDTH as u32, FB_HEIGHT as u32),
                timestamp_us: 0,
            })),
            pos: (0, 0),
            direction: (1, 1),
        };

        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    break;
                }
                _ = interval.tick() => {}
            }

            data.pos.0 += data.direction.0 * MOVE_SPEED;
            data.pos.1 += data.direction.1 * MOVE_SPEED;

            if data.pos.0 >= (FB_WIDTH - data.image.width() as usize) as i32 {
                data.direction.0 = -1;
            } else if data.pos.0 <= 0 {
                data.direction.0 = 1;
            }

            if data.pos.1 >= (FB_HEIGHT - data.image.height() as usize) as i32 {
                data.direction.1 = -1;
            } else if data.pos.1 <= 0 {
                data.direction.1 = 1;
            }

            tokio::task::spawn_blocking({
                let data = data.clone();
                let source = rtc_source.clone();
                move || {
                    let image = data.image.as_raw();
                    let mut framebuffer = data.framebuffer.lock();
                    let mut video_frame = data.video_frame.lock();
                    let i420_buffer = &mut video_frame.buffer;

                    let (stride_y, stride_u, stride_v) = i420_buffer.strides();
                    let (data_y, data_u, data_v) = i420_buffer.data_mut();

                    framebuffer.fill(0);
                    for i in 0..data.image.height() as usize {
                        let x = data.pos.0 as usize;
                        let y = data.pos.1 as usize;
                        let frame_width = data.image.width() as usize;
                        let logo_stride = frame_width * PIXEL_SIZE;
                        let row_start = (x + ((i + y) * FB_WIDTH)) * PIXEL_SIZE;
                        let row_end = row_start + logo_stride;

                        framebuffer[row_start..row_end].copy_from_slice(
                            &image[i * logo_stride..i * logo_stride + logo_stride],
                        );
                    }

                    yuv_helper::abgr_to_i420(
                        &framebuffer,
                        (FB_WIDTH * PIXEL_SIZE) as u32,
                        data_y,
                        stride_y,
                        data_u,
                        stride_u,
                        data_v,
                        stride_v,
                        FB_WIDTH as i32,
                        FB_HEIGHT as i32,
                    );

                    source.capture_frame(&*video_frame);
                }
            })
            .await
            .unwrap();
        }
    }
}

impl Drop for LogoTrack {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.close_tx.send(());
        }
    }
}
