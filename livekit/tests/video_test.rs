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
        test_rooms, test_rooms_with_options,
        video::{SolidColorParams, SolidColorTrack},
    },
    futures_util::StreamExt,
    libwebrtc::video_stream::native::NativeVideoStream,
    livekit::e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        E2eeOptions, EncryptionType,
    },
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

/// Room options with a shared key, or without encryption.
#[cfg(feature = "__lk-e2e-test")]
fn shared_key_options(encrypted: bool) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.encryption = encrypted.then(|| E2eeOptions {
        encryption_type: EncryptionType::Gcm,
        key_provider: KeyProvider::with_shared_key(
            KeyProviderOptions::default(),
            b"first-frame-test-key".to_vec(),
        ),
    });
    options
}

/// Waits for the first subscribed video track and returns its frames' average luma.
#[cfg(feature = "__lk-e2e-test")]
async fn first_frames_luma(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    frames: usize,
) -> Result<Vec<f64>> {
    let track = timeout(Duration::from_secs(15), async {
        loop {
            match events.recv().await {
                Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. }) => {
                    break Ok(track)
                }
                Some(_) => continue,
                None => break Err(anyhow!("Never received track")),
            }
        }
    })
    .await??;
    let mut stream = NativeVideoStream::new(track.rtc_track());
    timeout(Duration::from_secs(30), async {
        let mut lumas = Vec::with_capacity(frames);
        while lumas.len() < frames {
            let frame = stream.next().await.ok_or_else(|| anyhow!("Stream ended"))?;
            let i420 = frame.buffer.to_i420();
            let (data_y, _, _) = i420.data();
            lumas.push(data_y.iter().map(|&b| b as f64).sum::<f64>() / data_y.len() as f64);
        }
        Ok(lumas)
    })
    .await?
}

/// With a shared key, the subscriber decrypts the track from its first frame: the first
/// keyframe must not reach the decoder still encrypted.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee_shared_key_first_frame() -> Result<()> {
    const LUMA: u8 = 180;
    let mut rooms =
        test_rooms_with_options([shared_key_options(true).into(), shared_key_options(true).into()])
            .await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_sub_room, mut sub_events) = rooms.pop().unwrap();
    let mut track = SolidColorTrack::new(
        Arc::new(pub_room),
        SolidColorParams { width: 320, height: 240, luma: LUMA },
    );
    track.publish(VideoCodec::VP8, false).await?;

    for luma in first_frames_luma(&mut sub_events, 3).await? {
        assert!((luma - LUMA as f64).abs() < 30.0, "frame luma {luma:.1}, expected {LUMA}");
    }
    Ok(())
}

/// A track sent in the clear to a subscriber with a shared key stays readable when the
/// subscriber turns encryption off and on again.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee_shared_key_clear_track_across_toggle() -> Result<()> {
    const LUMA: u8 = 180;
    let mut rooms = test_rooms_with_options([
        shared_key_options(true).into(),
        shared_key_options(false).into(),
    ])
    .await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();
    let mut track = SolidColorTrack::new(
        Arc::new(pub_room),
        SolidColorParams { width: 320, height: 240, luma: LUMA },
    );
    track.publish(VideoCodec::VP8, false).await?;

    let first = first_frames_luma(&mut sub_events, 1).await?;
    assert!((first[0] - LUMA as f64).abs() < 30.0, "frame luma {:.1}", first[0]);

    sub_room.e2ee_manager().set_enabled(false);
    sub_room.e2ee_manager().set_enabled(true);
    let remote = sub_room
        .remote_participants()
        .values()
        .flat_map(|p| p.track_publications().into_values())
        .find_map(|publication| match publication.track() {
            Some(RemoteTrack::Video(track)) => Some(track),
            _ => None,
        })
        .ok_or_else(|| anyhow!("no remote video track"))?;
    let mut stream = NativeVideoStream::new(remote.rtc_track());
    // Frames already decoded may still be queued; one decoded after the toggle must arrive.
    let luma = timeout(Duration::from_secs(10), async {
        let mut last = 0.0;
        for _ in 0..10 {
            let frame = stream.next().await.ok_or_else(|| anyhow!("Stream ended"))?;
            let i420 = frame.buffer.to_i420();
            let (data_y, _, _) = i420.data();
            last = data_y.iter().map(|&b| b as f64).sum::<f64>() / data_y.len() as f64;
        }
        Ok(last)
    })
    .await??;
    assert!((luma - LUMA as f64).abs() < 30.0, "frame luma {luma:.1} after the toggle");
    Ok(())
}
