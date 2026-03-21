// Copyright 2026 LiveKit, Inc.
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

#[cfg(feature = "__lk-e2e-test")]
use {
    anyhow::{anyhow, Ok, Result},
    common::{
        test_rooms,
        video::{SolidColorParams, SolidColorTrack},
    },
    futures_util::StreamExt,
    libwebrtc::video_stream::native::NativeVideoStream,
    livekit::{options::VideoCodec, prelude::*},
    std::{sync::Arc, time::Duration},
    tokio::time::timeout,
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
struct VideoTestParams {
    codec: VideoCodec,
    width: u32,
    height: u32,
    simulcast: bool,
}

#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_video() -> Result<()> {
    let test_params = [
        VideoTestParams { codec: VideoCodec::VP8, width: 1280, height: 720, simulcast: false },
        VideoTestParams { codec: VideoCodec::VP8, width: 1280, height: 720, simulcast: true },
        VideoTestParams { codec: VideoCodec::VP9, width: 1280, height: 720, simulcast: false },
        VideoTestParams { codec: VideoCodec::VP9, width: 1280, height: 720, simulcast: true },
    ];
    for params in test_params {
        log::info!("Testing with {}", params);
        test_video_with(params).await?;
    }
    Ok(())
}

/// Tests video transfer between two participants.
///
/// Verifies that video can be published and received correctly by publishing
/// solid-color I420 frames and checking the average luminance on the subscriber end.
///
#[cfg(feature = "__lk-e2e-test")]
async fn test_video_with(params: VideoTestParams) -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_, mut sub_room_events) = rooms.pop().unwrap();

    const EXPECTED_LUMA: u8 = 180;
    const LUMA_TOLERANCE: f64 = 30.0;
    const FRAMES_TO_ANALYZE: usize = 1;

    let solid_params =
        SolidColorParams { width: params.width, height: params.height, luma: EXPECTED_LUMA };
    let mut solid_track = SolidColorTrack::new(Arc::new(pub_room), solid_params);
    solid_track.publish(params.codec, params.simulcast).await?;

    let track: RemoteTrack = timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = sub_room_events.recv().await else {
                Err(anyhow!("Never received track"))?
            };
            let RoomEvent::TrackSubscribed { track, publication, .. } = event else {
                continue;
            };
            break Ok(track.into());
        }
    })
    .await??;

    let RemoteTrack::Video(track) = track else { Err(anyhow!("Expected video track"))? };
    let mut stream = NativeVideoStream::new(track.rtc_track());

    let receive_frames = async {
        let mut frames_analyzed = 0;

        while let Some(frame) = stream.next().await {
            log::info!("Received frame: {:?}", frame);

            let (width, height) = (frame.buffer.width(), frame.buffer.height());
            assert!(width > 0 && height > 0, "Frame has zero dimensions");

            let expected_ar = params.width as f64 / params.height as f64;
            let actual_ar = width as f64 / height as f64;
            assert!(
                (actual_ar - expected_ar).abs() < 0.1,
                "Aspect ratio mismatch: {}x{} ({:.3}) != expected {:.3}",
                width,
                height,
                actual_ar,
                expected_ar
            );

            let i420 = frame.buffer.to_i420();
            let (data_y, _, _) = i420.data();
            let avg_luma = data_y.iter().map(|&b| b as f64).sum::<f64>() / data_y.len() as f64;

            assert!(
                (avg_luma - EXPECTED_LUMA as f64).abs() < LUMA_TOLERANCE,
                "Average luma {:.1} not within {} of expected {}",
                avg_luma,
                LUMA_TOLERANCE,
                EXPECTED_LUMA
            );

            frames_analyzed += 1;
            if frames_analyzed >= FRAMES_TO_ANALYZE {
                break;
            }
        }
        assert_eq!(frames_analyzed, FRAMES_TO_ANALYZE);
    };

    timeout(Duration::from_secs(60), receive_frames).await?;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
impl std::fmt::Display for VideoTestParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{}, {}, simulcast={}",
            self.width,
            self.height,
            self.codec.as_str(),
            self.simulcast
        )
    }
}
