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

//! Pre-encoded H.264 ingest receiver.
//!
//! Subscribes to a LiveKit room and forwards the first incoming video track
//! as tightly-packed I420 frames over a TCP connection. A gstreamer
//! pipeline on the other end renders them.
//!
//! NOTE: the current SDK only exposes *decoded* frames on the receive side
//! (via `NativeVideoStream`). WebRTC's internal decoder runs in-process
//! before we hand the frame to the application. Encoded-frame receive is
//! a future enhancement — see README.md.

use std::{
    env,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use futures::StreamExt;
use livekit::{
    prelude::*,
    webrtc::{prelude::VideoBuffer, video_stream::native::NativeVideoStream},
};
use livekit_api::access_token;
use log::{info, warn};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit server URL (or set LIVEKIT_URL env var)
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (or set LIVEKIT_API_KEY env var)
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret (or set LIVEKIT_API_SECRET env var)
    #[arg(long)]
    api_secret: Option<String>,

    /// Room name to join
    #[arg(long, default_value = "pre-encoded-demo")]
    room: String,

    /// Participant identity
    #[arg(long, default_value = "encoded-receiver")]
    identity: String,

    /// TCP port to serve tightly-packed I420 frames on
    #[arg(long, default_value_t = 5001)]
    tcp_port: u16,

    /// Only subscribe to the track from this participant identity
    #[arg(long)]
    from: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn({
        let shutdown_tx = shutdown_tx.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = shutdown_tx.send(true);
            info!("Ctrl-C received, shutting down...");
        }
    });

    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .context("--url or LIVEKIT_URL required")?;
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .context("--api-key or LIVEKIT_API_KEY required")?;
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .context("--api-secret or LIVEKIT_API_SECRET required")?;

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room.clone(),
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.adaptive_stream = false;
    let (room, mut events) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} (sid {})", room.name(), room.sid().await);

    // Boot the frame server. Accepts one client at a time; subsequent
    // clients supersede the previous.
    let bind: SocketAddr = format!("0.0.0.0:{}", args.tcp_port).parse().unwrap();
    let listener = TcpListener::bind(bind).await.with_context(|| format!("bind tcp {bind}"))?;
    info!(
        "Serving tightly-packed I420 frames on tcp/{}:{} — waiting for a client",
        bind.ip(),
        bind.port()
    );

    // Channel feeding raw I420 frames to the TCP writer task. Kept small
    // so the most recent frame wins when the client stalls.
    let (frame_tx, frame_rx) = mpsc::channel::<I420Packet>(2);

    tokio::spawn(frame_server_task(listener, frame_rx, shutdown_rx.clone()));

    let mut active_sid: Option<TrackSid> = None;
    let frame_tx = Arc::new(frame_tx);
    let mut shutdown_rx_main = shutdown_rx.clone();

    loop {
        tokio::select! {
            biased;
            r = shutdown_rx_main.changed() => {
                r.ok();
                if *shutdown_rx_main.borrow() {
                    break;
                }
            }
            event = events.recv() => {
                let Some(event) = event else { break };
                match event {
                    RoomEvent::TrackSubscribed { track, publication, participant } => {
                        if let Some(ref from) = args.from {
                            if participant.identity().as_str() != from {
                                continue;
                            }
                        }
                        let RemoteTrack::Video(video) = track else { continue };
                        if active_sid.is_some() {
                            info!(
                                "Ignoring extra video track {} (already have one active)",
                                publication.sid()
                            );
                            continue;
                        }
                        let sid = publication.sid();
                        active_sid = Some(sid.clone());
                        info!(
                            "Subscribed to {} from '{}': codec={}, {}x{}",
                            sid,
                            participant.identity(),
                            publication.mime_type(),
                            publication.dimension().0,
                            publication.dimension().1,
                        );

                        let frame_tx = frame_tx.clone();
                        let mut shutdown_rx_video = shutdown_rx.clone();
                        tokio::spawn(async move {
                            let mut sink = NativeVideoStream::new(video.rtc_track());
                            let mut frames: u64 = 0;
                            let mut last_log = Instant::now();
                            loop {
                                tokio::select! {
                                    biased;
                                    r = shutdown_rx_video.changed() => {
                                        r.ok();
                                        if *shutdown_rx_video.borrow() {
                                            break;
                                        }
                                    }
                                    frame = sink.next() => {
                                        let Some(frame) = frame else {
                                            break;
                                        };
                                        let i420 = frame.buffer.to_i420();
                                        let w = i420.width();
                                        let h = i420.height();
                                        let (sy, su, sv) = i420.strides();
                                        let (dy, du, dv) = i420.data();
                                        let packet = pack_i420(w, h, sy, su, sv, dy, du, dv);
                                        // Non-blocking try_send: drop if the writer is slow.
                                        let _ = frame_tx.try_send(packet);
                                        frames += 1;
                                        if last_log.elapsed() >= Duration::from_secs(2) {
                                            info!(
                                                "recv: {}x{}, ~{:.1} fps",
                                                w,
                                                h,
                                                frames as f64 / last_log.elapsed().as_secs_f64()
                                            );
                                            frames = 0;
                                            last_log = Instant::now();
                                        }
                                    }
                                }
                            }
                            info!("frame sink ended");
                        });
                    }
                    RoomEvent::TrackUnsubscribed { publication, .. }
                    | RoomEvent::TrackUnpublished { publication, .. } => {
                        if active_sid.as_ref() == Some(&publication.sid()) {
                            info!("Track {} ended", publication.sid());
                            active_sid = None;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    if let Err(e) = room.close().await {
        warn!("room.close: {e}");
    }
    drop(frame_tx);

    info!("Shutting down...");
    Ok(())
}

/// A tightly-packed I420 frame ready to be written on the wire.
struct I420Packet {
    width: u32,
    height: u32,
    /// `width*height + 2*(width/2)*(height/2)` bytes (Y, U, V planes packed
    /// contiguously with no row padding).
    data: Vec<u8>,
}

fn pack_i420(
    width: u32,
    height: u32,
    stride_y: u32,
    stride_u: u32,
    stride_v: u32,
    y: &[u8],
    u: &[u8],
    v: &[u8],
) -> I420Packet {
    let uv_w = (width + 1) / 2;
    let uv_h = (height + 1) / 2;
    let y_size = (width * height) as usize;
    let uv_size = (uv_w * uv_h) as usize;
    let mut data = Vec::with_capacity(y_size + 2 * uv_size);

    for row in 0..height as usize {
        let off = row * stride_y as usize;
        data.extend_from_slice(&y[off..off + width as usize]);
    }
    for row in 0..uv_h as usize {
        let off = row * stride_u as usize;
        data.extend_from_slice(&u[off..off + uv_w as usize]);
    }
    for row in 0..uv_h as usize {
        let off = row * stride_v as usize;
        data.extend_from_slice(&v[off..off + uv_w as usize]);
    }

    I420Packet { width, height, data }
}

/// Accepts TCP clients and pumps frames from the channel into whichever
/// one is currently connected. Frames received while no client is
/// connected are dropped.
async fn frame_server_task(
    listener: TcpListener,
    mut frame_rx: mpsc::Receiver<I420Packet>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let frames_out = Arc::new(AtomicU64::new(0));
    let frames_dropped = Arc::new(AtomicU64::new(0));

    {
        let frames_out = frames_out.clone();
        let frames_dropped = frames_dropped.clone();
        let mut shutdown_rx_stats = shutdown_rx.clone();
        tokio::spawn(async move {
            let mut last = Instant::now();
            loop {
                tokio::select! {
                    biased;
                    r = shutdown_rx_stats.changed() => {
                        r.ok();
                        if *shutdown_rx_stats.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {
                        let ok = frames_out.swap(0, Ordering::Relaxed);
                        let dropped = frames_dropped.swap(0, Ordering::Relaxed);
                        if ok > 0 || dropped > 0 {
                            info!(
                                "serve: {:.1} fps written, {:.1} fps dropped",
                                ok as f64 / last.elapsed().as_secs_f64(),
                                dropped as f64 / last.elapsed().as_secs_f64()
                            );
                        }
                        last = Instant::now();
                    }
                }
            }
        });
    }

    loop {
        tokio::select! {
            biased;
            r = shutdown_rx.changed() => {
                r.ok();
                if *shutdown_rx.borrow() {
                    return;
                }
            }
            accept = listener.accept() => {
                let (client, peer) = match accept {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("accept failed: {e}");
                        continue;
                    }
                };
                info!("client connected from {peer}");
                if let Err(e) = pump_to_client(
                    client,
                    &mut frame_rx,
                    &frames_out,
                    &frames_dropped,
                    shutdown_rx.clone(),
                )
                .await
                {
                    warn!("client disconnected: {e}");
                }
                info!("client {peer} closed, waiting for the next one");
            }
        }
    }
}

async fn pump_to_client(
    mut client: TcpStream,
    frame_rx: &mut mpsc::Receiver<I420Packet>,
    frames_out: &AtomicU64,
    frames_dropped: &AtomicU64,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let _ = client.set_nodelay(true);
    let mut announced_dims = None;
    loop {
        tokio::select! {
            biased;
            r = shutdown_rx.changed() => {
                r.ok();
                if *shutdown_rx.borrow() {
                    return Ok(());
                }
            }
            maybe_frame = frame_rx.recv() => {
                let Some(frame) = maybe_frame else {
                    return Ok(());
                };
                if announced_dims.is_none() {
                    announced_dims = Some((frame.width, frame.height));
                    info!("first frame to client: {}x{}", frame.width, frame.height);
                }
                if announced_dims != Some((frame.width, frame.height)) {
                    // Resolution change: restart the client to let gstreamer
                    // reconfigure its pipeline. rawvideoparse has fixed caps.
                    frames_dropped.fetch_add(1, Ordering::Relaxed);
                    return Err(anyhow::anyhow!(
                        "resolution changed from {:?} to {}x{}; dropping client",
                        announced_dims,
                        frame.width,
                        frame.height
                    ));
                }
                client.write_all(&frame.data).await?;
                frames_out.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
