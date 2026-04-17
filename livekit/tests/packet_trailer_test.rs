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

//! Packet Trailer E2E Tests
//!
//! These tests verify that user_timestamp and frame_id metadata survives the
//! full publish → SFU → subscribe WebRTC pipeline via the packet trailer
//! mechanism, both with and without E2EE.
//!
//! Run all tests (use --test-threads=1 to avoid local server flakiness):
//!   livekit-server --dev --node-ip 127.0.0.1
//!   cargo test -p livekit --features "default,__lk-e2e-test" --test packet_trailer_test -- --nocapture --test-threads=1

#![cfg(feature = "__lk-e2e-test")]

mod common;

use anyhow::{anyhow, Result};
use common::test_rooms_with_options;
use futures_util::StreamExt;
use libwebrtc::{
    audio_source::native::NativeAudioSource,
    audio_stream::native::NativeAudioStream,
    prelude::{
        AudioFrame, AudioSourceOptions, I420Buffer, RtcAudioSource, RtcVideoSource, VideoFrame,
        VideoResolution, VideoRotation,
    },
    video_source::native::NativeVideoSource,
    video_stream::native::NativeVideoStream,
};
use livekit::{
    e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        EncryptionType,
    },
    options::{PacketTrailerFeatures, TrackPublishOptions, VideoCodec},
    prelude::*,
    webrtc::video_frame::FrameMetadata,
    E2eeOptions, RoomOptions,
};
use std::{sync::Arc, time::Duration};
use tokio::{sync::oneshot, task::JoinHandle, time::timeout};

const FRAMES_TO_VALIDATE: usize = 5;
const TEST_WIDTH: u32 = 640;
const TEST_HEIGHT: u32 = 480;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
const AUDIO_CHANNELS: u32 = 1;

struct PacketTrailerTestParams {
    attach_timestamp: bool,
    attach_frame_id: bool,
    e2ee: bool,
    codec: VideoCodec,
}

// ==================== Test Functions ====================

#[test_log::test(tokio::test)]
async fn test_timestamp_only_vp8() -> Result<()> {
    run_packet_trailer_test(PacketTrailerTestParams {
        attach_timestamp: true,
        attach_frame_id: false,
        e2ee: false,
        codec: VideoCodec::VP8,
    })
    .await
}

#[test_log::test(tokio::test)]
async fn test_frame_id_only_vp8() -> Result<()> {
    run_packet_trailer_test(PacketTrailerTestParams {
        attach_timestamp: false,
        attach_frame_id: true,
        e2ee: false,
        codec: VideoCodec::VP8,
    })
    .await
}

#[test_log::test(tokio::test)]
async fn test_timestamp_and_frame_id_vp8() -> Result<()> {
    run_packet_trailer_test(PacketTrailerTestParams {
        attach_timestamp: true,
        attach_frame_id: true,
        e2ee: false,
        codec: VideoCodec::VP8,
    })
    .await
}

#[test_log::test(tokio::test)]
async fn test_timestamp_and_frame_id_vp8_e2ee() -> Result<()> {
    run_packet_trailer_test(PacketTrailerTestParams {
        attach_timestamp: true,
        attach_frame_id: true,
        e2ee: true,
        codec: VideoCodec::VP8,
    })
    .await
}

#[test_log::test(tokio::test)]
async fn test_audio_timestamp_only() -> Result<()> {
    run_audio_packet_trailer_test(true, false, false).await
}

#[test_log::test(tokio::test)]
async fn test_audio_frame_id_only() -> Result<()> {
    run_audio_packet_trailer_test(false, true, false).await
}

#[test_log::test(tokio::test)]
async fn test_audio_timestamp_and_frame_id() -> Result<()> {
    run_audio_packet_trailer_test(true, true, false).await
}

#[test_log::test(tokio::test)]
async fn test_audio_timestamp_and_frame_id_e2ee() -> Result<()> {
    run_audio_packet_trailer_test(true, true, true).await
}

// ==================== Implementation ====================

/// Publishes solid-color video frames with packet trailer metadata (user_timestamp
/// and/or frame_id) and verifies the subscriber receives matching metadata on the
/// decoded frames.
async fn run_packet_trailer_test(params: PacketTrailerTestParams) -> Result<()> {
    let make_room_options = |e2ee: bool| -> RoomOptions {
        let mut opts = RoomOptions::default();
        if e2ee {
            let key_provider = KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                b"packet-trailer-test-key".to_vec(),
            );
            opts.encryption =
                Some(E2eeOptions { key_provider, encryption_type: EncryptionType::Gcm });
        }
        opts
    };

    let mut rooms = test_rooms_with_options([
        make_room_options(params.e2ee).into(),
        make_room_options(params.e2ee).into(),
    ])
    .await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();

    if params.e2ee {
        pub_room.e2ee_manager().set_enabled(true);
        sub_room.e2ee_manager().set_enabled(true);
    }

    let pub_room = Arc::new(pub_room);

    let mut packet_trailer_features = PacketTrailerFeatures::default();
    packet_trailer_features.user_timestamp = params.attach_timestamp;
    packet_trailer_features.frame_id = params.attach_frame_id;

    let rtc_source =
        NativeVideoSource::new(VideoResolution { width: TEST_WIDTH, height: TEST_HEIGHT }, false);
    let track = LocalVideoTrack::create_video_track(
        "pt-test-track",
        RtcVideoSource::Native(rtc_source.clone()),
    );

    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Video(track.clone()),
            TrackPublishOptions {
                video_codec: params.codec,
                simulcast: false,
                packet_trailer_features,
                ..Default::default()
            },
        )
        .await?;

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let publish_task: JoinHandle<()> = tokio::spawn({
        let rtc_source = rtc_source.clone();
        let attach_ts = params.attach_timestamp;
        let attach_fid = params.attach_frame_id;
        async move {
            publish_frames(stop_rx, rtc_source, attach_ts, attach_fid).await;
        }
    });

    let remote_track: RemoteVideoTrack = timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed before receiving track"));
            };
            if let RoomEvent::TrackSubscribed { track, .. } = event {
                if let RemoteTrack::Video(video_track) = track.into() {
                    return Ok(video_track);
                }
            }
        }
    })
    .await??;

    {
        let mut stream = NativeVideoStream::new(remote_track.rtc_track());
        let attach_ts = params.attach_timestamp;
        let attach_fid = params.attach_frame_id;

        let validate = async {
            let mut validated = 0;
            let mut seen_timestamps: Vec<u64> = Vec::new();
            let mut seen_frame_ids: Vec<u32> = Vec::new();

            while let Some(frame) = stream.next().await {
                let Some(meta) = frame.frame_metadata else {
                    log::debug!("Frame without metadata, skipping (waiting for trailer pipeline)");
                    continue;
                };

                log::info!(
                    "Received frame with metadata: {:?} (validated {}/{})",
                    meta,
                    validated + 1,
                    FRAMES_TO_VALIDATE
                );

                if attach_ts {
                    let ts =
                        meta.user_timestamp.expect("Expected user_timestamp in frame metadata");
                    assert!(ts > 0, "user_timestamp should be a positive value, got {}", ts);
                    seen_timestamps.push(ts);
                }

                if attach_fid {
                    let fid = meta.frame_id.expect("Expected frame_id in frame metadata");
                    assert!(fid > 0, "frame_id should be a positive value, got {}", fid);
                    seen_frame_ids.push(fid);
                }

                validated += 1;
                if validated >= FRAMES_TO_VALIDATE {
                    break;
                }
            }

            assert_eq!(
                validated, FRAMES_TO_VALIDATE,
                "Expected {} frames with metadata, only received {}",
                FRAMES_TO_VALIDATE, validated
            );

            if attach_ts && seen_timestamps.len() >= 2 {
                for window in seen_timestamps.windows(2) {
                    assert!(
                        window[1] >= window[0],
                        "Timestamps should be monotonically non-decreasing: {} < {}",
                        window[1],
                        window[0]
                    );
                }
            }

            if attach_fid && seen_frame_ids.len() >= 2 {
                for window in seen_frame_ids.windows(2) {
                    assert!(
                        window[1] > window[0],
                        "Frame IDs should be strictly increasing: {} <= {}",
                        window[1],
                        window[0]
                    );
                }
            }
        };

        timeout(Duration::from_secs(60), validate).await?;
    }

    stop_tx.send(()).ok();
    publish_task.await?;

    pub_room.close().await.ok();
    sub_room.close().await.ok();

    Ok(())
}

async fn run_audio_packet_trailer_test(
    attach_timestamp: bool,
    attach_frame_id: bool,
    e2ee: bool,
) -> Result<()> {
    let make_room_options = |e2ee: bool| -> RoomOptions {
        let mut opts = RoomOptions::default();
        if e2ee {
            let key_provider = KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                b"packet-trailer-test-key".to_vec(),
            );
            opts.encryption =
                Some(E2eeOptions { key_provider, encryption_type: EncryptionType::Gcm });
        }
        opts
    };

    let mut rooms =
        test_rooms_with_options([make_room_options(e2ee).into(), make_room_options(e2ee).into()])
            .await?;

    let (pub_room, _) = rooms.pop().unwrap();
    let (sub_room, mut sub_events) = rooms.pop().unwrap();

    if e2ee {
        pub_room.e2ee_manager().set_enabled(true);
        sub_room.e2ee_manager().set_enabled(true);
    }

    let pub_room = Arc::new(pub_room);

    let mut packet_trailer_features = PacketTrailerFeatures::default();
    packet_trailer_features.user_timestamp = attach_timestamp;
    packet_trailer_features.frame_id = attach_frame_id;

    let rtc_source =
        NativeAudioSource::new(AudioSourceOptions::default(), AUDIO_SAMPLE_RATE, AUDIO_CHANNELS, 0);
    let track = LocalAudioTrack::create_audio_track(
        "pt-audio-track",
        RtcAudioSource::Native(rtc_source.clone()),
    );

    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track.clone()),
            TrackPublishOptions { packet_trailer_features, ..Default::default() },
        )
        .await?;

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let publish_task: JoinHandle<()> = tokio::spawn(async move {
        publish_audio_frames(stop_rx, rtc_source, attach_timestamp, attach_frame_id).await;
    });

    let remote_track: RemoteAudioTrack = timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("Event channel closed before receiving track"));
            };
            if let RoomEvent::TrackSubscribed { track, .. } = event {
                if let RemoteTrack::Audio(audio_track) = track.into() {
                    return Ok(audio_track);
                }
            }
        }
    })
    .await??;

    {
        let mut stream = NativeAudioStream::new(
            remote_track.rtc_track(),
            AUDIO_SAMPLE_RATE as i32,
            AUDIO_CHANNELS as i32,
        );

        let validate = async {
            let mut validated = 0;
            let mut seen_timestamps: Vec<u64> = Vec::new();
            let mut seen_frame_ids: Vec<u32> = Vec::new();
            let mut seen_rtp_timestamps: Vec<u32> = Vec::new();

            while let Some(frame) = stream.next().await {
                let Some(timestamp) = frame.timestamp else {
                    log::debug!("Audio frame without RTP timestamp, skipping");
                    continue;
                };
                seen_rtp_timestamps.push(timestamp.rtp_timestamp);

                let Some(meta) = frame.frame_metadata else {
                    log::debug!("Audio frame without metadata, skipping");
                    continue;
                };

                if attach_timestamp {
                    let ts =
                        meta.user_timestamp.expect("Expected user_timestamp in audio metadata");
                    assert!(ts > 0, "user_timestamp should be positive, got {}", ts);
                    seen_timestamps.push(ts);
                }

                if attach_frame_id {
                    let fid = meta.frame_id.expect("Expected frame_id in audio metadata");
                    assert!(fid > 0, "frame_id should be positive, got {}", fid);
                    seen_frame_ids.push(fid);
                }

                validated += 1;
                if validated >= FRAMES_TO_VALIDATE {
                    break;
                }
            }

            assert_eq!(validated, FRAMES_TO_VALIDATE);
            assert!(
                seen_rtp_timestamps.len() >= FRAMES_TO_VALIDATE,
                "expected {} frames with RTP timestamps, only received {}",
                FRAMES_TO_VALIDATE,
                seen_rtp_timestamps.len()
            );

            if attach_timestamp && seen_timestamps.len() >= 2 {
                for window in seen_timestamps.windows(2) {
                    assert!(window[1] >= window[0], "timestamps should be non-decreasing");
                }
            }

            if attach_frame_id && seen_frame_ids.len() >= 2 {
                for window in seen_frame_ids.windows(2) {
                    assert!(window[1] > window[0], "frame ids should be strictly increasing");
                }
            }

            if seen_rtp_timestamps.len() >= 2 {
                for window in seen_rtp_timestamps.windows(2) {
                    assert!(
                        window[1] > window[0],
                        "RTP timestamps should be strictly increasing: {} <= {}",
                        window[1],
                        window[0]
                    );
                }
            }
        };

        timeout(Duration::from_secs(60), validate).await?;
    }

    stop_tx.send(()).ok();
    publish_task.await?;

    pub_room.close().await.ok();
    sub_room.close().await.ok();

    Ok(())
}

/// Generates solid-color I420 frames with packet trailer metadata at ~5 fps.
async fn publish_frames(
    mut stop_rx: oneshot::Receiver<()>,
    rtc_source: NativeVideoSource,
    attach_timestamp: bool,
    attach_frame_id: bool,
) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let interval = Duration::from_millis(200); // 5 fps
    let mut frame_counter: u32 = 1;

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        let mut buffer = I420Buffer::new(TEST_WIDTH, TEST_HEIGHT);
        let (data_y, data_u, data_v) = buffer.data_mut();
        data_y.fill(128);
        data_u.fill(128);
        data_v.fill(128);

        let user_ts = if attach_timestamp {
            Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64)
        } else {
            None
        };

        let fid = if attach_frame_id {
            let id = frame_counter;
            frame_counter = frame_counter.wrapping_add(1);
            Some(id)
        } else {
            None
        };

        let frame_metadata = if user_ts.is_some() || fid.is_some() {
            Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
        } else {
            None
        };

        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            frame_metadata,
            buffer,
        };

        rtc_source.capture_frame(&frame);
        tokio::time::sleep(interval).await;
    }
}

async fn publish_audio_frames(
    mut stop_rx: oneshot::Receiver<()>,
    rtc_source: NativeAudioSource,
    attach_timestamp: bool,
    attach_frame_id: bool,
) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let samples_per_channel = AUDIO_SAMPLE_RATE / 100;
    let mut phase = 0f32;
    let mut frame_counter: u32 = 1;

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        let mut samples = vec![0i16; (samples_per_channel * AUDIO_CHANNELS) as usize];
        for sample in &mut samples {
            let value = (phase * std::f32::consts::TAU).sin() * 0.2;
            *sample = (value * i16::MAX as f32) as i16;
            phase += 440.0 / AUDIO_SAMPLE_RATE as f32;
            if phase >= 1.0 {
                phase -= 1.0;
            }
        }

        let user_timestamp = if attach_timestamp {
            Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros() as u64)
        } else {
            None
        };

        let frame_id = if attach_frame_id {
            let id = frame_counter;
            frame_counter = frame_counter.wrapping_add(1);
            Some(id)
        } else {
            None
        };

        let frame = AudioFrame {
            data: samples.into(),
            sample_rate: AUDIO_SAMPLE_RATE,
            num_channels: AUDIO_CHANNELS,
            samples_per_channel,
            timestamp: None,
            frame_metadata: Some(FrameMetadata { user_timestamp, frame_id }),
        };

        rtc_source.capture_frame(&frame).await.expect("audio frame capture should succeed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
