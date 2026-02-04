// Example: Publishing pre-encoded H264 video to LiveKit
//
// This example demonstrates how to use EncodedVideoSource to publish
// pre-encoded H264 frames directly to LiveKit without re-encoding.

use libwebrtc::encoded_video_source::{EncodedFrameInfo, EncodedVideoSource, VideoCodecType};
use libwebrtc::video_source::RtcVideoSource;
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::prelude::*;
use livekit_api::access_token;
use log::info;
use std::env;
use std::fs::File;
use std::io::Read;
use std::time::Duration;
use tokio::time::interval;

const FPS: u32 = 30;
const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

/// H264 NAL unit parsed from Annex B stream
struct NalUnit {
    data: Vec<u8>,
    is_keyframe: bool,
    has_sps_pps: bool,
}

/// Parse H264 Annex B stream into NAL units
fn parse_h264_annexb(data: &[u8]) -> Vec<NalUnit> {
    let mut nals = Vec::new();
    let mut i = 0;

    // Find start codes and extract NAL units
    while i < data.len() {
        // Look for start code (0x00 0x00 0x01 or 0x00 0x00 0x00 0x01)
        let start = if i + 3 < data.len() && data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                Some(i + 3)
            } else if i + 4 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                Some(i + 4)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(nal_start) = start {
            // Find the end of this NAL (next start code or end of data)
            let mut nal_end = data.len();
            for j in nal_start..data.len() - 2 {
                if data[j] == 0 && data[j + 1] == 0 && (data[j + 2] == 1 || (j + 3 < data.len() && data[j + 2] == 0 && data[j + 3] == 1)) {
                    nal_end = j;
                    break;
                }
            }

            if nal_start < nal_end {
                let nal_data = &data[nal_start..nal_end];
                if !nal_data.is_empty() {
                    let nal_type = nal_data[0] & 0x1F;

                    // NAL unit types:
                    // 1 = Non-IDR slice
                    // 5 = IDR slice (keyframe)
                    // 6 = SEI
                    // 7 = SPS
                    // 8 = PPS
                    let is_keyframe = nal_type == 5;
                    let is_sps = nal_type == 7;
                    let is_pps = nal_type == 8;

                    // Include start code in the data for proper framing
                    let mut full_nal = vec![0, 0, 0, 1];
                    full_nal.extend_from_slice(nal_data);

                    nals.push(NalUnit {
                        data: full_nal,
                        is_keyframe,
                        has_sps_pps: is_sps || is_pps,
                    });
                }
            }
            i = nal_end;
        } else {
            i += 1;
        }
    }

    nals
}

/// Group NAL units into frames (combine SPS/PPS with following IDR)
fn group_into_frames(nals: Vec<NalUnit>) -> Vec<NalUnit> {
    let mut frames = Vec::new();
    let mut current_frame = Vec::new();
    let mut frame_has_sps_pps = false;

    for nal in nals {
        let nal_type = if nal.data.len() > 4 { nal.data[4] & 0x1F } else { 0 };

        // SPS, PPS, SEI get bundled with the next slice
        if nal_type == 7 || nal_type == 8 || nal_type == 6 {
            current_frame.extend_from_slice(&nal.data);
            frame_has_sps_pps = frame_has_sps_pps || nal.has_sps_pps;
            continue;
        }

        // Slice NAL (1 or 5) - this completes a frame
        if nal_type == 1 || nal_type == 5 {
            current_frame.extend_from_slice(&nal.data);

            frames.push(NalUnit {
                data: current_frame,
                is_keyframe: nal.is_keyframe,
                has_sps_pps: frame_has_sps_pps,
            });

            current_frame = Vec::new();
            frame_has_sps_pps = false;
        }
    }

    frames
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Get LiveKit credentials from environment
    let url = env::var("LIVEKIT_URL").expect("LIVEKIT_URL is not set");
    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY is not set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET is not set");

    // Read H264 file
    let h264_path = env::args().nth(1).unwrap_or_else(|| "test_video_raw.h264".to_string());
    info!("Reading H264 file: {}", h264_path);

    let mut file = File::open(&h264_path)?;
    let mut h264_data = Vec::new();
    file.read_to_end(&mut h264_data)?;
    info!("Read {} bytes", h264_data.len());

    // Parse into frames
    let nals = parse_h264_annexb(&h264_data);
    info!("Parsed {} NAL units", nals.len());

    let frames = group_into_frames(nals);
    info!("Grouped into {} frames", frames.len());

    if frames.is_empty() {
        return Err("No frames found in H264 file".into());
    }

    // Count keyframes
    let keyframe_count = frames.iter().filter(|f| f.is_keyframe).count();
    let delta_count = frames.len() - keyframe_count;
    info!("Found {} keyframes, {} delta frames", keyframe_count, delta_count);

    // Print first few frames to verify
    for (i, f) in frames.iter().take(10).enumerate() {
        info!("  Frame {}: keyframe={}, size={}", i, f.is_keyframe, f.data.len());
    }

    // Create access token
    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("rust-encoded-publisher")
        .with_name("Rust Encoded Publisher")
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "loltest".to_string(),
            can_publish: true,
            ..Default::default()
        })
        .to_jwt()?;

    // Connect to room
    info!("Connecting to LiveKit...");
    let (room, mut rx) = Room::connect(&url, &token, RoomOptions::default()).await?;
    info!("Connected to room: {}", room.name());

    // Spawn event handler
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            info!("Room event: {:?}", event);
        }
    });

    // Create encoded video source
    let encoded_source = EncodedVideoSource::new(WIDTH, HEIGHT, VideoCodecType::H264);
    info!("Created EncodedVideoSource: {}x{}", WIDTH, HEIGHT);

    // Create and publish video track
    let source = RtcVideoSource::Encoded(encoded_source.clone());
    let track = LocalVideoTrack::create_video_track("h264-video", source);

    let publish_options = TrackPublishOptions {
        source: TrackSource::Camera,
        video_codec: VideoCodec::H264,
        simulcast: false, // No simulcast for pre-encoded frames
        ..Default::default()
    };

    room.local_participant()
        .publish_track(LocalTrack::Video(track), publish_options)
        .await?;
    info!("Published video track");

    // Publish frames in a loop with proper timing using interval for accuracy
    let frame_duration = Duration::from_secs_f64(1.0 / FPS as f64);
    let mut frame_interval = interval(frame_duration);
    frame_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut frame_idx: u64 = 0;

    // Use frame-index-based timestamps for perfect consistency
    // The jitter buffer cares about consistent intervals, not wall-clock accuracy
    let rtp_increment: u32 = 90000 / FPS;  // 3000 ticks per frame at 30fps
    let capture_increment_us: i64 = 1_000_000 / FPS as i64;  // 33333 us per frame

    // Start with a large offset to avoid edge cases with 0
    let rtp_base: u32 = 90000;  // 1 second offset
    let capture_base_us: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64;

    info!("Starting to publish {} frames at {} fps (looping)", frames.len(), FPS);

    loop {
        frame_interval.tick().await;

        let frame_in_video = (frame_idx as usize) % frames.len();
        let frame = &frames[frame_in_video];

        // Use frame-index-based timestamps for perfectly consistent intervals
        // This avoids jitter from wall-clock measurement variations
        let rtp_timestamp = rtp_base.wrapping_add((frame_idx as u32).wrapping_mul(rtp_increment));
        let capture_time_us = capture_base_us + (frame_idx as i64 * capture_increment_us);

        let frame_info = EncodedFrameInfo {
            data: frame.data.clone(),
            capture_time_us,
            rtp_timestamp,
            width: WIDTH,
            height: HEIGHT,
            is_keyframe: frame.is_keyframe,
            has_sps_pps: frame.has_sps_pps,
        };

        encoded_source.capture_frame(&frame_info);

        // Log only keyframes to reduce overhead
        if frame.is_keyframe {
            info!(
                "Keyframe at frame {} (video {}), size: {} bytes",
                frame_idx,
                frame_in_video,
                frame.data.len()
            );
        }

        frame_idx += 1;
    }
}
