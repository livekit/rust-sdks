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
        key_provider::{KeyProvider, KeyProviderOptions, DEFAULT_RATCHET_WINDOW_SIZE},
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

/// Average luma of the next `frames` frames decoded from `track` after this call.
#[cfg(feature = "__lk-e2e-test")]
async fn next_frames_luma(track: &RemoteVideoTrack, frames: usize) -> Result<f64> {
    let mut stream = NativeVideoStream::new(track.rtc_track());
    timeout(Duration::from_secs(10), async {
        let mut sum = 0.0;
        for _ in 0..frames {
            let frame = stream.next().await.ok_or_else(|| anyhow!("Stream ended"))?;
            let i420 = frame.buffer.to_i420();
            let (data_y, _, _) = i420.data();
            sum += data_y.iter().map(|&b| b as f64).sum::<f64>() / data_y.len() as f64;
        }
        Ok(sum / frames as f64)
    })
    .await?
}

/// With a shared key, one publisher ratcheting its key moves only its own receiver's key on
/// the subscriber: another publisher still sending with the old key stays readable.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee_shared_key_ratchet_one_publisher() -> Result<()> {
    const LUMA_A: u8 = 60;
    const LUMA_B: u8 = 200;
    let mut rooms = test_rooms_with_options([
        shared_key_options(true).into(),
        shared_key_options(true).into(),
        shared_key_options(true).into(),
    ])
    .await?;
    let (pub_b, _) = rooms.pop().unwrap();
    let (pub_a, _) = rooms.pop().unwrap();
    let (_sub_room, mut sub_events) = rooms.pop().unwrap();
    let pub_a = Arc::new(pub_a);
    let identity_a = pub_a.local_participant().identity();
    let mut track_a = SolidColorTrack::new(
        pub_a.clone(),
        SolidColorParams { width: 320, height: 240, luma: LUMA_A },
    );
    let mut track_b = SolidColorTrack::new(
        Arc::new(pub_b),
        SolidColorParams { width: 320, height: 240, luma: LUMA_B },
    );
    track_a.publish(VideoCodec::VP8, false).await?;
    track_b.publish(VideoCodec::VP8, false).await?;

    let (mut remote_a, mut remote_b) = (None, None);
    timeout(Duration::from_secs(15), async {
        while remote_a.is_none() || remote_b.is_none() {
            match sub_events.recv().await {
                Some(RoomEvent::TrackSubscribed {
                    track: RemoteTrack::Video(track),
                    participant,
                    ..
                }) => {
                    if participant.identity() == identity_a {
                        remote_a = Some(track);
                    } else {
                        remote_b = Some(track);
                    }
                }
                Some(_) => continue,
                None => return Err(anyhow!("Never received both tracks")),
            }
        }
        Ok(())
    })
    .await??;
    let (remote_a, remote_b) = (remote_a.unwrap(), remote_b.unwrap());
    let luma = next_frames_luma(&remote_b, 3).await?;
    assert!((luma - LUMA_B as f64).abs() < 30.0, "B's luma {luma:.1} before the ratchet");

    pub_a
        .e2ee_manager()
        .key_provider()
        .and_then(|key_provider| key_provider.ratchet_shared_key(0))
        .ok_or_else(|| anyhow!("ratchet failed"))?;
    let luma = next_frames_luma(&remote_a, 30).await?;
    assert!((luma - LUMA_A as f64).abs() < 30.0, "A's luma {luma:.1} after its ratchet");
    let luma = next_frames_luma(&remote_b, 10).await?;
    assert!((luma - LUMA_B as f64).abs() < 30.0, "B's luma {luma:.1} after A's ratchet");
    Ok(())
}

/// The next subscribed video track.
#[cfg(feature = "__lk-e2e-test")]
async fn next_video_track(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
) -> Result<RemoteVideoTrack> {
    timeout(Duration::from_secs(15), async {
        loop {
            match events.recv().await {
                Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. }) => {
                    return Ok(track)
                }
                Some(_) => continue,
                None => return Err(anyhow!("Never received the track")),
            }
        }
    })
    .await?
}

/// Ratchets `pub_room`'s shared key once.
#[cfg(feature = "__lk-e2e-test")]
fn ratchet(pub_room: &Room) -> Result<()> {
    let key_provider = pub_room.e2ee_manager().key_provider().ok_or_else(|| anyhow!("no keys"))?;
    key_provider.ratchet_shared_key(0).ok_or_else(|| anyhow!("ratchet failed"))?;
    Ok(())
}

/// With a shared key, a new track from a sender who ratcheted past the ratchet window since the
/// subscriber first decrypted it is still readable: the ratchets one of its receivers followed
/// carry over, even when the receiver of its last subscribed track got no frames.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee_shared_key_new_track_after_ratchets() -> Result<()> {
    const LUMA_LIVE: u8 = 60;
    const LUMA_IDLE: u8 = 120;
    const LUMA_NEW: u8 = 200;
    let mut rooms =
        test_rooms_with_options([shared_key_options(true).into(), shared_key_options(true).into()])
            .await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_sub_room, mut sub_events) = rooms.pop().unwrap();
    let pub_room = Arc::new(pub_room);
    let publish = |luma| {
        let mut track = SolidColorTrack::new(
            pub_room.clone(),
            SolidColorParams { width: 320, height: 240, luma },
        );
        async move { track.publish(VideoCodec::VP8, false).await.map(|_| track) }
    };
    let _live = publish(LUMA_LIVE).await?;
    let remote_live = next_video_track(&mut sub_events).await?;
    let _idle = publish(LUMA_IDLE).await?;
    let idle_publication = timeout(Duration::from_secs(15), async {
        loop {
            match sub_events.recv().await {
                Some(RoomEvent::TrackSubscribed { publication, .. }) => return Ok(publication),
                Some(_) => continue,
                None => return Err(anyhow!("Never received the second track")),
            }
        }
    })
    .await??;
    // Still subscribed, but the server stops forwarding its frames.
    idle_publication.set_enabled(false);

    for step in 0..=DEFAULT_RATCHET_WINDOW_SIZE {
        ratchet(&pub_room)?;
        let luma = next_frames_luma(&remote_live, 5)
            .await
            .map_err(|err| anyhow!("no frames after ratchet {step}: {err}"))?;
        assert!((luma - LUMA_LIVE as f64).abs() < 30.0, "luma {luma:.1} after ratchet {step}");
    }

    let _new = publish(LUMA_NEW).await?;
    let remote_new = next_video_track(&mut sub_events).await?;
    let luma = next_frames_luma(&remote_new, 10)
        .await
        .map_err(|err| anyhow!("no frames of the new track: {err}"))?;
    assert!((luma - LUMA_NEW as f64).abs() < 30.0, "new track's luma {luma:.1}");
    Ok(())
}

/// Sends `byte` as a data message and waits for the subscriber to read it.
#[cfg(feature = "__lk-e2e-test")]
async fn send_and_read(
    pub_room: &Room,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    byte: u8,
) -> Result<()> {
    let packet = DataPacket { payload: vec![byte; 16], reliable: true, ..Default::default() };
    pub_room.local_participant().publish_data(packet).await?;
    timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await {
                Some(RoomEvent::DataReceived { payload, .. }) => {
                    assert_eq!(payload.to_vec(), vec![byte; 16]);
                    return Ok(());
                }
                Some(_) => continue,
                None => return Err(anyhow!("Never received message {byte}")),
            }
        }
    })
    .await?
}

/// With a shared key, ratchets the subscriber followed through a sender's data messages carry
/// over to its first track, and those followed through its track carry over to its data.
#[cfg(feature = "__lk-e2e-test")]
#[test_log::test(tokio::test)]
async fn test_e2ee_shared_key_ratchets_between_data_and_media() -> Result<()> {
    const LUMA: u8 = 200;
    let mut rooms =
        test_rooms_with_options([shared_key_options(true).into(), shared_key_options(true).into()])
            .await?;
    let (pub_room, _) = rooms.pop().unwrap();
    let (_sub_room, mut sub_events) = rooms.pop().unwrap();
    let pub_room = Arc::new(pub_room);

    for step in 0..=DEFAULT_RATCHET_WINDOW_SIZE {
        ratchet(&pub_room)?;
        send_and_read(&pub_room, &mut sub_events, step as u8).await?;
    }

    let mut track = SolidColorTrack::new(
        pub_room.clone(),
        SolidColorParams { width: 320, height: 240, luma: LUMA },
    );
    track.publish(VideoCodec::VP8, false).await?;
    let remote = next_video_track(&mut sub_events).await?;
    let luma = next_frames_luma(&remote, 10)
        .await
        .map_err(|err| anyhow!("no frames of the track: {err}"))?;
    assert!((luma - LUMA as f64).abs() < 30.0, "track's luma {luma:.1}");

    for step in 0..=DEFAULT_RATCHET_WINDOW_SIZE {
        ratchet(&pub_room)?;
        let luma = next_frames_luma(&remote, 5)
            .await
            .map_err(|err| anyhow!("no frames after ratchet {step}: {err}"))?;
        assert!((luma - LUMA as f64).abs() < 30.0, "luma {luma:.1} after ratchet {step}");
    }
    send_and_read(&pub_room, &mut sub_events, 255).await
}
