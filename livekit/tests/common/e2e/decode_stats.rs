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

use anyhow::{anyhow, Result};
use libwebrtc::{
    prelude::{I420Buffer, RtcVideoSource, VideoFrame, VideoResolution, VideoRotation},
    stats::{InboundRtpStats, OutboundRtpStats, RtcStats},
    video_source::native::NativeVideoSource,
};
use livekit::{
    options::{TrackPublishOptions, VideoCodec},
    prelude::*,
};
use std::time::Duration;
use tokio::time::{self, timeout};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / 30);
const STREAM_PHASE: Duration = Duration::from_secs(4);
const CAPTURE_GAP: Duration = Duration::from_secs(2);
const SETTLE: Duration = Duration::from_millis(500);
const DECODE_SLACK_FRAMES: u64 = 2;
const MIN_DELIVERY_RATIO: f64 = 0.9;

/// Publishes a moving gradient to one subscriber with a capture gap in the middle, then returns
/// the publisher's outbound-rtp and the subscriber's inbound-rtp video stats.
pub async fn stream_with_capture_gap() -> Result<(OutboundRtpStats, InboundRtpStats)> {
    let mut rooms = super::test_rooms(2).await?;
    let (_sub_room, mut sub_events) = rooms.pop().expect("two rooms");
    let (pub_room, _) = rooms.pop().expect("two rooms");

    let source = NativeVideoSource::new(VideoResolution { width: WIDTH, height: HEIGHT }, false);
    let local_track = LocalVideoTrack::create_video_track(
        "moving-gradient",
        RtcVideoSource::Native(source.clone()),
    );
    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Video(local_track.clone()),
            TrackPublishOptions {
                video_codec: VideoCodec::VP8,
                simulcast: false,
                ..Default::default()
            },
        )
        .await?;

    let remote_track = timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = sub_events.recv().await else {
                return Err(anyhow!("subscriber room closed before subscribing"));
            };
            if let RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. } = event {
                return Ok(track);
            }
        }
    })
    .await??;

    let mut frame_index = 0;
    capture_for(&source, STREAM_PHASE, &mut frame_index).await;
    time::sleep(CAPTURE_GAP).await;
    capture_for(&source, STREAM_PHASE, &mut frame_index).await;
    time::sleep(SETTLE).await;

    let outbound = first(local_track.get_stats().await?, "outbound-rtp", |stats| match stats {
        RtcStats::OutboundRtp(stats) => Some(stats),
        _ => None,
    })?;
    let inbound = first(remote_track.get_stats().await?, "inbound-rtp", |stats| match stats {
        RtcStats::InboundRtp(stats) => Some(stats),
        _ => None,
    })?;
    Ok((outbound, inbound))
}

fn first<T>(stats: Vec<RtcStats>, kind: &str, pick: fn(RtcStats) -> Option<T>) -> Result<T> {
    stats.into_iter().find_map(pick).ok_or_else(|| anyhow!("no {kind} stats reported"))
}

/// Asserts that the subscriber's inbound video stats follow what the publisher sent, including
/// the capture gap showing up as freeze or pause time.
pub fn assert_inbound_tracks_outbound(outbound: &OutboundRtpStats, inbound: &InboundRtpStats) {
    let outbound = &outbound.outbound;
    let inbound = &inbound.inbound;
    let decoder = &inbound.decoder_implementation;
    let sent = u64::from(outbound.frames_sent);
    let received = inbound.frames_received;
    let decoded = u64::from(inbound.frames_decoded);
    let dropped = inbound.frames_dropped;
    let frames = format!(
        "{decoder}: sent {sent}, received {received}, decoded {decoded}, dropped {dropped}"
    );
    assert!(
        decoded + DECODE_SLACK_FRAMES >= received,
        "{frames}: decoded lags received by more than {DECODE_SLACK_FRAMES}"
    );
    assert!(
        received as f64 >= sent as f64 * MIN_DELIVERY_RATIO,
        "{frames}: received is below {MIN_DELIVERY_RATIO} of sent"
    );

    let sent_size = (outbound.frame_width, outbound.frame_height);
    let received_size = (inbound.frame_width, inbound.frame_height);
    assert_eq!(received_size, sent_size, "{decoder}: received size differs from the sent layer");

    let (freezes, freeze_s) = (inbound.freeze_count, inbound.total_freeze_duration);
    let (pauses, pause_s) = (inbound.pause_count, inbound.total_pause_duration);
    assert!(
        freeze_s + pause_s >= CAPTURE_GAP.as_secs_f64() * 0.75,
        "{decoder}: {freezes} freezes ({freeze_s:.3}s) and {pauses} pauses ({pause_s:.3}s) do not \
         cover the {CAPTURE_GAP:?} capture gap"
    );
}

async fn capture_for(source: &NativeVideoSource, duration: Duration, frame_index: &mut usize) {
    let mut ticks = time::interval(FRAME_INTERVAL);
    let end = time::Instant::now() + duration;
    while time::Instant::now() < end {
        ticks.tick().await;
        let mut buffer = I420Buffer::new(WIDTH, HEIGHT);
        let (stride_y, _, _) = buffer.strides();
        let (data_y, data_u, data_v) = buffer.data_mut();
        for (row, line) in data_y.chunks_mut(stride_y as usize).enumerate() {
            for (col, luma) in line.iter_mut().enumerate() {
                *luma = ((row + col + *frame_index * 4) % 256) as u8;
            }
        }
        data_u.fill(128);
        data_v.fill(128);
        source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
        *frame_index += 1;
    }
}
