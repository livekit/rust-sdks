// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0
//! Paired video handoff benchmark. Run with the SFU's test/moq/handoff_compare.py.
#![cfg(all(target_os = "linux", feature = "moq", feature = "__lk-e2e-test"))]

use anyhow::{bail, ensure, Context, Result};
use futures_util::StreamExt;
use libwebrtc::{
    prelude::{
        I420Buffer, RtcVideoSource, VideoBuffer, VideoFrame, VideoResolution, VideoRotation,
    },
    video_source::native::NativeVideoSource,
    video_stream::native::NativeVideoStream,
};
use livekit::{
    options::{TrackPublishOptions, VideoCodec, VideoEncoding},
    prelude::*,
};
use livekit_token::{AccessToken, VideoGrants};
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedReceiver;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FPS: u32 = 30;

fn checksum(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 == 0 { crc << 1 } else { (crc << 1) ^ 0x1021 };
        }
    }
    crc
}

// Twelve bytes in a 16 × 6 luma grid. Normalized coordinates survive encoder scaling.
fn frame(id: u32) -> VideoFrame<I420Buffer> {
    let mut buf = I420Buffer::new(WIDTH, HEIGHT);
    let stride = buf.strides().0 as usize;
    let (y, u, v) = buf.data_mut();
    u.fill(108);
    v.fill(148);
    // Moving bars in the remaining picture give both encoders inter-frame motion.
    for (row, line) in y.chunks_exact_mut(stride).take(HEIGHT as usize).enumerate() {
        for (col, block) in line[..WIDTH as usize].chunks_mut(16).enumerate() {
            block.fill(45 + (((col * 3 + row / 8 + id as usize * 2) % 32) * 5) as u8);
        }
    }
    VideoFrame::new(VideoRotation::VideoRotation0, buf)
}

fn stamp(f: &mut VideoFrame<I420Buffer>, id: u32, us: u32, origin: u64) {
    let stride = f.buffer.strides().0 as usize;
    let y = f.buffer.data_mut().0;
    let mut mark = [0u8; 12];
    mark[..2].copy_from_slice(b"LK");
    mark[2..6].copy_from_slice(&id.to_be_bytes());
    mark[6..10].copy_from_slice(&us.to_be_bytes());
    let crc = checksum(&mark[..10]);
    mark[10..].copy_from_slice(&crc.to_be_bytes());
    for bit in 0..96 {
        let col = bit % 16;
        let row = bit / 16;
        let level = if mark[bit / 8] & (128 >> (bit % 8)) == 0 { 24 } else { 232 };
        for line in (row * HEIGHT as usize / 24)..((row + 1) * HEIGHT as usize / 24) {
            y[line * stride + col * WIDTH as usize / 32
                ..line * stride + (col + 1) * WIDTH as usize / 32]
                .fill(level);
        }
    }
    f.timestamp_us = (origin + u64::from(us)) as i64;
}

fn watermark(buf: &I420Buffer) -> Option<(u32, u32)> {
    let (w, h) = (buf.width() as usize, buf.height() as usize);
    if w < 96 || h < 72 {
        return None;
    }
    let stride = buf.strides().0 as usize;
    let y = buf.data().0;
    let mut mark = [0u8; 12];
    for bit in 0..96 {
        let x = (2 * (bit % 16) + 1) * w / 64;
        let y0 = (2 * (bit / 16) + 1) * h / 48;
        let sum: u32 = (y0 - 1..=y0 + 1)
            .flat_map(|row| &y[row * stride + x - 1..=row * stride + x + 1])
            .map(|v| u32::from(*v))
            .sum();
        if sum > 9 * 128 {
            mark[bit / 8] |= 128 >> (bit % 8);
        }
    }
    if &mark[..2] != b"LK"
        || checksum(&mark[..10]) != u16::from_be_bytes(mark[10..].try_into().ok()?)
    {
        return None;
    }
    Some((
        u32::from_be_bytes(mark[2..6].try_into().ok()?),
        u32::from_be_bytes(mark[6..10].try_into().ok()?),
    ))
}

async fn join(
    room: &str,
    name: &str,
    transport: RoomTransport,
    signal: &str,
) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
    let viewer = name == "viewer";
    let token = AccessToken::with_api_key("devkey", "secret")
        .with_identity(name)
        .with_grants(VideoGrants {
            room_join: true,
            room: room.into(),
            can_publish: Some(!viewer),
            can_subscribe: Some(viewer),
            can_publish_data: Some(false),
            ..Default::default()
        })
        .to_jwt()?;
    let mut options = RoomOptions::default();
    options.transport = transport;
    options.auto_subscribe = viewer;
    Ok(Room::connect(signal, &token, options).await?)
}

async fn publish(room: &Room, source: &NativeVideoSource) -> Result<()> {
    let track =
        LocalVideoTrack::create_video_track("paired-video", RtcVideoSource::Native(source.clone()));
    room.local_participant()
        .publish_track(
            LocalTrack::Video(track),
            TrackPublishOptions {
                source: TrackSource::Camera,
                video_codec: VideoCodec::VP8,
                simulcast: false,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: 1_200_000,
                    max_framerate: FPS.into(),
                }),
                ..Default::default()
            },
        )
        .await?;
    Ok(())
}

#[derive(Default)]
struct View {
    sid: String,
    stream: Option<NativeVideoStream>,
    latest: Option<(u32, Instant)>,
    unique: usize,
}
impl View {
    fn received(
        &mut self,
        who: &str,
        f: Option<libwebrtc::video_frame::VideoFrame<libwebrtc::video_frame::BoxVideoBuffer>>,
        now: Instant,
        received_us: u64,
        out: &mut impl Write,
    ) -> Result<()> {
        let Some(f) = f else {
            self.stream = None;
            return Ok(());
        };
        if let Some((id, us)) = watermark(&f.buffer.to_i420()) {
            if self.latest.is_none_or(|(old, _)| id > old) {
                self.unique += 1;
                self.latest = Some((id, now));
            }
            writeln!(out, "{who},{},{id},{us},{received_us}", self.sid)?;
        } else {
            writeln!(out, "{who},{},,,{received_us}", self.sid)?;
        }
        Ok(())
    }

    fn warm(&self) -> bool {
        self.unique >= FPS as usize * 2
            && self.latest.is_some_and(|(_, at)| at.elapsed() < Duration::from_millis(300))
    }
}

async fn next(
    view: &mut View,
) -> Option<libwebrtc::video_frame::VideoFrame<libwebrtc::video_frame::BoxVideoBuffer>> {
    match &mut view.stream {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

fn lifecycle(role: &str, event: &RoomEvent, us: u64, out: &mut impl Write) -> Result<()> {
    let (kind, sid) = match event {
        RoomEvent::Reconnecting => ("reconnecting", String::new()),
        RoomEvent::Reconnected => ("reconnected", String::new()),
        RoomEvent::Disconnected { .. } => ("disconnected", String::new()),
        RoomEvent::LocalTrackPublished { publication, .. } => {
            ("published", publication.sid().to_string())
        }
        RoomEvent::LocalTrackRepublished { publication, .. } => {
            ("republished", publication.sid().to_string())
        }
        RoomEvent::TrackSubscribed { publication, .. } => {
            ("subscribed", publication.sid().to_string())
        }
        RoomEvent::TrackUnsubscribed { publication, .. } => {
            ("unsubscribed", publication.sid().to_string())
        }
        _ => return Ok(()),
    };
    writeln!(out, "{role},{kind},{sid},{us}")?;
    Ok(())
}

/// Shares CLOCK_MONOTONIC with Python across the guest's network namespaces.
fn monotonic_us() -> u64 {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
}

struct Fixture {
    dir: PathBuf,
    url: String,
    room: String,
}
impl Fixture {
    fn from_env() -> Result<Self> {
        Ok(Self {
            dir: env::var("LK_HANDOFF_OUTPUT")?.into(),
            url: env::var("LK_HANDOFF_URL")?,
            room: env::var("LK_HANDOFF_ROOM")?,
        })
    }
    fn csv(&self, name: &str, header: &str) -> Result<BufWriter<File>> {
        let mut out = BufWriter::new(File::create(self.dir.join(name))?);
        writeln!(out, "{header}")?;
        Ok(out)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "runs in the publisher network namespace"]
async fn paired_publishers() -> Result<()> {
    let fixture = Fixture::from_env()?;
    let ((rtc, mut re), (moq, mut me)) = tokio::try_join!(
        join(&fixture.room, "webrtc", RoomTransport::WebRtc, &fixture.url),
        join(&fixture.room, "moq", RoomTransport::Moq, &fixture.url)
    )?;
    let source = || NativeVideoSource::new(VideoResolution { width: WIDTH, height: HEIGHT }, false);
    let (rs, ms) = (source(), source());
    tokio::try_join!(publish(&rtc, &rs), publish(&moq, &ms))?;
    let mut captures = fixture
        .csv("source.csv", "id,relative_us,capture_monotonic_us,webrtc_accepted,moq_accepted")?;
    let mut events = fixture.csv("publishers.csv", "participant,event,track_sid,monotonic_us")?;
    let stop = fixture.dir.join("stop-capture");
    let origin = monotonic_us();
    let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(FPS)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut id = 0;
    let mut re_open = true;
    let mut me_open = true;
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if stop.exists() { break; }
                let mut f = frame(id);
                let us = u32::try_from(monotonic_us() - origin)?;
                stamp(&mut f, id, us, origin);
                let (ra, ma) = if id % 2 == 0 { (rs.capture_frame(&f), ms.capture_frame(&f)) }
                    else { let ma = ms.capture_frame(&f); (rs.capture_frame(&f), ma) };
                writeln!(captures, "{id},{us},{},{ra},{ma}", origin + u64::from(us))?;
                id += 1;
            }
            e = re.recv(), if re_open => if let Some(e) = e { lifecycle("webrtc",&e,monotonic_us(),&mut events)?; } else { re_open=false; },
            e = me.recv(), if me_open => if let Some(e) = e { lifecycle("moq",&e,monotonic_us(),&mut events)?; } else { me_open=false; },
        }
    }
    captures.flush()?;
    events.flush()?;
    // Keep publications alive while the viewer drains in-flight frames.
    while !fixture.dir.join("stop-viewer").exists() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = tokio::join!(rtc.close(), moq.close());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "runs in the stable viewer network namespace"]
async fn paired_viewer() -> Result<()> {
    let fixture = Fixture::from_env()?;
    let (room, mut ve) = join(&fixture.room, "viewer", RoomTransport::Moq, &fixture.url).await?;
    let mut decoded =
        fixture.csv("decoded.csv", "publisher,track_sid,id,watermark_us,received_monotonic_us")?;
    let mut events = fixture.csv("viewer.csv", "participant,event,track_sid,monotonic_us")?;
    let start = Instant::now();
    File::create(fixture.dir.join("viewer-ready"))?;
    let mut ready = false;
    let (mut rv, mut mv) = (View::default(), View::default());
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let stop = fixture.dir.join("stop-viewer");
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if stop.exists() { break; }
                if !ready && rv.warm() && mv.warm() && start.elapsed() >= Duration::from_secs(5) {
                    File::create(fixture.dir.join("video-ready"))?;
                    ready = true;
                }
            }
            f = next(&mut rv) => rv.received("webrtc",f,Instant::now(),monotonic_us(),&mut decoded)?,
            f = next(&mut mv) => mv.received("moq",f,Instant::now(),monotonic_us(),&mut decoded)?,
            e = ve.recv() => {
                let e = e.context("viewer event channel closed")?;
                lifecycle("viewer",&e,monotonic_us(),&mut events)?;
                match e {
                    RoomEvent::TrackSubscribed {track:RemoteTrack::Video(t),publication,participant} => {
                        ensure!(publication.mime_type().eq_ignore_ascii_case("video/vp8"), "unexpected codec");
                        let view = match participant.identity().as_str() {"webrtc" => &mut rv, "moq" => &mut mv, _ => continue};
                        view.sid = publication.sid().to_string();
                        view.stream = Some(NativeVideoStream::new(t.rtc_track()));
                    }
                    RoomEvent::TrackUnsubscribed {publication,..} => {
                        for v in [&mut rv,&mut mv] { if v.sid == publication.sid().as_str() { v.stream=None; } }
                    }
                    RoomEvent::Disconnected{..} | RoomEvent::Reconnecting => bail!("viewer connection interrupted"),
                    _=>{}
                }
            }
        }
    }
    decoded.flush()?;
    events.flush()?;
    let _ = room.close().await;
    Ok(())
}

#[test]
fn frame_id_and_timestamp_survive_watermark_readback() {
    let (id, us) = (0x1040_3070, 27_987_441);
    let mut f = frame(id);
    stamp(&mut f, id, us, 1_000_000_000);
    assert_eq!(watermark(&f.buffer), Some((id, us)));
    assert_eq!(f.timestamp_us, 1_000_000_000 + i64::from(us));
    // Invert the first ID bit. The magic stays intact but the CRC must fail.
    let stride = f.buffer.strides().0 as usize;
    let data = f.buffer.data_mut().0;
    for row in HEIGHT as usize / 24..HEIGHT as usize / 12 {
        data[row * stride..row * stride + WIDTH as usize / 32].fill(232);
    }
    assert_eq!(watermark(&f.buffer), None);
}
