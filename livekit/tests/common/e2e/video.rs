// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use libwebrtc::{
    prelude::{I420Buffer, RtcVideoSource, VideoFrame, VideoResolution, VideoRotation},
    video_source::native::NativeVideoSource,
};
use livekit::{
    options::{TrackPublishOptions, VideoCodec},
    track::{LocalTrack, LocalVideoTrack},
    Room, RoomResult,
};
use std::sync::Arc;
use tokio::{sync::oneshot, task::JoinHandle, time};

/// Parameters for the solid-color frames generated with [`SolidColorTrack`].
#[derive(Clone, Debug)]
pub struct SolidColorParams {
    pub width: u32,
    pub height: u32,
    /// Y-plane value (0..255). U and V planes are fixed at 128 (neutral gray).
    pub luma: u8,
}

/// Video track which generates and publishes solid-color I420 frames.
///
/// Analogous to [`super::audio::SineTrack`] for audio.
pub struct SolidColorTrack {
    rtc_source: NativeVideoSource,
    params: SolidColorParams,
    room: Arc<Room>,
    handle: Option<TrackHandle>,
}

struct TrackHandle {
    close_tx: oneshot::Sender<()>,
    track: LocalVideoTrack,
    task: JoinHandle<()>,
}

impl SolidColorTrack {
    pub fn new(room: Arc<Room>, params: SolidColorParams) -> Self {
        Self {
            rtc_source: NativeVideoSource::new(
                VideoResolution { width: params.width, height: params.height },
                false,
            ),
            params,
            room,
            handle: None,
        }
    }

    pub async fn publish(&mut self, codec: VideoCodec, simulcast: bool) -> RoomResult<()> {
        let (close_tx, close_rx) = oneshot::channel();
        let track = LocalVideoTrack::create_video_track(
            "solid-color-track",
            RtcVideoSource::Native(self.rtc_source.clone()),
        );
        let task =
            tokio::spawn(Self::track_task(close_rx, self.rtc_source.clone(), self.params.clone()));
        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Video(track.clone()),
                TrackPublishOptions { video_codec: codec, simulcast, ..Default::default() },
            )
            .await?;
        let handle = TrackHandle { close_tx, track, task };
        self.handle = Some(handle);
        Ok(())
    }

    pub async fn unpublish(&mut self) -> RoomResult<()> {
        if let Some(handle) = self.handle.take() {
            handle.close_tx.send(()).ok();
            handle.task.await.ok();
            self.room.local_participant().unpublish_track(&handle.track.sid()).await?;
        }
        Ok(())
    }

    async fn track_task(
        mut close_rx: oneshot::Receiver<()>,
        rtc_source: NativeVideoSource,
        params: SolidColorParams,
    ) {
        let interval = std::time::Duration::from_millis(1000 / 5); // ~5 FPS
        loop {
            if close_rx.try_recv().is_ok() {
                break;
            }
            let mut buffer = I420Buffer::new(params.width, params.height);
            let (data_y, data_u, data_v) = buffer.data_mut();
            data_y.fill(params.luma);
            data_u.fill(128);
            data_v.fill(128);

            let frame =
                VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: 0, buffer };
            rtc_source.capture_frame(&frame);
            time::sleep(interval).await;
        }
    }
}
