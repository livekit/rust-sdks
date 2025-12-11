use anyhow::Result;
use clap::Parser;
use eframe::egui;
use egui_wgpu as egui_wgpu_backend;
use futures::StreamExt;
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::prelude::*;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token;
use log::{debug, info};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

mod yuv_viewer;
use yuv_viewer::{SharedYuv, YuvPaintCallback};

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

    /// Shared E2EE key (enables end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,

    /// Only subscribe to video from this participant identity
    #[arg(long)]
    participant: Option<String>,

    /// Show system time and delta vs sensor timestamp in the YUV viewer overlay
    #[arg(long, default_value_t = false)]
    show_sys_time: bool,
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

fn simulcast_state_full_dims(
    state: &Arc<Mutex<SimulcastState>>,
) -> Option<(u32, u32)> {
    let sc = state.lock();
    sc.full_dims
}

fn format_sensor_timestamp(ts_micros: i64) -> Option<String> {
    if ts_micros == 0 {
        // Treat 0 as "not set"
        return None;
    }
    // Convert microseconds since UNIX epoch to `OffsetDateTime` in UTC, then format.
    let nanos = i128::from(ts_micros).checked_mul(1_000)?;
    let dt = time::OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
    let format = time::macros::format_description!(
        "[year]-[month]-[day] [hour]:[minute]:[second]:[subsecond digits:3]"
    );
    dt.format(&format).ok()
}

fn now_unix_timestamp_micros() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH")
        .as_micros() as i64
}

struct VideoApp {
    shared: Arc<Mutex<SharedYuv>>,
    simulcast: Arc<Mutex<SimulcastState>>,
    show_sys_time: bool,
    last_latency_ms: Option<i32>,
    last_latency_update: Option<Instant>,
}

impl eframe::App for VideoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let rect = egui::Rect::from_min_size(ui.min_rect().min, available);

            // Ensure we keep repainting for smooth playback
            ui.ctx().request_repaint();

            // Add a custom wgpu paint callback that renders I420 directly
            let cb = egui_wgpu_backend::Callback::new_paint_callback(
                rect,
                YuvPaintCallback { shared: self.shared.clone() },
            );
            ui.painter().add(cb);
        });

        // Sensor timestamp / system time overlay: top-left.
        //
        // When `show_sys_time` is false, we only render the user (sensor) timestamp, if present.
        //
        // When `show_sys_time` is true:
        //   - If there is a sensor timestamp, we render up to three rows:
        //       1) "usr ts: yyyy-mm-dd hh:mm:ss:nnn"    (sensor timestamp)
        //       2) "sys ts: yyyy-mm-dd hh:mm:ss:nnn"    (system timestamp)
        //       3) "latency: xxxxms"                    (delta in ms, 4 digits, updated at 2 Hz)
        //   - If there is no sensor timestamp, we render a single row:
        //       "sys ts: yyyy-mm-dd hh:mm:ss:nnn"
        if self.show_sys_time {
            let (sensor_raw, sensor_text, sys_raw, sys_text_opt) = {
                let shared = self.shared.lock();
                let sensor_raw = shared.sensor_timestamp;
                let sensor_text = sensor_raw.and_then(format_sensor_timestamp);
                let sys_raw = now_unix_timestamp_micros();
                let sys_text = format_sensor_timestamp(sys_raw);
                (sensor_raw, sensor_text, sys_raw, sys_text)
            };

            if let Some(sys_text) = sys_text_opt {
                // Latency: throttle updates to 2 Hz to reduce jitter in the display.
                let latency_to_show = if let Some(sensor) = sensor_raw {
                    let now = Instant::now();
                    let needs_update = self
                        .last_latency_update
                        .map(|prev| now.duration_since(prev) >= Duration::from_millis(500))
                        .unwrap_or(true);
                    if needs_update {
                        let delta_micros = sys_raw - sensor;
                        let delta_ms = delta_micros as f64 / 1000.0;
                        // Clamp to [0, 9999] ms to keep formatting consistent.
                        let clamped = delta_ms.round().clamp(0.0, 9_999.0) as i32;
                        self.last_latency_ms = Some(clamped);
                        self.last_latency_update = Some(now);
                    }
                    self.last_latency_ms
                } else {
                    self.last_latency_ms = None;
                    self.last_latency_update = None;
                    None
                };

                egui::Area::new("sensor_sys_timestamp_overlay".into())
                    .anchor(egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.vertical(|ui| {
                            if let Some(ts_text) = sensor_text {
                                // First row: user (sensor) timestamp
                                let usr_line = format!("usr ts: {ts_text}");
                                ui.label(
                                    egui::RichText::new(usr_line)
                                        .monospace()
                                        .size(22.0)
                                        .color(egui::Color32::WHITE),
                                );

                                // Second row: system timestamp.
                                let sys_line = format!("sys ts: {sys_text}");
                                ui.label(
                                    egui::RichText::new(sys_line)
                                        .monospace()
                                        .size(22.0)
                                        .color(egui::Color32::WHITE),
                                );

                                // Third row: latency in milliseconds (if available).
                                if let Some(latency_ms) = latency_to_show {
                                    let latency_line =
                                        format!("latency: {:04}ms", latency_ms.max(0));
                                    ui.label(
                                        egui::RichText::new(latency_line)
                                            .monospace()
                                            .size(22.0)
                                            .color(egui::Color32::WHITE),
                                    );
                                }
                            } else {
                                // No sensor timestamp: only show system timestamp.
                                let sys_line = format!("sys ts: {sys_text}");
                                ui.label(
                                    egui::RichText::new(sys_line)
                                        .monospace()
                                        .size(22.0)
                                        .color(egui::Color32::WHITE),
                                );
                            }
                        });
                    });
            }
        } else {
            // Original behavior: render only the user (sensor) timestamp, if present.
            let sensor_timestamp_text = {
                let shared = self.shared.lock();
                shared
                    .sensor_timestamp
                    .and_then(format_sensor_timestamp)
            };
            if let Some(ts_text) = sensor_timestamp_text {
                let usr_line = format!("usr ts: {ts_text}");
                egui::Area::new("sensor_timestamp_overlay".into())
                    .anchor(egui::Align2::LEFT_TOP, egui::vec2(20.0, 20.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(usr_line)
                                .monospace()
                                .size(22.0)
                                .color(egui::Color32::WHITE),
                        );
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

    // LiveKit connection details (prefer CLI args, fallback to env vars)
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable");
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
    if let Some(ref key) = args.e2ee_key {
        let key_provider =
            KeyProvider::with_shared_key(KeyProviderOptions::default(), key.clone().into_bytes());
        room_options.encryption =
            Some(E2eeOptions { encryption_type: EncryptionType::Gcm, key_provider });
        info!("E2EE enabled with provided shared key");
    }
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Shared YUV buffer for UI/GPU
    let shared = Arc::new(Mutex::new(SharedYuv {
        width: 0,
        height: 0,
        stride_y: 0,
        stride_u: 0,
        stride_v: 0,
        y: Vec::new(),
        u: Vec::new(),
        v: Vec::new(),
        dirty: false,
        sensor_timestamp: None,
    }));

    // Subscribe to room events: on first video track, start sink task
    let allowed_identity = args.participant.clone();
    let shared_clone = shared.clone();
    let rt = tokio::runtime::Handle::current();
    // Track currently active video track SID to handle unpublish/unsubscribe
    let active_sid = Arc::new(Mutex::new(None::<TrackSid>));
    // Shared simulcast UI/control state
    let simulcast = Arc::new(Mutex::new(SimulcastState::default()));
    let simulcast_events = simulcast.clone();
    tokio::spawn(async move {
        let active_sid = active_sid.clone();
        let simulcast = simulcast_events;
        let mut events = room.subscribe();
        info!("Subscribed to room events");
        while let Some(evt) = events.recv().await {
            debug!("Room event: {:?}", evt);
            match evt {
                RoomEvent::TrackSubscribed { track, publication, participant } => {
                    // If a participant filter is set, skip others
                    if let Some(ref allow) = allowed_identity {
                        if participant.identity().as_str() != allow {
                            debug!("Skipping track from '{}' (filter set to '{}')", participant.identity(), allow);
                            continue;
                        }
                    }
                    if let livekit::track::RemoteTrack::Video(video_track) = track {
                        let sid = publication.sid().clone();
                        // Only handle if we don't already have an active video track
                        {
                            let mut active = active_sid.lock();
                            if active.as_ref() == Some(&sid) {
                                debug!("Track {} already active, ignoring duplicate subscribe", sid);
                                continue;
                            }
                            if active.is_some() {
                                debug!("A video track is already active ({}), ignoring new subscribe {}", active.as_ref().unwrap(), sid);
                                continue;
                            }
                            *active = Some(sid.clone());
                        }

                        info!(
                            "Subscribed to video track: {} (sid {}) from {} - codec: {}, simulcast: {}, dimension: {}x{}",
                            publication.name(),
                            publication.sid(),
                            participant.identity(),
                            publication.mime_type(),
                            publication.simulcasted(),
                            publication.dimension().0,
                            publication.dimension().1
                        );

                        // Try to fetch inbound RTP/codec stats for more details
                        match video_track.get_stats().await {
                            Ok(stats) => {
                                let mut codec_by_id: HashMap<String, (String, String)> = HashMap::new();
                                let mut inbound: Option<livekit::webrtc::stats::InboundRtpStats> = None;
                                for s in stats.iter() {
                                    match s {
                                        livekit::webrtc::stats::RtcStats::Codec(c) => {
                                            codec_by_id.insert(
                                                c.rtc.id.clone(),
                                                (c.codec.mime_type.clone(), c.codec.sdp_fmtp_line.clone()),
                                            );
                                        }
                                        livekit::webrtc::stats::RtcStats::InboundRtp(i) => {
                                            if i.stream.kind == "video" {
                                                inbound = Some(i.clone());
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                if let Some(i) = inbound {
                                    if let Some((mime, fmtp)) = codec_by_id.get(&i.stream.codec_id) {
                                        info!("Inbound codec: {} (fmtp: {})", mime, fmtp);
                                    } else {
                                        info!("Inbound codec id: {}", i.stream.codec_id);
                                    }
                                    info!(
                                        "Inbound current layer: {}x{} ~{:.1} fps, decoder: {}, power_efficient: {}",
                                        i.inbound.frame_width,
                                        i.inbound.frame_height,
                                        i.inbound.frames_per_second,
                                        i.inbound.decoder_implementation,
                                        i.inbound.power_efficient_decoder
                                    );
                                }
                            }
                            Err(e) => debug!("Failed to get stats for video track: {:?}", e),
                        }
                        // Start background sink thread
                        let shared2 = shared_clone.clone();
                        let active_sid2 = active_sid.clone();
                        let my_sid = sid.clone();
                        let rt_clone = rt.clone();
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
                        let simulcast2 = simulcast.clone();
                        std::thread::spawn(move || {
                            let mut sink = NativeVideoStream::new(video_track.rtc_track());
                            let mut frames: u64 = 0;
                            let mut last_log = Instant::now();
                            let mut logged_first = false;
                            let mut last_stats = Instant::now();
                            // YUV buffers reused to avoid per-frame allocations
                            let mut y_buf: Vec<u8> = Vec::new();
                            let mut u_buf: Vec<u8> = Vec::new();
                            let mut v_buf: Vec<u8> = Vec::new();
                            while let Some(frame) = rt_clone.block_on(sink.next()) {
                                let w = frame.buffer.width();
                                let h = frame.buffer.height();

                                if !logged_first {
                                    debug!(
                                        "First frame: {}x{}, type {:?}",
                                        w, h, frame.buffer.buffer_type()
                                    );
                                    logged_first = true;
                                }

                                // Convert to I420 on CPU, but keep planes separate for GPU sampling
                                let i420 = frame.buffer.to_i420();
                                let (sy, su, sv) = i420.strides();
                                let (dy, du, dv) = i420.data();

                                let ch = (h + 1) / 2;

                                // Ensure capacity and copy full plane slices
                                let y_size = (sy * h) as usize;
                                let u_size = (su * ch) as usize;
                                let v_size = (sv * ch) as usize;
                                if y_buf.len() != y_size { y_buf.resize(y_size, 0); }
                                if u_buf.len() != u_size { u_buf.resize(u_size, 0); }
                                if v_buf.len() != v_size { v_buf.resize(v_size, 0); }
                                y_buf.copy_from_slice(dy);
                                u_buf.copy_from_slice(du);
                                v_buf.copy_from_slice(dv);

                                // Fetch any parsed sensor timestamp for this frame, if available.
                                // Treat 0 as "not set".
                                let ts_opt = video_track
                                    .last_sensor_timestamp()
                                    .and_then(|ts| if ts == 0 { None } else { Some(ts) });

                                // Swap buffers into shared state, and only update the
                                // sensor timestamp when we actually have one. This
                                // prevents the overlay from flickering on frames that
                                // don't carry a parsed timestamp.
                                {
                                    let mut s = shared2.lock();
                                    s.width = w as u32;
                                    s.height = h as u32;
                                    s.stride_y = sy as u32;
                                    s.stride_u = su as u32;
                                    s.stride_v = sv as u32;
                                    std::mem::swap(&mut s.y, &mut y_buf);
                                    std::mem::swap(&mut s.u, &mut u_buf);
                                    std::mem::swap(&mut s.v, &mut v_buf);
                                    s.dirty = true;
                                    if let Some(ts) = ts_opt {
                                        s.sensor_timestamp = Some(ts);
                                    }
                                }

                                // Log sensor timestamp + derived latency if available.
                                if let Some(ts) = ts_opt {
                                    use std::time::{SystemTime, UNIX_EPOCH};
                                    let now = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_micros() as i64;

                                    let latency_us = now - ts;
                                    let latency_ms = latency_us as f64 / 1000.0;

                                    info!(
                                        "Subscriber: decoded frame {}x{} sensor_timestamp={} latency={:.2} ms",
                                        w, h, ts, latency_ms
                                    );
                                }

                                frames += 1;
                                let elapsed = last_log.elapsed();
                                if elapsed >= Duration::from_secs(2) {
                                    let fps = frames as f64 / elapsed.as_secs_f64();
                                    info!("Receiving video: {}x{}, ~{:.1} fps", w, h, fps);
                                    frames = 0;
                                    last_log = Instant::now();
                                }
                                // Periodically infer active simulcast quality from inbound stats
                                if last_stats.elapsed() >= Duration::from_secs(1) {
                                    if let Ok(stats) = rt_clone.block_on(video_track.get_stats()) {
                                        let mut inbound: Option<livekit::webrtc::stats::InboundRtpStats> = None;
                                        for s in stats.iter() {
                                            if let livekit::webrtc::stats::RtcStats::InboundRtp(i) = s {
                                                if i.stream.kind == "video" {
                                                    inbound = Some(i.clone());
                                                }
                                            }
                                        }
                                        if let Some(i) = inbound {
                                            if let Some((fw, fh)) = simulcast_state_full_dims(&simulcast2) {
                                                let q = infer_quality_from_dims(fw, fh, i.inbound.frame_width as u32, i.inbound.frame_height as u32);
                                                let mut sc = simulcast2.lock();
                                                sc.active_quality = Some(q);
                                            }
                                        }
                                    }
                                    last_stats = Instant::now();
                                }
                            }
                            info!("Video stream ended for {}", my_sid);
                            // Clear active sid if still ours
                            let mut active = active_sid2.lock();
                            if active.as_ref() == Some(&my_sid) {
                                *active = None;
                            }
                        });
                    }
                }
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    let sid = publication.sid().clone();
                    let mut active = active_sid.lock();
                    if active.as_ref() == Some(&sid) {
                        info!("Video track unsubscribed ({}), clearing active sink", sid);
                        *active = None;
                    }
                    // Clear simulcast state
                    let mut sc = simulcast.lock();
                    *sc = SimulcastState::default();
                }
                RoomEvent::TrackUnpublished { publication, .. } => {
                    let sid = publication.sid().clone();
                    let mut active = active_sid.lock();
                    if active.as_ref() == Some(&sid) {
                        info!("Video track unpublished ({}), clearing active sink", sid);
                        *active = None;
                    }
                    // Clear simulcast state
                    let mut sc = simulcast.lock();
                    *sc = SimulcastState::default();
                }
                _ => {}
            }
        }
    });

    // Start UI
    let app = VideoApp {
        shared,
        simulcast,
        show_sys_time: args.show_sys_time,
        last_latency_ms: None,
        last_latency_update: None,
    };
    let native_options = eframe::NativeOptions::default();
    eframe::run_native("LiveKit Video Subscriber", native_options, Box::new(|_| Ok::<Box<dyn eframe::App>, _>(Box::new(app))))?;

    Ok(())
}

