// Live-streaming load test.
//
// A publisher publishes a simulcast H.264 video track (a generated, scrolling
// SMPTE color-bar test pattern, so no camera is required). Subscribers join the
// same room at random moments spread across a short window, mimicking an
// audience flooding into a live stream, and immediately request the highest
// simulcast layer.
//
// The publisher and subscribers can run as separate processes (subcommands
// `publisher` / `subscriber`), or together in one process (`all`).

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Args as ClapArgs, Parser, Subcommand};
use livekit::options::{
    self, video as video_presets, TrackPublishOptions, VideoCodec, VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
use livekit::track::VideoQuality;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{info, warn};
use rand::Rng;
use tokio_stream::StreamExt;

#[derive(Parser, Debug)]
#[command(about = "LiveKit live-streaming load test: simulcast H.264 publisher + many subscribers")]
struct Cli {
    #[command(flatten)]
    conn: ConnArgs,
    #[command(subcommand)]
    command: Command,
}

/// Connection details shared by every subcommand (also accepted after the
/// subcommand thanks to `global = true`).
#[derive(ClapArgs, Debug, Clone)]
struct ConnArgs {
    /// LiveKit server URL (falls back to LIVEKIT_URL)
    #[arg(long, global = true)]
    url: Option<String>,
    /// API key (falls back to LIVEKIT_API_KEY)
    #[arg(long, global = true)]
    api_key: Option<String>,
    /// API secret (falls back to LIVEKIT_API_SECRET)
    #[arg(long, global = true)]
    api_secret: Option<String>,
    /// Room to publish into / subscribe to
    #[arg(long, global = true, default_value = "live-streaming-loadtest")]
    room_name: String,
}

impl ConnArgs {
    fn resolve(&self) -> (String, String, String) {
        let url = self
            .url
            .clone()
            .or_else(|| std::env::var("LIVEKIT_URL").ok())
            .expect("LIVEKIT_URL must be provided via --url or env");
        let api_key = self
            .api_key
            .clone()
            .or_else(|| std::env::var("LIVEKIT_API_KEY").ok())
            .expect("LIVEKIT_API_KEY must be provided via --api-key or env");
        let api_secret = self
            .api_secret
            .clone()
            .or_else(|| std::env::var("LIVEKIT_API_SECRET").ok())
            .expect("LIVEKIT_API_SECRET must be provided via --api-secret or env");
        (url, api_key, api_secret)
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Publish a scrolling simulcast H.264 test-pattern track, then stay up
    Publisher(PublisherArgs),
    /// Join as N subscribers spread across the join window
    Subscriber(SubscriberArgs),
    /// Run the publisher and subscribers together in one process
    All {
        #[command(flatten)]
        publisher: PublisherArgs,
        #[command(flatten)]
        subscriber: SubscriberArgs,
    },
}

#[derive(ClapArgs, Debug, Clone)]
struct PublisherArgs {
    /// Participant identity for the publisher
    #[arg(long, default_value = "publisher")]
    identity: String,
    /// Publish width
    #[arg(long, default_value_t = 1280)]
    width: u32,
    /// Publish height
    #[arg(long, default_value_t = 720)]
    height: u32,
    /// Publish framerate
    #[arg(long, default_value_t = 30)]
    fps: u32,
}

#[derive(ClapArgs, Debug, Clone)]
struct SubscriberArgs {
    /// Number of subscribers to spawn
    #[arg(long, default_value_t = 40)]
    subscribers: usize,
    /// Window (seconds) over which subscribers randomly join
    #[arg(long, default_value_t = 4.0)]
    join_window: f64,
    /// How long (seconds) to keep subscribers connected after everyone joined
    #[arg(long, default_value_t = 15.0)]
    hold: f64,
    /// Identity prefix; each subscriber is `<prefix>-<index>`. Use distinct
    /// prefixes when running multiple subscriber processes against one room.
    #[arg(long, default_value = "sub")]
    identity_prefix: String,
}

fn token(
    api_key: &str,
    api_secret: &str,
    room: &str,
    identity: &str,
    publish: bool,
    subscribe: bool,
) -> Result<String> {
    Ok(access_token::AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room.to_string(),
            can_publish: publish,
            can_subscribe: subscribe,
            ..Default::default()
        })
        .to_jwt()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let (url, api_key, api_secret) = cli.conn.resolve();
    let room_name = cli.conn.room_name.clone();

    match cli.command {
        Command::Publisher(p) => {
            let _room = connect_publisher(&url, &api_key, &api_secret, &room_name, &p).await?;
            info!("[publisher] streaming; press Ctrl-C to stop");
            tokio::signal::ctrl_c().await?;
            info!("[publisher] shutting down");
        }
        Command::Subscriber(s) => {
            run_subscribers(&url, &api_key, &api_secret, &room_name, &s).await;
        }
        Command::All { publisher, subscriber } => {
            let _room =
                connect_publisher(&url, &api_key, &api_secret, &room_name, &publisher).await?;
            // Let the publisher start encoding before the audience floods in.
            tokio::time::sleep(Duration::from_millis(500)).await;
            run_subscribers(&url, &api_key, &api_secret, &room_name, &subscriber).await;
        }
    }
    Ok(())
}

/// Connect a publisher, publish a scrolling simulcast H.264 track, and start the
/// background capture loop. The returned `Room` must be kept alive to keep
/// streaming.
async fn connect_publisher(
    url: &str,
    api_key: &str,
    api_secret: &str,
    room_name: &str,
    p: &PublisherArgs,
) -> Result<Arc<Room>> {
    let tok = token(api_key, api_secret, room_name, &p.identity, true, false)?;
    let (room, _) = Room::connect(url, &tok, RoomOptions::default()).await?;
    let room = Arc::new(room);
    info!(
        "[publisher] connected to '{}' as '{}' ({}x{}@{}fps H.264 simulcast)",
        room.name(),
        p.identity,
        p.width,
        p.height,
        p.fps
    );

    let rtc_source =
        NativeVideoSource::new(VideoResolution { width: p.width, height: p.height }, false);
    let track =
        LocalVideoTrack::create_video_track("stream", RtcVideoSource::Native(rtc_source.clone()));

    let main_encoding = {
        let base = options::compute_appropriate_encoding(false, p.width, p.height, VideoCodec::H264);
        VideoEncoding { max_bitrate: base.max_bitrate, max_framerate: p.fps as f64 }
    };
    let simulcast_presets = simulcast_presets(p.width, p.height, p.fps as f64);
    info!(
        "[publisher] simulcast layers: {}",
        simulcast_presets
            .iter()
            .map(|preset| format!(
                "{}x{}@{:.0}fps/{}bps",
                preset.width, preset.height, preset.encoding.max_framerate, preset.encoding.max_bitrate
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let publish_opts = TrackPublishOptions {
        source: TrackSource::Camera,
        simulcast: true,
        video_codec: VideoCodec::H264,
        video_encoding: Some(main_encoding),
        simulcast_layers: Some(simulcast_presets),
        ..Default::default()
    };
    room.local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_opts)
        .await?;
    info!("[publisher] published simulcast H.264 track");

    // Drive frames from the scrolling test pattern in the background.
    tokio::spawn(capture_loop(rtc_source, p.width, p.height, p.fps));
    Ok(room)
}

/// Spawn `s.subscribers` subscribers, each joining at a random time within the
/// join window, then hold them connected and print a final tally.
async fn run_subscribers(
    url: &str,
    api_key: &str,
    api_secret: &str,
    room_name: &str,
    s: &SubscriberArgs,
) {
    info!(
        "[subscribers] spawning {} subscribers over {:.1}s (prefix '{}')",
        s.subscribers, s.join_window, s.identity_prefix
    );
    let subscribed = Arc::new(AtomicUsize::new(0));
    let first_frame = Arc::new(AtomicUsize::new(0));
    let test_start = Instant::now();

    let mut handles = Vec::with_capacity(s.subscribers);
    for i in 0..s.subscribers {
        // Each subscriber picks a random delay within the join window.
        let delay = Duration::from_secs_f64(rand::rng().random_range(0.0..s.join_window));
        let url = url.to_string();
        let api_key = api_key.to_string();
        let api_secret = api_secret.to_string();
        let room_name = room_name.to_string();
        let identity = format!("{}-{i:02}", s.identity_prefix);
        let subscribed = subscribed.clone();
        let first_frame = first_frame.clone();
        // Run each subscriber on its own OS thread with a dedicated
        // current-thread tokio runtime, isolating it from the others.
        let handle = std::thread::Builder::new()
            .name(identity.clone())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build subscriber runtime");
                rt.block_on(async move {
                    tokio::time::sleep(delay).await;
                    if let Err(e) = run_subscriber(
                        &url,
                        &api_key,
                        &api_secret,
                        &room_name,
                        &identity,
                        test_start,
                        delay,
                        subscribed,
                        first_frame,
                    )
                    .await
                    {
                        warn!("[{identity}] error: {e}");
                    }
                });
            })
            .expect("failed to spawn subscriber thread");
        handles.push(handle);
    }

    // Give everyone time to join, then hold connections open for a while so
    // frames keep flowing to all subscribers. The subscriber threads run their
    // event loops for the lifetime of the process.
    tokio::time::sleep(Duration::from_secs_f64(s.join_window)).await;
    info!(
        "All {} subscribers scheduled. {} subscribed to video, {} received a first frame. Holding {:.1}s...",
        s.subscribers,
        subscribed.load(Ordering::Relaxed),
        first_frame.load(Ordering::Relaxed),
        s.hold
    );
    tokio::time::sleep(Duration::from_secs_f64(s.hold)).await;

    info!(
        "Done. Final tally: {}/{} subscribed, {}/{} received frames.",
        subscribed.load(Ordering::Relaxed),
        s.subscribers,
        first_frame.load(Ordering::Relaxed),
        s.subscribers
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_subscriber(
    url: &str,
    api_key: &str,
    api_secret: &str,
    room_name: &str,
    identity: &str,
    test_start: Instant,
    scheduled_delay: Duration,
    subscribed: Arc<AtomicUsize>,
    first_frame: Arc<AtomicUsize>,
) -> Result<()> {
    let tok = token(api_key, api_secret, room_name, identity, false, true)?;
    let mut opts = RoomOptions::default();
    opts.auto_subscribe = true;
    opts.adaptive_stream = false;

    let connect_started = Instant::now();
    let (room, mut events) = Room::connect(url, &tok, opts).await?;
    info!(
        "[{identity}] joined at +{:.2}s (scheduled +{:.2}s), connect took {} ms",
        test_start.elapsed().as_secs_f64(),
        scheduled_delay.as_secs_f64(),
        connect_started.elapsed().as_millis()
    );

    // Keep the room alive for the lifetime of this task.
    let _room = room;

    while let Some(evt) = events.recv().await {
        if let RoomEvent::TrackSubscribed { track, publication, .. } = evt {
            if let RemoteTrack::Video(video_track) = track {
                subscribed.fetch_add(1, Ordering::Relaxed);
                // Immediately request the highest simulcast layer for this stream.
                publication.set_video_quality(VideoQuality::High);
                /*info!(
                    "[{identity}] subscribed to video at +{:.2}s, requested HIGH quality",
                    test_start.elapsed().as_secs_f64()
                );*/

                let first_frame = first_frame.clone();
                let identity = identity.to_string();
                tokio::spawn(async move {
                    let mut stream = NativeVideoStream::new(video_track.rtc_track());
                    if let Some(frame) = stream.next().await {
                        first_frame.fetch_add(1, Ordering::Relaxed);
                        let buf = frame.buffer;
                        info!(
                            "[{identity}] first frame at +{:.2}s ({}x{}), rtp_timestamp={} (capture_ts_us={})",
                            test_start.elapsed().as_secs_f64(),
                            buf.width(),
                            buf.height(),
                            frame.rtp_timestamp,
                            frame.timestamp_us,
                        );
                    }
                    // Keep draining so the subscription stays active.
                    while stream.next().await.is_some() {}
                });
            }
        }
    }
    Ok(())
}

/// SDK default simulcast presets for the publish resolution, normalized to a
/// uniform framerate.
fn simulcast_presets(width: u32, height: u32, fps: f64) -> Vec<VideoPreset> {
    let ar = width as f32 / height as f32;
    let defaults: &[VideoPreset] = if f32::abs(ar - 16.0 / 9.0) < f32::abs(ar - 4.0 / 3.0) {
        video_presets::DEFAULT_SIMULCAST_PRESETS
    } else {
        livekit::options::video43::DEFAULT_SIMULCAST_PRESETS
    };
    defaults
        .iter()
        .map(|p| VideoPreset::new(p.width, p.height, p.encoding.max_bitrate, fps))
        .collect()
}

/// Continuously push an SMPTE 75% color-bar test pattern into the video source
/// at the requested framerate.
async fn capture_loop(source: NativeVideoSource, width: u32, height: u32, fps: u32) {
    let pattern = TestPattern::new(width, height);
    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / fps as f64));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let start = Instant::now();
    // Scroll the bars one full screen width every 4 seconds.
    let scroll_px_per_frame = (width as f64 / (4.0 * fps as f64)).max(1.0);
    let mut frame_idx: u64 = 0;
    loop {
        ticker.tick().await;
        let mut frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: start.elapsed().as_micros() as i64,
            rtp_timestamp: 0,
            frame_metadata: None,
            buffer: I420Buffer::new(width, height),
        };
        // Even offset keeps luma/chroma column alignment for 4:2:0.
        let x_offset = (((frame_idx as f64 * scroll_px_per_frame) as u32) % width) & !1;
        let (sy, su, sv) = frame.buffer.strides();
        let (dy, du, dv) = frame.buffer.data_mut();
        pattern.render(dy, sy as i32, du, su as i32, dv, sv as i32, x_offset);
        source.capture_frame(&frame);
        frame_idx = frame_idx.wrapping_add(1);
    }
}

// --- SMPTE 75% color-bar test pattern (I420) --------------------------------

#[derive(Clone, Copy)]
struct I420Color {
    y: u8,
    u: u8,
    v: u8,
}

const BARS: [I420Color; 7] = [
    rgb_to_i420(191, 191, 191), // white
    rgb_to_i420(191, 191, 0),   // yellow
    rgb_to_i420(0, 191, 191),   // cyan
    rgb_to_i420(0, 191, 0),     // green
    rgb_to_i420(191, 0, 191),   // magenta
    rgb_to_i420(191, 0, 0),     // red
    rgb_to_i420(0, 0, 191),     // blue
];

struct TestPattern {
    width: usize,
    height: usize,
    chroma_width: usize,
    chroma_height: usize,
    y_plane: Vec<u8>,
    u_plane: Vec<u8>,
    v_plane: Vec<u8>,
}

impl TestPattern {
    fn new(width: u32, height: u32) -> Self {
        let width = width as usize;
        let height = height as usize;
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);
        let mut y_plane = vec![0; width * height];
        let mut u_plane = vec![128; chroma_width * chroma_height];
        let mut v_plane = vec![128; chroma_width * chroma_height];

        for row in 0..height {
            let row_start = row * width;
            for col in 0..width {
                y_plane[row_start + col] = color_for_column(col, width).y;
            }
        }
        for row in 0..chroma_height {
            let row_start = row * chroma_width;
            for col in 0..chroma_width {
                let color = color_for_column(col * 2, width);
                u_plane[row_start + col] = color.u;
                v_plane[row_start + col] = color.v;
            }
        }
        Self { width, height, chroma_width, chroma_height, y_plane, u_plane, v_plane }
    }

    /// Render the pattern, scrolling the bars left by `x_offset` luma pixels
    /// (with horizontal wraparound).
    fn render(
        &self,
        data_y: &mut [u8],
        stride_y: i32,
        data_u: &mut [u8],
        stride_u: i32,
        data_v: &mut [u8],
        stride_v: i32,
        x_offset: u32,
    ) {
        let y_off = (x_offset as usize) % self.width.max(1);
        let c_off = y_off / 2;
        copy_plane(data_y, stride_y as usize, &self.y_plane, self.width, self.height, y_off);
        copy_plane(
            data_u,
            stride_u as usize,
            &self.u_plane,
            self.chroma_width,
            self.chroma_height,
            c_off,
        );
        copy_plane(
            data_v,
            stride_v as usize,
            &self.v_plane,
            self.chroma_width,
            self.chroma_height,
            c_off,
        );
    }
}

const fn rgb_to_i420(r: u8, g: u8, b: u8) -> I420Color {
    let r = r as i32;
    let g = g as i32;
    let b = b as i32;
    I420Color {
        y: clamp_u8(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16),
        u: clamp_u8(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128),
        v: clamp_u8(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128),
    }
}

const fn clamp_u8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > u8::MAX as i32 {
        u8::MAX
    } else {
        value as u8
    }
}

fn color_for_column(col: usize, width: usize) -> I420Color {
    if width == 0 {
        return BARS[0];
    }
    let bar = (col * BARS.len()) / width;
    BARS[bar.min(BARS.len() - 1)]
}

/// Copy `src` into `dst`, rotating each row left by `x_offset` columns (the
/// columns scrolled off the left wrap around to the right).
fn copy_plane(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    width: usize,
    height: usize,
    x_offset: usize,
) {
    if width == 0 || height == 0 {
        return;
    }
    let off = x_offset % width;
    for row in 0..height {
        let dst_start = row * dst_stride;
        let src_start = row * width;
        let dst_row = &mut dst[dst_start..dst_start + width];
        let src_row = &src[src_start..src_start + width];
        // Left part: src[off..], right part wraps: src[..off].
        dst_row[..width - off].copy_from_slice(&src_row[off..]);
        dst_row[width - off..].copy_from_slice(&src_row[..off]);
    }
}
