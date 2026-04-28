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
use livekit::webrtc::{video_frame::BoxVideoFrame, video_stream::native::NativeVideoStream};
use livekit_api::access_token;
use log::{debug, info};
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
    codec: String,
    fps: f32,
    repaint_ctx: Option<egui::Context>,
    pending_frame: Option<PendingFrame>,
    /// Time when the latest frame became available to the subscriber code.
    received_at_us: Option<u64>,
    /// Time when the latest frame was uploaded by the render callback.
    uploaded_at_us: Option<u64>,
    /// Packet-trailer metadata from the most recent frame, if any.
    frame_metadata: Option<livekit::webrtc::video_frame::FrameMetadata>,
    /// Whether the publisher advertised PTF_USER_TIMESTAMP in its track info.
    has_user_timestamp: bool,
    /// Frames replaced in the app handoff before the renderer consumed them.
    replaced_frames: u64,
}

struct PendingFrame {
    frame: BoxVideoFrame,
    received_at_us: u64,
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
        let mut sink = NativeVideoStream::latest(rtc_track);
        let mut frames: u64 = 0;
        let mut last_log = Instant::now();
        let mut logged_first = false;
        let mut fps_window_frames: u64 = 0;
        let mut fps_window_start = Instant::now();
        let mut fps_smoothed: f32 = 0.0;
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
            let frame_metadata = frame.frame_metadata;

            if !logged_first {
                debug!("First frame: {}x{}, type {:?}", w, h, frame.buffer.buffer_type());
                logged_first = true;
            }

            let repaint_ctx = {
                let mut s = shared2.lock();
                s.width = w;
                s.height = h;
                if s.pending_frame.replace(PendingFrame { frame, received_at_us }).is_some() {
                    s.replaced_frames += 1;
                }
                s.received_at_us = Some(received_at_us);
                s.frame_metadata = frame_metadata;

                if !s.has_user_timestamp && frame_metadata.and_then(|m| m.user_timestamp).is_some()
                {
                    s.has_user_timestamp = true;
                }

                // Update smoothed FPS (~500ms window)
                fps_window_frames += 1;
                let win_elapsed = fps_window_start.elapsed();
                if win_elapsed >= Duration::from_millis(500) {
                    let inst_fps =
                        (fps_window_frames as f32) / (win_elapsed.as_secs_f32().max(0.001));
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
                        "Receiving video: {}x{}, ~{:.1} fps, sdk_dropped={}, app_replaced={}",
                        w,
                        h,
                        fps,
                        sink.dropped_frames(),
                        s.replaced_frames
                    );
                    frames = 0;
                    last_log = Instant::now();
                }

                s.repaint_ctx.clone()
            };

            if let Some(ctx) = repaint_ctx {
                ctx.request_repaint();
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
        s.uploaded_at_us = None;
        s.frame_metadata = None;
        s.has_user_timestamp = false;
        s.pending_frame = None;
        s.replaced_frames = 0;
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
    last_upload_latency_text: String,
    last_latency_refresh: Option<Instant>,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.ctrl_c_received.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        self.shared.lock().repaint_ctx = Some(ctx.clone());

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
            let upload_us = s.uploaded_at_us;
            let has_user_timestamp = s.has_user_timestamp;
            drop(s);

            let publish_us = meta.and_then(|m| m.user_timestamp);
            let frame_id = meta.and_then(|m| m.frame_id);

            if publish_us.is_some() || frame_id.is_some() {
                let frame_id_line = match frame_id {
                    Some(fid) => format!("Frame ID:   {}", fid),
                    None => "Frame ID:   N/A".to_string(),
                };
                if has_user_timestamp {
                    let (receive_latency, upload_latency) =
                        match (publish_us, receive_us, upload_us) {
                            (Some(pub_ts), Some(recv_ts), uploaded) => {
                                let should_refresh = self.last_latency_text.is_empty()
                                    || self.last_latency_refresh.is_none_or(|last| {
                                        last.elapsed() >= Duration::from_millis(500)
                                    });
                                if should_refresh {
                                    self.last_latency_text = format!(
                                        "{:.1}ms",
                                        recv_ts.saturating_sub(pub_ts) as f64 / 1000.0
                                    );
                                    self.last_upload_latency_text = uploaded
                                        .map(|upload_ts| {
                                            format!(
                                                "{:.1}ms",
                                                upload_ts.saturating_sub(pub_ts) as f64 / 1000.0
                                            )
                                        })
                                        .unwrap_or_else(|| "N/A".to_string());
                                    self.last_latency_refresh = Some(Instant::now());
                                }
                                (
                                    self.last_latency_text.clone(),
                                    self.last_upload_latency_text.clone(),
                                )
                            }
                            _ => {
                                self.last_latency_text = "N/A".to_string();
                                self.last_upload_latency_text = "N/A".to_string();
                                self.last_latency_refresh = None;
                                (
                                    self.last_latency_text.clone(),
                                    self.last_upload_latency_text.clone(),
                                )
                            }
                        };
                    self.last_timestamp_text = format!(
                        "{}\nPublish:    {}\nReceive:    {}\nUpload:     {}\nRecv Lat:   {}\nUpload Lat: {}",
                        frame_id_line,
                        format_optional_timestamp_us(publish_us),
                        format_optional_timestamp_us(receive_us),
                        format_optional_timestamp_us(upload_us),
                        receive_latency,
                        upload_latency,
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

        ctx.request_repaint();
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
    room_options.dynacast = false;
    room_options.adaptive_stream = false;
    info!(
        "Low-latency subscriber mode: latest-frame stream, stable subscription, wgpu renderer, vsync off"
    );

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
        codec: String::new(),
        fps: 0.0,
        repaint_ctx: None,
        pending_frame: None,
        received_at_us: None,
        uploaded_at_us: None,
        frame_metadata: None,
        has_user_timestamp: false,
        replaced_frames: 0,
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
        last_upload_latency_text: String::new(),
        last_latency_refresh: None,
    };
    let native_options = eframe::NativeOptions {
        vsync: false,
        renderer: eframe::Renderer::Wgpu,
        hardware_acceleration: eframe::HardwareAcceleration::Required,
        ..Default::default()
    };
    eframe::run_native(
        "LiveKit Video Subscriber",
        native_options,
        Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))),
    )?;

    // If the window was closed manually, still signal shutdown to background threads.
    ctrl_c_received.store(true, Ordering::Release);

    Ok(())
}

// ===== WGPU YUV renderer =====

struct YuvPaintCallback {
    shared: Arc<Mutex<SharedYuv>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GpuFrameFormat {
    I420,
    Nv12,
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
    format: GpuFrameFormat,
    upload_y: Vec<u8>,
    upload_u: Vec<u8>,
    upload_v: Vec<u8>,
}

impl YuvGpuState {
    fn create_textures(
        device: &wgpu::Device,
        height: u32,
        y_pad_w: u32,
        uv_pad_w: u32,
        format: GpuFrameFormat,
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
        let desc = |size: wgpu::Extent3d, format: wgpu::TextureFormat| wgpu::TextureDescriptor {
            label: Some("yuv_plane"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        };
        let y_tex = device.create_texture(&desc(y_size, wgpu::TextureFormat::R8Unorm));
        let uv_format = match format {
            GpuFrameFormat::I420 => wgpu::TextureFormat::R8Unorm,
            GpuFrameFormat::Nv12 => wgpu::TextureFormat::Rg8Unorm,
        };
        let u_tex = device.create_texture(&desc(uv_size, uv_format));
        let v_tex = device.create_texture(&desc(uv_size, wgpu::TextureFormat::R8Unorm));
        let y_view = y_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let u_view = u_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let v_view = v_tex.create_view(&wgpu::TextureViewDescriptor::default());
        (y_tex, u_tex, v_tex, y_view, u_view, v_view)
    }
}

fn frame_gpu_format(frame: &BoxVideoFrame) -> GpuFrameFormat {
    if frame.buffer.as_nv12().is_some() {
        GpuFrameFormat::Nv12
    } else {
        GpuFrameFormat::I420
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

fn write_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    src: &[u8],
    src_stride: u32,
    row_bytes: u32,
    extent_width: u32,
    rows: u32,
    dst_stride: u32,
    scratch: &mut Vec<u8>,
) {
    let data = if src_stride == dst_stride {
        src
    } else {
        pack_plane(src, src_stride, row_bytes, rows, dst_stride, scratch);
        scratch
    };

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(dst_stride),
            rows_per_image: Some(rows),
        },
        wgpu::Extent3d { width: extent_width, height: rows, depth_or_array_layers: 1 },
    );
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsUniform {
    src_w: u32,
    src_h: u32,
    y_tex_w: u32,
    uv_tex_w: u32,
    format: u32,
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
        let pending = {
            let mut shared = self.shared.lock();
            shared.pending_frame.take()
        };

        let Some(pending) = pending else {
            return Vec::new();
        };
        let frame = pending.frame;
        let dims = (frame.buffer.width(), frame.buffer.height());
        let format = frame_gpu_format(&frame);
        let uv_w = (dims.0 + 1) / 2;
        let uv_h = (dims.1 + 1) / 2;
        let y_pad_w = align_up(dims.0, 256);
        let uv_pad_w = match format {
            GpuFrameFormat::I420 => align_up(uv_w, 256),
            GpuFrameFormat::Nv12 => align_up(uv_w * 2, 256) / 2,
        };
        let y_bytes_per_row = y_pad_w;
        let uv_bytes_per_row = match format {
            GpuFrameFormat::I420 => uv_pad_w,
            GpuFrameFormat::Nv12 => uv_pad_w * 2,
        };

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
                    format: 0,
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Initial tiny textures
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, 1, 256, 256, GpuFrameFormat::I420);
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
                format: GpuFrameFormat::I420,
                upload_y: Vec::new(),
                upload_u: Vec::new(),
                upload_v: Vec::new(),
            };
            resources.insert(new_state);
        }
        let state = resources.get_mut::<YuvGpuState>().unwrap();

        // Recreate textures/bind group on size change.
        if state.dims != dims || state.format != format {
            let (y_tex, u_tex, v_tex, y_view, u_view, v_view) =
                YuvGpuState::create_textures(device, dims.1, y_pad_w, uv_pad_w, format);
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
            state.format = format;
        }

        if let Some(nv12) = frame.buffer.as_nv12() {
            let (stride_y, stride_uv) = nv12.strides();
            let (data_y, data_uv) = nv12.data();
            write_plane(
                queue,
                &state.y_tex,
                data_y,
                stride_y,
                dims.0,
                dims.0,
                dims.1,
                y_bytes_per_row,
                &mut state.upload_y,
            );
            write_plane(
                queue,
                &state.u_tex,
                data_uv,
                stride_uv,
                uv_w * 2,
                uv_w,
                uv_h,
                uv_bytes_per_row,
                &mut state.upload_u,
            );
        } else {
            let converted;
            let i420 = if let Some(i420) = frame.buffer.as_i420() {
                i420
            } else {
                converted = frame.buffer.to_i420();
                &converted
            };
            let (stride_y, stride_u, stride_v) = i420.strides();
            let (data_y, data_u, data_v) = i420.data();
            write_plane(
                queue,
                &state.y_tex,
                data_y,
                stride_y,
                dims.0,
                dims.0,
                dims.1,
                y_bytes_per_row,
                &mut state.upload_y,
            );
            write_plane(
                queue,
                &state.u_tex,
                data_u,
                stride_u,
                uv_w,
                uv_w,
                uv_h,
                uv_bytes_per_row,
                &mut state.upload_u,
            );
            write_plane(
                queue,
                &state.v_tex,
                data_v,
                stride_v,
                uv_w,
                uv_w,
                uv_h,
                uv_bytes_per_row,
                &mut state.upload_v,
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
                format: match format {
                    GpuFrameFormat::I420 => 0,
                    GpuFrameFormat::Nv12 => 1,
                },
            }),
        );

        let uploaded_at_us = current_timestamp_us();
        {
            let mut shared = self.shared.lock();
            if shared.received_at_us == Some(pending.received_at_us) {
                shared.uploaded_at_us = Some(uploaded_at_us);
            }
        }

        Vec::new()
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
