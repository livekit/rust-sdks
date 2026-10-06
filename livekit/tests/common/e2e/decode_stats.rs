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

use anyhow::{anyhow, ensure, Result};
use libwebrtc::{
    prelude::{I420Buffer, RtcVideoSource, VideoFrame, VideoResolution, VideoRotation},
    stats::{InboundRtpStats, RtcStats},
    video_source::native::NativeVideoSource,
};
use livekit::{
    options::{TrackPublishOptions, VideoCodec},
    prelude::*,
};
use std::time::Duration;
use tokio::{
    sync::mpsc::UnboundedReceiver,
    time::{self, timeout},
};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / 30);
const LATE_SUBSCRIBE: Duration = Duration::from_secs(1);
const FIRST_PHASE: Duration = Duration::from_secs(5);
const CAPTURE_GAP: Duration = Duration::from_secs(2);
const RESUME_PHASE: Duration = Duration::from_secs(3);
const SETTLE: Duration = Duration::from_millis(500);
const STATS_POLL: Duration = Duration::from_millis(20);
const DECODE_SLACK_FRAMES: u64 = 2;
const MIN_DELIVERY_RATIO: f64 = 0.9;

pub async fn run_codec_matrix(decoder_ok: fn(&str) -> bool) -> Result<()> {
    let mut failures = Vec::new();
    for codec in [VideoCodec::VP8, VideoCodec::H264, VideoCodec::VP9, VideoCodec::AV1] {
        for layered in [true, false] {
            let scenario = format!("{} layered={layered}", codec.as_str());
            let reports = match stream_with_capture_gap(codec, layered).await {
                Ok(reports) => reports,
                Err(err) => {
                    failures.push(format!("{scenario}: {err:#}"));
                    continue;
                }
            };
            for (subscriber, report) in reports {
                log::info!("{scenario} {subscriber}: {report:?}");
                failures.extend(
                    report
                        .violations(codec, decoder_ok)
                        .into_iter()
                        .map(|violation| format!("{scenario} {subscriber}: {violation}")),
                );
            }
        }
    }
    ensure!(
        failures.is_empty(),
        "{} codec matrix failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct Counts {
    captured: u64,
    received: u64,
}

#[derive(Debug)]
struct DecodeReport {
    mime: Option<String>,
    decoder: String,
    reached_top_layer: Option<Counts>,
    before_gap: Counts,
    decoded: u64,
    sent_size: (u32, u32),
    received_size: (u32, u32),
    freeze_s: f64,
    pause_s: f64,
}

impl DecodeReport {
    fn violations(&self, codec: VideoCodec, decoder_ok: fn(&str) -> bool) -> Vec<String> {
        let mut violations = Vec::new();
        let requested = format!("video/{}", codec.as_str());
        match &self.mime {
            None => violations.push("no codec stats".into()),
            Some(mime) if !mime.eq_ignore_ascii_case(&requested) => {
                violations.push(format!("received {mime} instead of {requested}"))
            }
            Some(_) => {}
        }
        if !decoder_ok(&self.decoder) {
            violations.push(format!("unexpected decoder {:?}", self.decoder));
        }
        let received = self.before_gap.received;
        if self.decoded + DECODE_SLACK_FRAMES < received {
            violations.push(format!(
                "decoded {} lags received {received} by more than {DECODE_SLACK_FRAMES}",
                self.decoded
            ));
        }
        match self.reached_top_layer {
            None => violations
                .push(format!("never decoded a frame at the top layer {:?}", self.sent_size)),
            Some(from) => {
                let captured = self.before_gap.captured.saturating_sub(from.captured);
                let received = received.saturating_sub(from.received);
                if (received as f64) < captured as f64 * MIN_DELIVERY_RATIO {
                    violations.push(format!(
                        "received {received} of {captured} frames captured after the first \
                         top layer frame, below {MIN_DELIVERY_RATIO}"
                    ));
                }
            }
        }
        if self.received_size != self.sent_size {
            violations.push(format!(
                "received {:?} instead of the top layer {:?}",
                self.received_size, self.sent_size
            ));
        }
        if self.freeze_s + self.pause_s < CAPTURE_GAP.as_secs_f64() * 0.75 {
            violations.push(format!(
                "freezes {:.3}s and pauses {:.3}s do not cover the {CAPTURE_GAP:?} capture gap",
                self.freeze_s, self.pause_s
            ));
        }
        violations
    }
}

async fn stream_with_capture_gap(
    codec: VideoCodec,
    layered: bool,
) -> Result<[(&'static str, DecodeReport); 2]> {
    let mut late_options = RoomOptions::default();
    late_options.auto_subscribe = false;
    let mut rooms = super::test_rooms_with_options([
        super::TestRoomOptions::default(),
        super::TestRoomOptions::default(),
        late_options.into(),
    ])
    .await?;
    let (_late_room, mut late_events) = rooms.pop().expect("three rooms");
    let (_early_room, mut early_events) = rooms.pop().expect("three rooms");
    let (pub_room, _) = rooms.pop().expect("three rooms");

    let source = NativeVideoSource::new(VideoResolution { width: WIDTH, height: HEIGHT }, false);
    let local_track = LocalVideoTrack::create_video_track(
        "moving-gradient",
        RtcVideoSource::Native(source.clone()),
    );
    let svc = layered && matches!(codec, VideoCodec::VP9 | VideoCodec::AV1);
    pub_room
        .local_participant()
        .publish_track(
            LocalTrack::Video(local_track.clone()),
            TrackPublishOptions {
                video_codec: codec,
                simulcast: layered,
                scalability_mode: svc.then(|| "L3T3_KEY".to_string()),
                ..Default::default()
            },
        )
        .await?;
    let early = video_publication(&mut early_events).await?;
    let late = video_publication(&mut late_events).await?;

    let first_phase = async {
        let mut frame_index = 0;
        capture_for(&source, LATE_SUBSCRIBE, &mut frame_index).await;
        late.set_subscribed(true);
        capture_for(&source, FIRST_PHASE - LATE_SUBSCRIBE, &mut frame_index).await;
        frame_index
    };
    let (mut frame_index, early_top, late_top) = tokio::join!(
        first_phase,
        reach_top_layer(&early, &local_track),
        reach_top_layer(&late, &local_track)
    );

    // After a capture gap the server resumes from the lowest simulcast layer, so size and
    // delivery are read before it.
    let (captured, sent_size) = publisher_stats(&local_track).await?;
    let mut reports = [
        ("early", report(&early, early_top?, captured, sent_size).await?),
        ("late", report(&late, late_top?, captured, sent_size).await?),
    ];

    time::sleep(CAPTURE_GAP).await;
    capture_for(&source, RESUME_PHASE, &mut frame_index).await;
    time::sleep(SETTLE).await;
    for (publication, (_, report)) in [&early, &late].into_iter().zip(&mut reports) {
        let stats = subscriber_stats(publication).await?;
        let inbound = &inbound(&stats).ok_or_else(|| anyhow!("no inbound-rtp stats"))?.inbound;
        report.freeze_s = inbound.total_freeze_duration;
        report.pause_s = inbound.total_pause_duration;
    }
    Ok(reports)
}

async fn video_publication(
    events: &mut UnboundedReceiver<RoomEvent>,
) -> Result<RemoteTrackPublication> {
    timeout(Duration::from_secs(15), async {
        loop {
            let Some(event) = events.recv().await else {
                return Err(anyhow!("room closed before the track was published"));
            };
            if let RoomEvent::TrackPublished { publication, .. } = event {
                if publication.kind() == TrackKind::Video {
                    return Ok(publication);
                }
            }
        }
    })
    .await?
}

// Simulcast subscribers start on the lowest layer, which runs at a lower frame rate, so delivery
// counts from the first frame decoded at the top layer's size.
async fn reach_top_layer(
    publication: &RemoteTrackPublication,
    local_track: &LocalVideoTrack,
) -> Result<Option<Counts>> {
    let poll = async {
        loop {
            time::sleep(STATS_POLL).await;
            let stats = subscriber_stats(publication).await?;
            let Some(inbound) = inbound(&stats).map(|stats| &stats.inbound) else {
                continue;
            };
            if inbound.frames_decoded == 0 {
                continue;
            }
            let (captured, top_size) = publisher_stats(local_track).await?;
            if (inbound.frame_width, inbound.frame_height) == top_size {
                let received = inbound.frames_received;
                return Ok::<_, anyhow::Error>(Counts { captured, received });
            }
        }
    };
    match timeout(FIRST_PHASE, poll).await {
        Ok(reached) => reached.map(Some),
        Err(_) => Ok(None),
    }
}

async fn report(
    publication: &RemoteTrackPublication,
    reached_top_layer: Option<Counts>,
    captured: u64,
    sent_size: (u32, u32),
) -> Result<DecodeReport> {
    let stats = subscriber_stats(publication).await?;
    let inbound = inbound(&stats).ok_or_else(|| anyhow!("no inbound-rtp stats"))?;
    let mime = stats.iter().find_map(|stats| match stats {
        RtcStats::Codec(codec) if codec.rtc.id == inbound.stream.codec_id => {
            Some(codec.codec.mime_type.clone())
        }
        _ => None,
    });
    let inbound = &inbound.inbound;
    Ok(DecodeReport {
        mime,
        decoder: inbound.decoder_implementation.clone(),
        reached_top_layer,
        before_gap: Counts { captured, received: inbound.frames_received },
        decoded: u64::from(inbound.frames_decoded),
        sent_size,
        received_size: (inbound.frame_width, inbound.frame_height),
        freeze_s: 0.0,
        pause_s: 0.0,
    })
}

async fn subscriber_stats(publication: &RemoteTrackPublication) -> Result<Vec<RtcStats>> {
    let Some(RemoteTrack::Video(track)) = publication.track() else {
        return Ok(Vec::new());
    };
    Ok(track.get_stats().await?)
}

fn inbound(stats: &[RtcStats]) -> Option<&InboundRtpStats> {
    stats.iter().find_map(|stats| match stats {
        RtcStats::InboundRtp(stats) => Some(stats),
        _ => None,
    })
}

// Counts captured frames because an SVC encoding's frames_sent counts every spatial layer.
async fn publisher_stats(track: &LocalVideoTrack) -> Result<(u64, (u32, u32))> {
    let stats = track.get_stats().await?;
    let captured = stats
        .iter()
        .find_map(|stats| match stats {
            RtcStats::MediaSource(source) => Some(u64::from(source.video.frames)),
            _ => None,
        })
        .ok_or_else(|| anyhow!("no media-source stats"))?;
    let top_layer = stats
        .iter()
        .filter_map(|stats| match stats {
            RtcStats::OutboundRtp(stats) => Some(&stats.outbound),
            _ => None,
        })
        .max_by_key(|outbound| outbound.frame_height)
        .ok_or_else(|| anyhow!("no outbound-rtp stats"))?;
    Ok((captured, (top_layer.frame_width, top_layer.frame_height)))
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
