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

//! High-level helper that ingests a pre-encoded video bytestream over TCP
//! and publishes it to a LiveKit room as an encoded video track.
//!
//! The caller supplies the TCP endpoint, codec, and declared resolution.
//! The helper:
//!
//! 1. Creates a [`NativeEncodedVideoSource`] for the codec.
//! 2. Creates a [`LocalVideoTrack`] bound to that source.
//! 3. Publishes the track via `LocalParticipant::publish_track`.
//! 4. Connects to the TCP endpoint and reconnects on disconnect.
//! 5. Demuxes the stream (Annex-B for H.264/H.265, IVF for VP8/VP9/AV1).
//! 6. Pushes each demuxed frame through `capture_frame`.
//!
//! The matching gstreamer pipelines are documented in
//! `examples/pre_encoded_ingest/README.md`.

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use libwebrtc::video_source::{
    native::{EncodedVideoSourceObserver, NativeEncodedVideoSource},
    EncodedFrameInfo, RtcVideoSource, VideoCodec, VideoResolution,
};
use livekit_runtime::JoinHandle;
use parking_lot::Mutex;
use tokio::{io::AsyncReadExt, net::TcpStream, time::sleep};

use super::{demux::Demuxer, keyframe::is_keyframe};
use crate::{
    options::{TrackPublishOptions, VideoEncoding},
    participant::LocalParticipant,
    prelude::*,
    RoomError, RoomResult,
};

/// Configuration for [`EncodedTcpIngest::start`].
///
/// Only `port`, `codec`, `width`, and `height` are mandatory. Everything
/// else has a default that matches the reference gstreamer pipelines.
#[derive(Debug, Clone)]
pub struct EncodedTcpIngestOptions {
    /// Host running the gstreamer `tcpserversink`. Default: `127.0.0.1`.
    pub host: String,

    /// Port of the gstreamer `tcpserversink`.
    pub port: u16,

    /// Pre-encoded codec on the wire. Must match the upstream encoder.
    pub codec: VideoCodec,

    /// Declared stream width (px).
    pub width: u32,

    /// Declared stream height (px).
    pub height: u32,

    /// Optional track name. Default: `encoded-<codec>`.
    pub track_name: Option<String>,

    /// Track source classification. Default: [`TrackSource::Camera`].
    pub track_source: TrackSource,

    /// Optional target max bitrate (bps) forwarded to
    /// `TrackPublishOptions.video_encoding.max_bitrate`. When `None`, the
    /// SDK picks an appropriate default for the resolution.
    pub max_bitrate_bps: Option<u64>,

    /// Target max framerate forwarded when `max_bitrate_bps` is set.
    /// Ignored otherwise. Default: 30.0.
    pub max_framerate_fps: f64,

    /// Backoff between reconnection attempts. Default: 1 s.
    pub reconnect_backoff: Duration,

    /// When `true`, [`EncodedTcpIngest::stop`] unpublishes the track
    /// before returning. Default: `true`.
    pub unpublish_on_stop: bool,
}

impl EncodedTcpIngestOptions {
    /// New options with sensible defaults. Mandatory fields only.
    pub fn new(port: u16, codec: VideoCodec, width: u32, height: u32) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port,
            codec,
            width,
            height,
            track_name: None,
            track_source: TrackSource::Camera,
            max_bitrate_bps: None,
            max_framerate_fps: 30.0,
            reconnect_backoff: Duration::from_secs(1),
            unpublish_on_stop: true,
        }
    }
}

/// Callbacks dispatched by [`EncodedTcpIngest`] as the ingest loop runs.
///
/// All methods are invoked on Tokio / WebRTC threads; implementers MUST be
/// cheap and non-blocking. Default impls are no-ops so consumers can
/// override only what they care about.
pub trait EncodedIngestObserver: Send + Sync {
    /// The TCP connection to the upstream producer is established.
    fn on_connected(&self, _peer: SocketAddr) {}

    /// The TCP connection was closed (by peer, timeout, or demux desync).
    /// The ingest loop will reconnect after
    /// [`EncodedTcpIngestOptions::reconnect_backoff`].
    fn on_disconnected(&self, _reason: &str) {}

    /// The receiver requested a keyframe (PLI/FIR). Producers should emit
    /// a keyframe on the next frame.
    fn on_keyframe_requested(&self) {}

    /// The bandwidth estimator produced a new target bitrate / framerate.
    fn on_target_bitrate(&self, _bitrate_bps: u32, _framerate_fps: f64) {}
}

/// Snapshot of cumulative ingest stats. Counters are monotonic since
/// [`EncodedTcpIngest::start`].
#[derive(Debug, Clone, Copy, Default)]
pub struct EncodedIngestStats {
    /// Frames pushed to the source and accepted by WebRTC.
    pub frames_accepted: u64,
    /// Frames the source rejected because its internal queue was full.
    pub frames_dropped: u64,
    /// Keyframes observed on the wire (accepted + dropped).
    pub keyframes: u64,
    /// TCP reconnections attempted (including the first connect).
    pub tcp_reconnects: u64,
}

/// Ingests a pre-encoded video feed from a TCP socket and publishes it as
/// an encoded LiveKit track.
///
/// Create one with [`EncodedTcpIngest::start`], inspect it via
/// [`EncodedTcpIngest::stats`] / [`EncodedTcpIngest::track_sid`], and
/// shut it down with [`EncodedTcpIngest::stop`]. Dropping the value
/// without calling `stop` still terminates the background task, but does
/// not unpublish the track.
pub struct EncodedTcpIngest {
    inner: Arc<Inner>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

struct Inner {
    participant: LocalParticipant,
    source: NativeEncodedVideoSource,
    track: LocalVideoTrack,
    stop: AtomicBool,
    stats: Stats,
    observer: Mutex<Option<Arc<dyn EncodedIngestObserver>>>,
    options: EncodedTcpIngestOptions,
}

#[derive(Default)]
struct Stats {
    frames_accepted: AtomicU64,
    frames_dropped: AtomicU64,
    keyframes: AtomicU64,
    tcp_reconnects: AtomicU64,
}

impl EncodedTcpIngest {
    /// Creates the encoded source, publishes the track, and spawns the
    /// TCP ingest task. The returned value owns all of those.
    pub async fn start(
        participant: LocalParticipant,
        options: EncodedTcpIngestOptions,
    ) -> RoomResult<Self> {
        validate_options(&options)?;

        let resolution = VideoResolution { width: options.width, height: options.height };
        let source = NativeEncodedVideoSource::new(options.codec, resolution);
        log::info!(
            "EncodedTcpIngest: created {:?} source {}x{} (source_id={})",
            options.codec,
            options.width,
            options.height,
            source.source_id()
        );

        let track_name = options
            .track_name
            .clone()
            .unwrap_or_else(|| default_track_name(options.codec).to_string());
        let track = LocalVideoTrack::create_video_track(
            &track_name,
            RtcVideoSource::Encoded(source.clone()),
        );

        let publish_opts = build_publish_options(&options);
        // video_codec is force-pinned to match the encoded source by
        // LocalParticipant::publish_track, so we leave the default.

        participant.publish_track(LocalTrack::Video(track.clone()), publish_opts).await?;
        log::info!("EncodedTcpIngest: published track '{}' ({:?})", track_name, options.codec);

        let inner = Arc::new(Inner {
            participant,
            source: source.clone(),
            track,
            stop: AtomicBool::new(false),
            stats: Stats::default(),
            observer: Mutex::new(None),
            options,
        });

        source.set_observer(Arc::new(SourceObserverBridge { inner: Arc::downgrade(&inner) }));

        let join_handle = livekit_runtime::spawn({
            let inner = inner.clone();
            async move {
                run_ingest_loop(inner).await;
            }
        });

        Ok(Self { inner, join_handle: Mutex::new(Some(join_handle)) })
    }

    /// Register (or replace) the ingest-level observer.
    pub fn set_observer(&self, observer: Arc<dyn EncodedIngestObserver>) {
        *self.inner.observer.lock() = Some(observer);
    }

    /// Returns a snapshot of ingest stats since `start`.
    pub fn stats(&self) -> EncodedIngestStats {
        EncodedIngestStats {
            frames_accepted: self.inner.stats.frames_accepted.load(Ordering::Relaxed),
            frames_dropped: self.inner.stats.frames_dropped.load(Ordering::Relaxed),
            keyframes: self.inner.stats.keyframes.load(Ordering::Relaxed),
            tcp_reconnects: self.inner.stats.tcp_reconnects.load(Ordering::Relaxed),
        }
    }

    /// Returns the sid of the published track.
    pub fn track_sid(&self) -> TrackSid {
        self.inner.track.sid()
    }

    /// Returns a clone of the underlying track. Useful for hooking mute /
    /// packet-trailer state from the caller.
    pub fn track(&self) -> LocalVideoTrack {
        self.inner.track.clone()
    }

    /// Stops the ingest loop and, if configured, unpublishes the track.
    ///
    /// Safe to call at most once. After `stop` returns, the TCP task is
    /// terminated. If [`EncodedTcpIngestOptions::unpublish_on_stop`] is
    /// true (the default), the track is unpublished from the room.
    pub async fn stop(self) {
        self.inner.stop.store(true, Ordering::Release);

        let join = self.join_handle.lock().take();
        if let Some(handle) = join {
            // We don't care about join errors — the task can only panic
            // on a broken invariant, and we're shutting down anyway.
            let _ = handle.await;
        }

        if self.inner.options.unpublish_on_stop {
            let sid = self.inner.track.sid();
            match self.inner.participant.unpublish_track(&sid).await {
                Ok(_) => log::info!("EncodedTcpIngest: unpublished track {sid:?}"),
                Err(e) => log::warn!("EncodedTcpIngest: unpublish_track failed: {e}"),
            }
        }
    }
}

impl Drop for EncodedTcpIngest {
    fn drop(&mut self) {
        // Make sure the background task exits even if the caller forgot
        // to call `stop`. We can't await here, so the track stays
        // published until the room is dropped or explicitly unpublished.
        self.inner.stop.store(true, Ordering::Release);
    }
}

fn validate_options(options: &EncodedTcpIngestOptions) -> RoomResult<()> {
    if options.width == 0 || options.height == 0 {
        return Err(RoomError::Internal(
            "EncodedTcpIngest: width and height must be non-zero".to_string(),
        ));
    }
    if options.port == 0 {
        return Err(RoomError::Internal("EncodedTcpIngest: port must be non-zero".to_string()));
    }
    Ok(())
}

fn build_publish_options(options: &EncodedTcpIngestOptions) -> TrackPublishOptions {
    let mut publish_opts = TrackPublishOptions {
        source: options.track_source,
        simulcast: false,
        ..Default::default()
    };
    if let Some(max_bitrate) = options.max_bitrate_bps {
        publish_opts.video_encoding =
            Some(VideoEncoding { max_bitrate, max_framerate: options.max_framerate_fps });
    }
    publish_opts
}

fn default_track_name(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "encoded-h264",
        VideoCodec::H265 => "encoded-h265",
        VideoCodec::Vp8 => "encoded-vp8",
        VideoCodec::Vp9 => "encoded-vp9",
        VideoCodec::Av1 => "encoded-av1",
    }
}

/// Forwards source-level callbacks (keyframe request, bitrate update) to
/// the ingest-level observer, if any. Held via a `Weak` so the source
/// does not keep `Inner` alive past `drop`.
struct SourceObserverBridge {
    inner: std::sync::Weak<Inner>,
}

impl EncodedVideoSourceObserver for SourceObserverBridge {
    fn on_keyframe_requested(&self) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(obs) = inner.observer.lock().clone() {
                obs.on_keyframe_requested();
            }
        }
    }

    fn on_target_bitrate(&self, bitrate_bps: u32, framerate_fps: f64) {
        if let Some(inner) = self.inner.upgrade() {
            if let Some(obs) = inner.observer.lock().clone() {
                obs.on_target_bitrate(bitrate_bps, framerate_fps);
            }
        }
    }
}

/// Reconnect loop: connects, demuxes, captures, and reconnects on
/// disconnect / desync until `stop` is flipped.
async fn run_ingest_loop(inner: Arc<Inner>) {
    let opts = &inner.options;
    let addr = format!("{}:{}", opts.host, opts.port);

    while !inner.stop.load(Ordering::Acquire) {
        inner.stats.tcp_reconnects.fetch_add(1, Ordering::Relaxed);
        log::info!("EncodedTcpIngest: connecting to {addr} ({:?})", opts.codec);

        let mut stream = match TcpStream::connect(&addr).await {
            Ok(s) => s,
            Err(e) => {
                log::warn!("EncodedTcpIngest: connect {addr} failed: {e}");
                notify_disconnected(&inner, &format!("connect: {e}"));
                if !sleep_interruptible(&inner.stop, opts.reconnect_backoff).await {
                    return;
                }
                continue;
            }
        };
        let _ = stream.set_nodelay(true);

        let peer = stream.peer_addr().ok();
        if let Some(addr) = peer {
            log::info!("EncodedTcpIngest: connected to {addr}");
            if let Some(obs) = inner.observer.lock().clone() {
                obs.on_connected(addr);
            }
        } else {
            log::info!("EncodedTcpIngest: connected to {addr} (peer_addr unknown)");
        }

        let reason = pump_stream(&inner, &mut stream).await;
        log::warn!("EncodedTcpIngest: disconnected: {reason}");
        notify_disconnected(&inner, &reason);

        if inner.stop.load(Ordering::Acquire) {
            return;
        }
        if !sleep_interruptible(&inner.stop, opts.reconnect_backoff).await {
            return;
        }
    }
}

/// Reads from the socket, demuxes, and captures frames until EOF, error,
/// desync, or stop. Returns a human-readable disconnect reason.
async fn pump_stream(inner: &Arc<Inner>, stream: &mut TcpStream) -> String {
    let opts = &inner.options;
    let mut demuxer = Demuxer::new(opts.codec);
    let mut read_buf = vec![0u8; 64 * 1024];
    let mut out: Vec<Vec<u8>> = Vec::new();

    loop {
        if inner.stop.load(Ordering::Acquire) {
            return "stopped".to_string();
        }

        let n = tokio::select! {
            r = stream.read(&mut read_buf) => r,
            _ = sleep(Duration::from_millis(250)) => continue,
        };

        let n = match n {
            Ok(0) => return "peer closed connection".to_string(),
            Ok(n) => n,
            Err(e) => return format!("read error: {e}"),
        };

        out.clear();
        demuxer.feed(&read_buf[..n], &mut out);
        if demuxer.desynced() {
            return "demuxer desync (reconnecting to re-align)".to_string();
        }
        for frame in out.drain(..) {
            let is_keyframe = is_keyframe(opts.codec, &frame);
            if is_keyframe {
                inner.stats.keyframes.fetch_add(1, Ordering::Relaxed);
            }
            let info = EncodedFrameInfo {
                is_keyframe,
                // The source scans + prepends SPS/PPS as needed.
                has_sps_pps: false,
                width: opts.width,
                height: opts.height,
                capture_time_us: 0,
            };
            if inner.source.capture_frame(&frame, &info) {
                inner.stats.frames_accepted.fetch_add(1, Ordering::Relaxed);
            } else {
                inner.stats.frames_dropped.fetch_add(1, Ordering::Relaxed);
                log::warn!(
                    "EncodedTcpIngest: capture_frame dropped frame ({} bytes, keyframe={})",
                    frame.len(),
                    is_keyframe
                );
            }
        }
    }
}

fn notify_disconnected(inner: &Arc<Inner>, reason: &str) {
    if let Some(obs) = inner.observer.lock().clone() {
        obs.on_disconnected(reason);
    }
}

/// Sleeps up to `dur`, waking early when `stop` is set. Returns `false`
/// if the sleep was interrupted by a stop request.
async fn sleep_interruptible(stop: &AtomicBool, dur: Duration) -> bool {
    let tick = Duration::from_millis(100);
    let mut remaining = dur;
    while remaining > Duration::ZERO {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        let step = remaining.min(tick);
        sleep(step).await;
        remaining = remaining.saturating_sub(step);
    }
    true
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    };

    use libwebrtc::video_source::VideoCodec;

    use super::*;
    use crate::{prelude::TrackSource, RoomError};

    #[test]
    fn options_new_sets_network_and_track_defaults() {
        let options = EncodedTcpIngestOptions::new(5004, VideoCodec::H264, 1920, 1080);

        assert_eq!(options.host, "127.0.0.1");
        assert_eq!(options.port, 5004);
        assert_eq!(options.codec, VideoCodec::H264);
        assert_eq!(options.width, 1920);
        assert_eq!(options.height, 1080);
        assert_eq!(options.track_name, None);
        assert_eq!(options.track_source, TrackSource::Camera);
        assert_eq!(options.max_bitrate_bps, None);
        assert_eq!(options.max_framerate_fps, 30.0);
        assert_eq!(options.reconnect_backoff, Duration::from_secs(1));
        assert!(options.unpublish_on_stop);
    }

    #[test]
    fn validate_options_rejects_invalid_dimensions_before_publish() {
        let mut options = EncodedTcpIngestOptions::new(5004, VideoCodec::Vp8, 0, 720);

        let err = validate_options(&options).expect_err("zero width should be rejected");
        assert!(
            matches!(err, RoomError::Internal(message) if message.contains("width and height"))
        );

        options.width = 1280;
        options.height = 0;
        let err = validate_options(&options).expect_err("zero height should be rejected");
        assert!(
            matches!(err, RoomError::Internal(message) if message.contains("width and height"))
        );
    }

    #[test]
    fn validate_options_rejects_zero_port_before_publish() {
        let options = EncodedTcpIngestOptions::new(0, VideoCodec::Av1, 1280, 720);

        let err = validate_options(&options).expect_err("zero port should be rejected");
        assert!(matches!(err, RoomError::Internal(message) if message.contains("port")));
    }

    #[test]
    fn build_publish_options_disables_simulcast_and_preserves_source() {
        let mut options = EncodedTcpIngestOptions::new(5004, VideoCodec::H265, 1280, 720);
        options.track_source = TrackSource::Screenshare;

        let publish_options = build_publish_options(&options);

        assert_eq!(publish_options.source, TrackSource::Screenshare);
        assert!(!publish_options.simulcast);
        assert!(publish_options.video_encoding.is_none());
    }

    #[test]
    fn build_publish_options_uses_explicit_bitrate_pair() {
        let mut options = EncodedTcpIngestOptions::new(5004, VideoCodec::Vp9, 1280, 720);
        options.max_bitrate_bps = Some(2_500_000);
        options.max_framerate_fps = 24.0;

        let publish_options = build_publish_options(&options);
        let encoding = publish_options.video_encoding.expect("encoding should be set");

        assert_eq!(encoding.max_bitrate, 2_500_000);
        assert_eq!(encoding.max_framerate, 24.0);
        assert!(!publish_options.simulcast);
    }

    #[test]
    fn default_track_names_cover_all_ingest_codecs() {
        assert_eq!(default_track_name(VideoCodec::H264), "encoded-h264");
        assert_eq!(default_track_name(VideoCodec::H265), "encoded-h265");
        assert_eq!(default_track_name(VideoCodec::Vp8), "encoded-vp8");
        assert_eq!(default_track_name(VideoCodec::Vp9), "encoded-vp9");
        assert_eq!(default_track_name(VideoCodec::Av1), "encoded-av1");
    }

    #[tokio::test]
    async fn sleep_interruptible_returns_false_when_stop_already_set() {
        let stop = AtomicBool::new(true);

        assert!(!sleep_interruptible(&stop, Duration::from_secs(60)).await);
    }

    #[tokio::test]
    async fn sleep_interruptible_wakes_soon_after_stop_is_set() {
        let stop = Arc::new(AtomicBool::new(false));
        let setter = {
            let stop = stop.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                stop.store(true, Ordering::Release);
            })
        };

        let start = Instant::now();
        let slept = sleep_interruptible(&stop, Duration::from_secs(5)).await;
        setter.await.expect("stop setter should complete");

        assert!(!slept);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "sleep should be interrupted instead of waiting for the full backoff"
        );
    }
}
