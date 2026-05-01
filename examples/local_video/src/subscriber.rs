use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Parser;
use eframe::egui;
use eframe::wgpu::{self, util::DeviceExt};
use egui_wgpu as egui_wgpu_backend;
use egui_wgpu_backend::CallbackTrait;
use futures::StreamExt;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{debug, info, warn};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

async fn wait_for_shutdown(flag: Arc<AtomicBool>) {
    while !flag.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit participant identity
    #[arg(long, default_value = "rust-video-subscriber")]
    identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")]
    room_name: String,

    /// LiveKit server URL
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key (can also be set via LIVEKIT_API_KEY environment variable)
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret (can also be set via LIVEKIT_API_SECRET environment variable)
    #[arg(long)]
    api_secret: Option<String>,

    /// Only subscribe to video from this participant identity
    #[arg(long)]
    participant: Option<String>,

    /// Display user timestamp, current timestamp, and latency overlay
    #[arg(long)]
    display_timestamp: bool,

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set; must match publisher's key)
    #[arg(long)]
    e2ee_key: Option<String>,
}

struct SharedYuv {
    width: u32,
    height: u32,
    y_bytes_per_row: u32,
    uv_bytes_per_row: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    codec: String,
    fps: f32,
    dirty: bool,
    /// Time when the latest frame became available to the subscriber code.
    received_at_us: Option<u64>,
    /// Packet-trailer metadata from the most recent frame, if any.
    frame_metadata: Option<livekit::webrtc::video_frame::FrameMetadata>,
    /// Whether the publisher advertised PTF_USER_TIMESTAMP in its track info.
    has_user_timestamp: bool,
    /// Latest frame whose GPU submit has completed; lags CPU receive by ~1 display frame.
    gpu_done: Option<GpuDoneSample>,
}

#[derive(Clone, Copy, Debug)]
struct GpuDoneSample {
    frame_id: Option<u32>,
    publish_us: Option<u64>,
    cpu_received_us: u64,
    gpu_done_us: u64,
}

/// Carried from upload into the wgpu submit callback to stamp `gpu_done_us`.
#[derive(Clone, Copy, Debug)]
struct PendingGpuSample {
    frame_id: Option<u32>,
    publish_us: Option<u64>,
    cpu_received_us: u64,
}

#[derive(Clone)]
struct SimulcastState {
    available: bool,
    publication: Option<RemoteTrackPublication>,
    requested_quality: Option<livekit::track::VideoQuality>,
    active_quality: Option<livekit::track::VideoQuality>,
    full_dims: Option<(u32, u32)>,
}

impl Default for SimulcastState {
    fn default() -> Self {
        Self {
            available: false,
            publication: None,
            requested_quality: None,
            active_quality: None,
            full_dims: None,
        }
    }
}

fn codec_label(mime: &str) -> String {
    let base = mime.split(';').next().unwrap_or(mime).trim();
    let last = base.rsplit('/').next().unwrap_or(base).trim();
    last.to_ascii_uppercase()
}

fn infer_quality_from_dims(
    full_w: u32,
    _full_h: u32,
    cur_w: u32,
    _cur_h: u32,
) -> livekit::track::VideoQuality {
    if full_w == 0 {
        return livekit::track::VideoQuality::High;
    }
    let ratio = cur_w as f32 / full_w as f32;
    if ratio >= 0.75 {
        livekit::track::VideoQuality::High
    } else if ratio >= 0.45 {
        livekit::track::VideoQuality::Medium
    } else {
        livekit::track::VideoQuality::Low
    }
}

fn find_video_inbound_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
) -> Option<livekit::webrtc::stats::InboundRtpStats> {
    stats.iter().find_map(|stat| match stat {
        livekit::webrtc::stats::RtcStats::InboundRtp(inbound) if inbound.stream.kind == "video" => {
            Some(inbound.clone())
        }
        _ => None,
    })
}

fn log_video_inbound_stats(stats: &[livekit::webrtc::stats::RtcStats]) {
    let mut codec_by_id: HashMap<String, (String, String)> = HashMap::new();
    for stat in stats {
        if let livekit::webrtc::stats::RtcStats::Codec(codec) = stat {
            codec_by_id.insert(
                codec.rtc.id.clone(),
                (codec.codec.mime_type.clone(), codec.codec.sdp_fmtp_line.clone()),
            );
        }
    }

    if let Some(inbound) = find_video_inbound_stats(stats) {
        if let Some((mime, fmtp)) = codec_by_id.get(&inbound.stream.codec_id) {
            info!("Inbound codec: {} (fmtp: {})", mime, fmtp);
        } else {
            info!("Inbound codec id: {}", inbound.stream.codec_id);
        }
        info!(
            "Inbound current layer: {}x{} ~{:.1} fps, decoder: {}, power_efficient: {}",
            inbound.inbound.frame_width,
            inbound.inbound.frame_height,
            inbound.inbound.frames_per_second,
            inbound.inbound.decoder_implementation,
            inbound.inbound.power_efficient_decoder
        );
        info!(
            "Inbound RTP totals: packets={}, bytes={}, header_bytes={}, lost={}, jitter={:.3}s, frames recv/decoded/rendered/dropped={}/{}/{}/{}, keyframes={}, discarded_packets={}, nack/pli/fir={}/{}/{}",
            inbound.received.packets_received,
            inbound.inbound.bytes_received,
            inbound.inbound.header_bytes_received,
            inbound.received.packets_lost,
            inbound.received.jitter,
            inbound.inbound.frames_received,
            inbound.inbound.frames_decoded,
            inbound.inbound.frames_rendered,
            inbound.inbound.frames_dropped,
            inbound.inbound.key_frames_decoded,
            inbound.inbound.packets_discarded,
            inbound.inbound.nack_count,
            inbound.inbound.pli_count,
            inbound.inbound.fir_count,
        );
    }
}

#[derive(Debug, Clone)]
struct VideoInboundStatsSample {
    packets_received: u64,
    packets_lost: i64,
    bytes_received: u64,
    header_bytes_received: u64,
    frames_received: u64,
    frames_decoded: u32,
    key_frames_decoded: u32,
    frames_rendered: u32,
    frames_dropped: u32,
    packets_discarded: u64,
    nack_count: u32,
    pli_count: u32,
    fir_count: u32,
    total_decode_time: f64,
    total_processing_delay: f64,
    jitter_buffer_emitted_count: u64,
    frames_assembled_from_multiple_packets: u64,
}

impl From<&livekit::webrtc::stats::InboundRtpStats> for VideoInboundStatsSample {
    fn from(inbound: &livekit::webrtc::stats::InboundRtpStats) -> Self {
        Self {
            packets_received: inbound.received.packets_received,
            packets_lost: inbound.received.packets_lost,
            bytes_received: inbound.inbound.bytes_received,
            header_bytes_received: inbound.inbound.header_bytes_received,
            frames_received: inbound.inbound.frames_received,
            frames_decoded: inbound.inbound.frames_decoded,
            key_frames_decoded: inbound.inbound.key_frames_decoded,
            frames_rendered: inbound.inbound.frames_rendered,
            frames_dropped: inbound.inbound.frames_dropped,
            packets_discarded: inbound.inbound.packets_discarded,
            nack_count: inbound.inbound.nack_count,
            pli_count: inbound.inbound.pli_count,
            fir_count: inbound.inbound.fir_count,
            total_decode_time: inbound.inbound.total_decode_time,
            total_processing_delay: inbound.inbound.total_processing_delay,
            jitter_buffer_emitted_count: inbound.inbound.jitter_buffer_emitted_count,
            frames_assembled_from_multiple_packets: inbound
                .inbound
                .frames_assembled_from_multiple_packets,
        }
    }
}

fn log_video_inbound_stats_delta(
    inbound: &livekit::webrtc::stats::InboundRtpStats,
    current: &VideoInboundStatsSample,
    previous: Option<&VideoInboundStatsSample>,
    elapsed: Duration,
) {
    let Some(previous) = previous else {
        info!(
            "Inbound RTP sample: packets={}, bytes={}, frames recv/decoded/rendered={}/{}/{}",
            current.packets_received,
            current.bytes_received,
            current.frames_received,
            current.frames_decoded,
            current.frames_rendered,
        );
        return;
    };

    let packets_delta = current.packets_received.saturating_sub(previous.packets_received);
    let bytes_delta = current.bytes_received.saturating_sub(previous.bytes_received);
    let header_bytes_delta =
        current.header_bytes_received.saturating_sub(previous.header_bytes_received);
    let frames_received_delta = current.frames_received.saturating_sub(previous.frames_received);
    let frames_decoded_delta = current.frames_decoded.saturating_sub(previous.frames_decoded);
    let frames_rendered_delta = current.frames_rendered.saturating_sub(previous.frames_rendered);
    let frames_dropped_delta = current.frames_dropped.saturating_sub(previous.frames_dropped);
    let keyframes_delta = current.key_frames_decoded.saturating_sub(previous.key_frames_decoded);
    let discarded_delta = current.packets_discarded.saturating_sub(previous.packets_discarded);
    let nack_delta = current.nack_count.saturating_sub(previous.nack_count);
    let pli_delta = current.pli_count.saturating_sub(previous.pli_count);
    let fir_delta = current.fir_count.saturating_sub(previous.fir_count);
    let jitter_emitted_delta =
        current.jitter_buffer_emitted_count.saturating_sub(previous.jitter_buffer_emitted_count);
    let multi_packet_frames_delta = current
        .frames_assembled_from_multiple_packets
        .saturating_sub(previous.frames_assembled_from_multiple_packets);
    let decode_time_delta = (current.total_decode_time - previous.total_decode_time).max(0.0);
    let processing_delay_delta =
        (current.total_processing_delay - previous.total_processing_delay).max(0.0);
    let decode_ms_per_frame = if frames_decoded_delta > 0 {
        decode_time_delta * 1_000.0 / f64::from(frames_decoded_delta)
    } else {
        0.0
    };
    let processing_ms_per_frame = if frames_received_delta > 0 {
        processing_delay_delta * 1_000.0 / frames_received_delta as f64
    } else {
        0.0
    };

    info!(
        "Inbound RTP +{:.1}s: +{} pkts (+{} media bytes, +{} header), lost {}->{}, jitter {:.3}s, frames recv/dec/render/drop +{}/{}/{}/{}, key +{}, discarded +{}, nack/pli/fir +{}/{}/{}, decode {:.2}ms/frame, processing {:.2}ms/frame, jitterbuf_emit +{}, multi_pkt_frames +{}, layer {}x{} {:.1}fps",
        elapsed.as_secs_f64(),
        packets_delta,
        bytes_delta,
        header_bytes_delta,
        previous.packets_lost,
        current.packets_lost,
        inbound.received.jitter,
        frames_received_delta,
        frames_decoded_delta,
        frames_rendered_delta,
        frames_dropped_delta,
        keyframes_delta,
        discarded_delta,
        nack_delta,
        pli_delta,
        fir_delta,
        decode_ms_per_frame,
        processing_ms_per_frame,
        jitter_emitted_delta,
        multi_packet_frames_delta,
        inbound.inbound.frame_width,
        inbound.inbound.frame_height,
        inbound.inbound.frames_per_second,
    );

    if bytes_delta > 0 && frames_decoded_delta == 0 {
        warn!(
            "Inbound RTP bytes are increasing but frames_decoded did not increase in the last {:.1}s (frames_received +{}, keyframes_decoded total {})",
            elapsed.as_secs_f64(),
            frames_received_delta,
            current.key_frames_decoded,
        );
    }
}

fn update_simulcast_quality_from_stats(
    stats: &[livekit::webrtc::stats::RtcStats],
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let Some(inbound) = find_video_inbound_stats(stats) else {
        return;
    };
    let Some((fw, fh)) = simulcast_state_full_dims(simulcast) else {
        return;
    };

    let q = infer_quality_from_dims(
        fw,
        fh,
        inbound.inbound.frame_width as u32,
        inbound.inbound.frame_height as u32,
    );
    let mut sc = simulcast.lock();
    sc.active_quality = Some(q);
}

/// Returns the current wall-clock time as microseconds since Unix epoch.
fn current_timestamp_us() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

/// Format a user timestamp (microseconds since Unix epoch) as
/// `yyyy-mm-dd hh:mm:ss:xxx` where xxx is milliseconds.
fn format_timestamp_us(ts_us: u64) -> String {
    DateTime::<Utc>::from_timestamp_micros(ts_us as i64)
        .map(|dt| {
            dt.format("%Y-%m-%d %H:%M:%S:").to_string()
                + &format!("{:03}", dt.timestamp_subsec_millis())
        })
        .unwrap_or_else(|| format!("<invalid timestamp {ts_us}>"))
}

fn format_optional_timestamp_us(ts_us: Option<u64>) -> String {
    ts_us.map(format_timestamp_us).unwrap_or_else(|| "N/A".to_string())
}

/// Format the us delta as a millisecond string like `"12.3ms"`.
fn format_us_delta_ms(later_us: u64, earlier_us: u64) -> String {
    let delta_us = later_us.saturating_sub(earlier_us);
    format!("{:.1}ms", delta_us as f64 / 1_000.0)
}

fn simulcast_state_full_dims(state: &Arc<Mutex<SimulcastState>>) -> Option<(u32, u32)> {
    let sc = state.lock();
    sc.full_dims
}

async fn handle_track_subscribed(
    track: livekit::track::RemoteTrack,
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
    allowed_identity: &Option<String>,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    ctrl_c_received: &Arc<AtomicBool>,
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    // If a participant filter is set, skip others
    if let Some(ref allow) = allowed_identity {
        if participant.identity().as_str() != allow {
            debug!("Skipping track from '{}' (filter set to '{}')", participant.identity(), allow);
            return;
        }
    }

    let livekit::track::RemoteTrack::Video(video_track) = track else {
        return;
    };

    let sid = publication.sid().clone();
    let codec = codec_label(&publication.mime_type());
    // Only handle if we don't already have an active video track
    {
        let mut active = active_sid.lock();
        if active.as_ref() == Some(&sid) {
            debug!("Track {} already active, ignoring duplicate subscribe", sid);
            return;
        }
        if active.is_some() {
            debug!(
                "A video track is already active ({}), ignoring new subscribe {}",
                active.as_ref().unwrap(),
                sid
            );
            return;
        }
        *active = Some(sid.clone());
    }

    // Update HUD codec label and feature flags early (before first frame arrives)
    {
        let mut s = shared.lock();
        s.codec = codec;
        s.has_user_timestamp =
            publication.packet_trailer_features().contains(&PacketTrailerFeature::PtfUserTimestamp);
    }

    info!(
        "Subscribed to video track: {} (sid {}) from {} - codec: {}, simulcast: {}, dimension: {}x{}, packet_trailer_features: {:?}",
        publication.name(),
        publication.sid(),
        participant.identity(),
        publication.mime_type(),
        publication.simulcasted(),
        publication.dimension().0,
        publication.dimension().1,
        publication.packet_trailer_features(),
    );

    let rtc_track = video_track.rtc_track();

    // Start background sink task immediately so stats lookup cannot delay first-frame handling.
    let shared2 = shared.clone();
    let active_sid2 = active_sid.clone();
    let my_sid = sid.clone();
    let ctrl_c_sink = ctrl_c_received.clone();
    // Initialize simulcast state for this publication
    {
        let mut sc = simulcast.lock();
        sc.available = publication.simulcasted();
        let dim = publication.dimension();
        sc.full_dims = Some((dim.0, dim.1));
        sc.requested_quality = None;
        sc.active_quality = None;
        sc.publication = Some(publication.clone());
    }
    tokio::spawn(async move {
        let mut sink = NativeVideoStream::new(rtc_track);
        let mut frames: u64 = 0;
        let mut last_log = Instant::now();
        let mut logged_first = false;
        let mut fps_window_frames: u64 = 0;
        let mut fps_window_start = Instant::now();
        let mut fps_smoothed: f32 = 0.0;
        // YUV buffers reused to avoid per-frame allocations
        let mut y_buf: Vec<u8> = Vec::new();
        let mut u_buf: Vec<u8> = Vec::new();
        let mut v_buf: Vec<u8> = Vec::new();
        loop {
            if ctrl_c_sink.load(Ordering::Acquire) {
                break;
            }
            let next = tokio::select! {
                _ = wait_for_shutdown(ctrl_c_sink.clone()) => None,
                frame = sink.next() => frame,
            };
            let Some(frame) = next else { break };
            let received_at_us = current_timestamp_us();
            let w = frame.buffer.width();
            let h = frame.buffer.height();
            let buffer_type = frame.buffer.buffer_type();
            let rotation = frame.rotation;
            let timestamp_us = frame.timestamp_us;
            let frame_metadata = frame.frame_metadata;

            if !logged_first {
                info!(
                    "First decoded subscriber frame: {}x{}, buffer={:?}, rotation={:?}, timestamp_us={}, metadata={:?}",
                    w, h, buffer_type, rotation, timestamp_us, frame_metadata
                );
                logged_first = true;
            }

            let i420 = frame.buffer.to_i420();
            let (sy, su, sv) = i420.strides();
            let (dy, du, dv) = i420.data();

            let width = w as u32;
            let height = h as u32;
            let uv_w = (width + 1) / 2;
            let uv_h = (height + 1) / 2;
            let y_bytes_per_row = align_up(width, 256);
            let uv_bytes_per_row = align_up(uv_w, 256);

            pack_plane(dy, sy as u32, width, height, y_bytes_per_row, &mut y_buf);
            pack_plane(du, su as u32, uv_w, uv_h, uv_bytes_per_row, &mut u_buf);
            pack_plane(dv, sv as u32, uv_w, uv_h, uv_bytes_per_row, &mut v_buf);

            // Swap buffers into shared state
            let mut s = shared2.lock();
            s.width = width;
            s.height = height;
            s.y_bytes_per_row = y_bytes_per_row;
            s.uv_bytes_per_row = uv_bytes_per_row;
            std::mem::swap(&mut s.y, &mut y_buf);
            std::mem::swap(&mut s.u, &mut u_buf);
            std::mem::swap(&mut s.v, &mut v_buf);
            s.dirty = true;
            s.received_at_us = Some(received_at_us);

            s.frame_metadata = frame_metadata;

            if !s.has_user_timestamp && frame_metadata.and_then(|m| m.user_timestamp).is_some() {
                s.has_user_timestamp = true;
            }

            // Update smoothed FPS (~500ms window)
            fps_window_frames += 1;
            let win_elapsed = fps_window_start.elapsed();
            if win_elapsed >= Duration::from_millis(500) {
                let inst_fps = (fps_window_frames as f32) / (win_elapsed.as_secs_f32().max(0.001));
                fps_smoothed = if fps_smoothed <= 0.0 {
                    inst_fps
                } else {
                    // light EMA smoothing to reduce jitter
                    (fps_smoothed * 0.7) + (inst_fps * 0.3)
                };
                s.fps = fps_smoothed;
                fps_window_frames = 0;
                fps_window_start = Instant::now();
            }

            frames += 1;
            let elapsed = last_log.elapsed();
            if elapsed >= Duration::from_secs(2) {
                let fps = frames as f64 / elapsed.as_secs_f64();
                info!(
                    "Decoded subscriber frames: {}x{}, ~{:.1} fps, buffer={:?}, timestamp_us={}, metadata={:?}",
                    w, h, fps, buffer_type, timestamp_us, frame_metadata
                );
                frames = 0;
                last_log = Instant::now();
            }
        }
        info!("Video stream ended for {}", my_sid);
        // Clear active sid if still ours
        let mut active = active_sid2.lock();
        if active.as_ref() == Some(&my_sid) {
            *active = None;
        }
    });

    let ctrl_c_stats = ctrl_c_received.clone();
    let active_sid_stats = active_sid.clone();
    let my_sid_stats = sid.clone();
    let simulcast_stats = simulcast.clone();
    tokio::spawn(async move {
        let mut logged_initial = false;
        let mut last_sample: Option<VideoInboundStatsSample> = None;
        let mut last_stats_log = Instant::now();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if ctrl_c_stats.load(Ordering::Acquire) {
                break;
            }
            if active_sid_stats.lock().as_ref() != Some(&my_sid_stats) {
                break;
            }

            match video_track.get_stats().await {
                Ok(stats) => {
                    if !logged_initial {
                        log_video_inbound_stats(&stats);
                        logged_initial = true;
                    }
                    update_simulcast_quality_from_stats(&stats, &simulcast_stats);
                    if let Some(inbound) = find_video_inbound_stats(&stats) {
                        let current = VideoInboundStatsSample::from(&inbound);
                        let elapsed = last_stats_log.elapsed();
                        if last_sample.is_none() || elapsed >= Duration::from_secs(2) {
                            log_video_inbound_stats_delta(
                                &inbound,
                                &current,
                                last_sample.as_ref(),
                                elapsed,
                            );
                            last_sample = Some(current);
                            last_stats_log = Instant::now();
                        }
                    }
                }
                Err(e) if !logged_initial => {
                    debug!("Failed to get stats for video track: {:?}", e);
                    logged_initial = true;
                }
                Err(_) => {}
            }

            interval.tick().await;
        }
    });
}

fn clear_hud_and_simulcast(shared: &Arc<Mutex<SharedYuv>>, simulcast: &Arc<Mutex<SimulcastState>>) {
    {
        let mut s = shared.lock();
        s.codec.clear();
        s.fps = 0.0;
        s.received_at_us = None;
        s.frame_metadata = None;
        s.has_user_timestamp = false;
        s.gpu_done = None;
    }
    let mut sc = simulcast.lock();
    *sc = SimulcastState::default();
}

fn handle_track_unsubscribed(
    publication: RemoteTrackPublication,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let sid = publication.sid().clone();
    let mut active = active_sid.lock();
    if active.as_ref() == Some(&sid) {
        info!("Video track unsubscribed ({}), clearing active sink", sid);
        *active = None;
    }
    clear_hud_and_simulcast(shared, simulcast);
}

fn handle_track_unpublished(
    publication: RemoteTrackPublication,
    shared: &Arc<Mutex<SharedYuv>>,
    active_sid: &Arc<Mutex<Option<TrackSid>>>,
    simulcast: &Arc<Mutex<SimulcastState>>,
) {
    let sid = publication.sid().clone();
    let mut active = active_sid.lock();
    if active.as_ref() == Some(&sid) {
        info!("Video track unpublished ({}), clearing active sink", sid);
        *active = None;
    }
    clear_hud_and_simulcast(shared, simulcast);
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
    simulcast: Arc<Mutex<SimulcastState>>,
    ctrl_c_received: Arc<AtomicBool>,
    locked_aspect: Option<f32>,
    display_timestamp: bool,
    /// Cached timestamp overlay text to avoid layout churn on every repaint.
    last_timestamp_text: String,
    /// Cached latency text, refreshed at 2 Hz for readability.
    last_latency_text: String,
    last_render_dur_text: String,
    last_latency_refresh: Option<Instant>,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ctrl_c_received.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Lock aspect ratio based on the first received video frame.
        if self.locked_aspect.is_none() {
            let s = self.shared.lock();
            if s.width > 0 && s.height > 0 {
                self.locked_aspect = Some(s.width as f32 / s.height as f32);
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Ensure we keep repainting for smooth playback.
            ui.ctx().request_repaint();

            // Render into a centered rect that matches the source aspect ratio. This keeps resize
            // smooth (no feedback loop) and avoids stretching/distortion while dragging.
            let available = ui.available_size();
            let size = if let Some(aspect) = self.locked_aspect {
                let mut w = available.x.max(1.0);
                let mut h = (w / aspect).max(1.0);
                if h > available.y.max(1.0) {
                    h = available.y.max(1.0);
                    w = (h * aspect).max(1.0);
                }
                egui::vec2(w, h)
            } else {
                egui::vec2(available.x.max(1.0), available.y.max(1.0))
            };

            ui.with_layout(
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    let cb = egui_wgpu_backend::Callback::new_paint_callback(
                        rect,
                        YuvPaintCallback { shared: self.shared.clone() },
                    );
                    ui.painter().add(cb);
                },
            );
        });

        // Resolution/FPS overlay: top-right
        egui::Area::new("video_hud".into())
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0))
            .interactable(false)
            .show(ctx, |ui| {
                let s = self.shared.lock();
                if s.width == 0 || s.height == 0 || s.fps <= 0.0 || s.codec.is_empty() {
                    return;
                }
                let mut text = format!("{} {}x{} {:.1}fps", s.codec, s.width, s.height, s.fps);
                let sc = self.simulcast.lock();
                if sc.available {
                    let layer = sc
                        .active_quality
                        .map(|q| match q {
                            livekit::track::VideoQuality::Low => "Low",
                            livekit::track::VideoQuality::Medium => "Medium",
                            livekit::track::VideoQuality::High => "High",
                        })
                        .unwrap_or("?");
                    text.push_str(&format!("\nSimulcast: {}", layer));
                } else {
                    text.push_str("\nSimulcast: off");
                }
                drop(sc);
                egui::Frame::NONE
                    .fill(egui::Color32::from_black_alpha(140))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).color(egui::Color32::WHITE))
                                .extend(),
                        );
                    });
            });

        if self.display_timestamp {
            let s = self.shared.lock();
            let meta = s.frame_metadata;
            let receive_us = s.received_at_us;
            let has_user_timestamp = s.has_user_timestamp;
            let gpu_done = s.gpu_done;
            drop(s);

            let publish_us = meta.and_then(|m| m.user_timestamp);
            let frame_id = meta.and_then(|m| m.frame_id);

            // Prefer the GPU-done sample so latency reflects "pixels drawn", not just CPU receive.
            let gpu_frame_id = gpu_done.and_then(|g| g.frame_id);
            let hud_frame_id = gpu_frame_id.or(frame_id);
            let hud_publish_us = gpu_done.and_then(|g| g.publish_us).or(publish_us);
            let hud_receive_us = gpu_done.map(|g| g.cpu_received_us).or(receive_us);
            let hud_gpu_done_us = gpu_done.map(|g| g.gpu_done_us);

            if hud_publish_us.is_some() || hud_frame_id.is_some() {
                let frame_id_line = match hud_frame_id {
                    Some(fid) => format!("Frame ID:    {}", fid),
                    None => "Frame ID:    N/A".to_string(),
                };
                if has_user_timestamp {
                    let should_refresh = self.last_latency_text.is_empty()
                        || self
                            .last_latency_refresh
                            .map_or(true, |last| last.elapsed() >= Duration::from_millis(500));
                    if should_refresh {
                        self.last_latency_text = match (hud_publish_us, hud_gpu_done_us) {
                            (Some(pub_ts), Some(gpu_ts)) => format_us_delta_ms(gpu_ts, pub_ts),
                            (Some(pub_ts), None) => match hud_receive_us {
                                Some(recv_ts) => format_us_delta_ms(recv_ts, pub_ts),
                                None => "N/A".to_string(),
                            },
                            _ => "N/A".to_string(),
                        };
                        self.last_render_dur_text = match (hud_receive_us, hud_gpu_done_us) {
                            (Some(recv_ts), Some(gpu_ts)) => format_us_delta_ms(gpu_ts, recv_ts),
                            _ => "N/A".to_string(),
                        };
                        self.last_latency_refresh = Some(Instant::now());
                    }
                    let gpu_done_line = match hud_gpu_done_us {
                        Some(ts) => format!("Render:      {}", format_timestamp_us(ts)),
                        None => "Render:      N/A".to_string(),
                    };
                    self.last_timestamp_text = format!(
                        "{}\nSensor:      {}\nReceive:     {}\n{}\nRender dur:  {}\nE2E Latency: {}",
                        frame_id_line,
                        format_optional_timestamp_us(hud_publish_us),
                        format_optional_timestamp_us(hud_receive_us),
                        gpu_done_line,
                        self.last_render_dur_text,
                        self.last_latency_text,
                    );
                } else {
                    self.last_timestamp_text = frame_id_line;
                }
            }

            if !self.last_timestamp_text.is_empty() {
                egui::Area::new("timestamp_hud".into())
                    .anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::NONE
                            .fill(egui::Color32::from_black_alpha(140))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::same(6))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.last_timestamp_text)
                                            .color(egui::Color32::WHITE)
                                            .monospace(),
                                    )
                                    .extend(),
                                );
                            });
                    });
            }
        }

        // Simulcast layer controls: bottom-left overlay
        egui::Area::new("simulcast_controls".into())
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -10.0))
            .interactable(true)
            .show(ctx, |ui| {
                let mut sc = self.simulcast.lock();
                if !sc.available {
                    return;
                }
                let selected = sc.requested_quality.or(sc.active_quality);
                ui.horizontal(|ui| {
                    let choices = [
                        (livekit::track::VideoQuality::Low, "Low"),
                        (livekit::track::VideoQuality::Medium, "Med"),
                        (livekit::track::VideoQuality::High, "High"),
                    ];
                    for (q, label) in choices {
                        let is_selected = selected.is_some_and(|s| s == q);
                        let resp = ui.selectable_label(is_selected, label);
                        if resp.clicked() {
                            if let Some(ref pub_remote) = sc.publication {
                                pub_remote.set_video_quality(q);
                                sc.requested_quality = Some(q);
                            }
                        }
                    }
                });
            });

        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    let ctrl_c_received = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let ctrl_c_received = ctrl_c_received.clone();
        async move {
            let _ = tokio::signal::ctrl_c().await;
            ctrl_c_received.store(true, Ordering::Release);
            info!("Ctrl-C received, exiting...");
        }
    });

    run(args, ctrl_c_received).await
}

async fn run(args: Args, ctrl_c_received: Arc<AtomicBool>) -> Result<()> {
    // LiveKit connection details (prefer CLI args, fallback to env vars)
    let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok()).expect(
        "LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable",
    );
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LiveKit API key must be provided via --api-key argument or LIVEKIT_API_KEY environment variable");
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LiveKit API secret must be provided via --api-secret argument or LIVEKIT_API_SECRET environment variable");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            can_subscribe: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.dynacast = true;
    room_options.adaptive_stream = true;

    // Configure E2EE if an encryption key is provided
    if let Some(ref e2ee_key) = args.e2ee_key {
        let key_provider = KeyProvider::with_shared_key(
            KeyProviderOptions::default(),
            e2ee_key.as_bytes().to_vec(),
        );
        room_options.encryption =
            Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
        info!("E2EE enabled with AES-GCM encryption");
    }

    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Enable E2EE after connection
    if args.e2ee_key.is_some() {
        room.e2ee_manager().set_enabled(true);
        info!("End-to-end encryption activated");
    }

    // Shared YUV buffer for UI/GPU
    let shared = Arc::new(Mutex::new(SharedYuv {
        width: 0,
        height: 0,
        y_bytes_per_row: 0,
        uv_bytes_per_row: 0,
        y: Vec::new(),
        u: Vec::new(),
        v: Vec::new(),
        codec: String::new(),
        fps: 0.0,
        dirty: false,
        received_at_us: None,
        frame_metadata: None,
        has_user_timestamp: false,
        gpu_done: None,
    }));

    // Subscribe to room events: on first video track, start sink task
    let allowed_identity = args.participant.clone();
    let shared_clone = shared.clone();
    // Track currently active video track SID to handle unpublish/unsubscribe
    let active_sid = Arc::new(Mutex::new(None::<TrackSid>));
    // Shared simulcast UI/control state
    let simulcast = Arc::new(Mutex::new(SimulcastState::default()));
    let simulcast_events = simulcast.clone();
    let ctrl_c_events = ctrl_c_received.clone();
    tokio::spawn(async move {
        let active_sid = active_sid.clone();
        let simulcast = simulcast_events;
        let mut events = room.subscribe();
        info!("Subscribed to room events");
        while let Some(evt) = events.recv().await {
            debug!("Room event: {:?}", evt);
            match evt {
                RoomEvent::TrackSubscribed { track, publication, participant } => {
                    handle_track_subscribed(
                        track,
                        publication,
                        participant,
                        &allowed_identity,
                        &shared_clone,
                        &active_sid,
                        &ctrl_c_events,
                        &simulcast,
                    )
                    .await;
                }
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    handle_track_unsubscribed(publication, &shared_clone, &active_sid, &simulcast);
                }
                RoomEvent::TrackUnpublished { publication, .. } => {
                    handle_track_unpublished(publication, &shared_clone, &active_sid, &simulcast);
                }
                _ => {}
            }
        }
    });

    // Start UI
    let app = VideoApp {
        shared,
        simulcast,
        ctrl_c_received: ctrl_c_received.clone(),
        locked_aspect: None,
        display_timestamp: args.display_timestamp,
        last_timestamp_text: String::new(),
        last_latency_text: String::new(),
        last_render_dur_text: String::new(),
        last_latency_refresh: None,
    };
    let native_options = eframe::NativeOptions { vsync: false, ..Default::default() };
    eframe::run_native(
        "LiveKit Video Subscriber",
        native_options,
        Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))),
    )?;

    // If the window was closed manually, still signal shutdown to background threads.
    ctrl_c_received.store(true, Ordering::Release);

    Ok(())
}

// ===== WGPU I420 renderer =====

struct YuvPaintCallback {
    shared: Arc<Mutex<SharedYuv>>,
}

struct YuvGpuState {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_layout: wgpu::BindGroupLayout,
    y_tex: wgpu::Texture,
    u_tex: wgpu::Texture,
    v_tex: wgpu::Texture,
    y_view: wgpu::TextureView,
    u_view: wgpu::TextureView,
    v_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    params_buf: wgpu::Buffer,
    y_pad_w: u32,
    uv_pad_w: u32,
    dims: (u32, u32),
    upload_y: Vec<u8>,
    upload_u: Vec<u8>,
    upload_v: Vec<u8>,
}

impl YuvGpuState {
    fn create_textures(
        device: &wgpu::Device,
        _width: u32,
        height: u32,
        y_pad_w: u32,
        uv_pad_w: u32,
    ) -> (
        wgpu::Texture,
        wgpu::Texture,
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::TextureView,
        wgpu::TextureView,
    ) {
        let y_size = wgpu::Extent3d { width: y_pad_w, height, depth_or_array_layers: 1 };
        let uv_size =
            wgpu::Extent3d { width: uv_pad_w, height: (height + 1) / 2, depth_or_array_layers: 1 };
        let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING;
        let desc = |size: wgpu::Extent3d| wgpu::TextureDescriptor {
            label: Some("yuv_plane"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage,
            view_formats: &[],
        };
        let y_tex = device.create_texture(&desc(y_size));
        let u_tex = device.create_texture(&desc(uv_size));
        let v_tex = device.create_texture(&desc(uv_size));
        let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let u_view = u_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let v_view = v_tex.create_view(&wgpu::TextureViewDescriptor::default());
        (y_tex, u_tex, v_tex, y_view, u_view, v_view)
    }
}

fn align_up(value: u32, alignment: u32) -> u32 {
    ((value + alignment - 1) / alignment) * alignment
}

fn resize_reused_buffer(buf: &mut Vec<u8>, len: usize) {
    if buf.len() != len {
        buf.resize(len, 0);
    }
}

fn pack_plane(
    src: &[u8],
    src_stride: u32,
    row_width: u32,
    rows: u32,
    dst_stride: u32,
    dst: &mut Vec<u8>,
) {
    resize_reused_buffer(dst, (dst_stride * rows) as usize);
    for row in 0..rows {
        let src_off = (row * src_stride) as usize;
        let dst_off = (row * dst_stride) as usize;
        let row_end = dst_off + row_width as usize;
        dst[dst_off..row_end].copy_from_slice(&src[src_off..src_off + row_width as usize]);
        if dst_stride > row_width {
            dst[row_end..dst_off + dst_stride as usize].fill(0);
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsUniform {
    src_w: u32,
    src_h: u32,
    y_tex_w: u32,
    uv_tex_w: u32,
}

impl CallbackTrait for YuvPaintCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_desc: &egui_wgpu_backend::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu_backend::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // Initialize or update GPU state lazily based on current frame
        let mut shared = self.shared.lock();

        // Nothing to draw yet
        if shared.width == 0 || shared.height == 0 {
            return Vec::new();
        }

        // Fetch or create our GPU state
        if resources.get::<YuvGpuState>().is_none() {
            // Build pipeline and initial small textures; will be recreated on first upload
            let shader_src = include_str!("yuv_shader.wgsl");
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("yuv_shader"),
                source: wgpu::ShaderSource::Wgsl(shader_src.into()),
            });

            let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("yuv_bind_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(
                                    std::mem::size_of::<ParamsUniform>() as u64
                                )
                                .unwrap(),
                            ),
                        },
                        count: None,
                    },
                ],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("yuv_pipeline_layout"),
                bind_group_layouts: &[&bind_layout],
                push_constant_ranges: &[],
            });

            let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("yuv_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("yuv_sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("yuv_params"),
                contents: bytemuck::bytes_of(&ParamsUniform {
                    src_w: 1,
                    src_h: 1,
                    y_tex_w: 1,
                    uv_tex_w: 1,
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Initial tiny textures
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, 1, 1, 256, 256);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&v_view),
                    },
                    wgpu::BindGroupEntry { binding: 4, resource: params_buf.as_entire_binding() },
                ],
            });

            let new_state = YuvGpuState {
                pipeline: render_pipeline,
                sampler,
                bind_layout,
                y_tex,
                u_tex,
                v_tex,
                y_view,
                u_view,
                v_view,
                bind_group,
                params_buf,
                y_pad_w: 256,
                uv_pad_w: 256,
                dims: (0, 0),
                upload_y: Vec::new(),
                upload_u: Vec::new(),
                upload_v: Vec::new(),
            };
            resources.insert(new_state);
        }
        let state = resources.get_mut::<YuvGpuState>().unwrap();

        let dims = (shared.width, shared.height);
        let upload_row_bytes = (shared.y_bytes_per_row, shared.uv_bytes_per_row);
        let mut gpu_sample_in_flight: Option<PendingGpuSample> = None;
        let has_dirty_frame = if shared.dirty {
            std::mem::swap(&mut state.upload_y, &mut shared.y);
            std::mem::swap(&mut state.upload_u, &mut shared.u);
            std::mem::swap(&mut state.upload_v, &mut shared.v);
            shared.dirty = false;
            if let Some(cpu_received_us) = shared.received_at_us {
                let frame_id = shared.frame_metadata.and_then(|m| m.frame_id);
                let publish_us = shared.frame_metadata.and_then(|m| m.user_timestamp);
                gpu_sample_in_flight =
                    Some(PendingGpuSample { frame_id, publish_us, cpu_received_us });
            }
            true
        } else {
            false
        };
        drop(shared);

        // Recreate textures/bind group on size change.
        if state.dims != dims {
            let y_pad_w = align_up(dims.0, 256);
            let uv_w = (dims.0 + 1) / 2;
            let uv_pad_w = align_up(uv_w, 256);
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, dims.0, dims.1, y_pad_w, uv_pad_w);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bind_group"),
                layout: &state.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&state.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&v_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: state.params_buf.as_entire_binding(),
                    },
                ],
            });
            state.y_tex = y_tex;
            state.u_tex = u_tex;
            state.v_tex = v_tex;
            state.y_view = y_view;
            state.u_view = u_view;
            state.v_view = v_view;
            state.bind_group = bind_group;
            state.y_pad_w = y_pad_w;
            state.uv_pad_w = uv_pad_w;
            state.dims = dims;
        }

        if has_dirty_frame {
            let uv_w = (dims.0 + 1) / 2;
            let uv_h = (dims.1 + 1) / 2;

            if upload_row_bytes.0 >= dims.0 {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.y_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_y,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.0),
                        rows_per_image: Some(dims.1),
                    },
                    wgpu::Extent3d { width: dims.0, height: dims.1, depth_or_array_layers: 1 },
                );
            }

            if upload_row_bytes.1 >= uv_w {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.u_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_u,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.1),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
                );
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &state.v_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &state.upload_v,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(upload_row_bytes.1),
                        rows_per_image: Some(uv_h),
                    },
                    wgpu::Extent3d { width: uv_w, height: uv_h, depth_or_array_layers: 1 },
                );
            }

            queue.write_buffer(
                &state.params_buf,
                0,
                bytemuck::bytes_of(&ParamsUniform {
                    src_w: dims.0,
                    src_h: dims.1,
                    y_tex_w: state.y_pad_w,
                    uv_tex_w: state.uv_pad_w,
                }),
            );
        }

        // Ride an empty command buffer with egui's submit so we can stamp GPU-done.
        if let Some(sample) = gpu_sample_in_flight {
            let shared_for_cb = self.shared.clone();
            let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("yuv_gpu_done_probe"),
            });
            let cb = encoder.finish();
            cb.on_submitted_work_done(move || {
                let gpu_done_us = current_timestamp_us();
                let mut s = shared_for_cb.lock();
                s.gpu_done = Some(GpuDoneSample {
                    frame_id: sample.frame_id,
                    publish_us: sample.publish_us,
                    cpu_received_us: sample.cpu_received_us,
                    gpu_done_us,
                });
            });
            vec![cb]
        } else {
            Vec::new()
        }
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu_backend::CallbackResources,
    ) {
        let Some(state) = resources.get::<YuvGpuState>() else {
            return;
        };
        if state.dims == (0, 0) {
            return;
        }

        render_pass.set_pipeline(&state.pipeline);
        render_pass.set_bind_group(0, &state.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
