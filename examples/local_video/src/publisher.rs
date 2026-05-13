use anyhow::Result;
use clap::{Parser, ValueEnum};
use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
use livekit::options::{
    self, video as video_presets, PacketTrailerFeatures, TrackPublishOptions, VideoCodec,
    VideoEncoding, VideoPreset,
};
use livekit::prelude::*;
use livekit::webrtc::stats::RtcStats;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use livekit_capture::{CaptureConfig, CaptureFrame, Publisher, PublisherConfig};
use log::{debug, info};
use nokhwa::utils::ApiBackend;
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

mod timestamp_burn;

use timestamp_burn::TimestampOverlay;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CaptureSource {
    /// USB UVC webcam via nokhwa (default; cross-platform).
    Uvc,
    /// Raspberry Pi CSI camera via libcamera. Produces DMABUF-backed
    /// frames that the V4L2 hardware encoder imports zero-copy.
    /// Linux-only; requires the `libcamera` feature.
    Libcamera,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List available cameras and exit
    #[arg(long)]
    list_cameras: bool,

    /// Camera capture source
    #[arg(long, value_enum, default_value_t = CaptureSource::Uvc)]
    source: CaptureSource,

    /// Camera index to use (numeric)
    #[arg(long, default_value_t = 0)]
    camera_index: usize,

    /// Desired width
    #[arg(long, default_value_t = 1280)]
    width: u32,

    /// Desired height
    #[arg(long, default_value_t = 720)]
    height: u32,

    /// Desired framerate
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Max video bitrate for the main layer in bps (optional)
    #[arg(long)]
    max_bitrate: Option<u64>,

    /// Enable simulcast publishing (low/medium/high layers as appropriate)
    #[arg(long, default_value_t = false)]
    simulcast: bool,

    /// LiveKit participant identity
    #[arg(long, default_value = "rust-camera-pub")]
    identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")]
    room_name: String,

    /// LiveKit server URL
    #[arg(long)]
    url: Option<String>,

    /// LiveKit API key
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret
    #[arg(long)]
    api_secret: Option<String>,

    /// Use H.265/HEVC encoding if supported (falls back to H.264 on failure)
    #[arg(long, default_value_t = false)]
    h265: bool,

    /// Attach the current system time (microseconds since UNIX epoch) as the user timestamp on each frame
    #[arg(long, default_value_t = false)]
    attach_timestamp: bool,

    /// Burn the attached timestamp into each video frame; does nothing unless --attach-timestamp is also enabled
    #[arg(long, default_value_t = false)]
    burn_timestamp: bool,

    /// Attach a monotonically increasing frame ID to each published frame via the packet trailer
    #[arg(long, default_value_t = false)]
    attach_frame_id: bool,

    /// Shared encryption key for E2EE (enables AES-GCM end-to-end encryption when set)
    #[arg(long)]
    e2ee_key: Option<String>,
}

fn list_cameras() -> Result<()> {
    let cams = nokhwa::query(ApiBackend::Auto)?;
    println!("Available cameras (UVC backend):");
    for (i, cam) in cams.iter().enumerate() {
        println!("{}. {}", i, cam.human_name());
    }
    Ok(())
}

fn log_video_outbound_stats(stats: &[RtcStats]) {
    for stat in stats {
        let RtcStats::OutboundRtp(outbound) = stat else {
            continue;
        };
        if outbound.stream.kind != "video" {
            continue;
        }

        let rid = if outbound.outbound.rid.is_empty() {
            "single".to_string()
        } else {
            outbound.outbound.rid.clone()
        };
        info!(
            "WebRTC outbound ({rid}): {}x{} | ~{:.1} fps | encoded {} (key {}) | sent {} frames, {} packets, {} bytes | encoder: {} | active: {} | target {:.0} bps",
            outbound.outbound.frame_width,
            outbound.outbound.frame_height,
            outbound.outbound.frames_per_second,
            outbound.outbound.frames_encoded,
            outbound.outbound.key_frames_encoded,
            outbound.outbound.frames_sent,
            outbound.sent.packets_sent,
            outbound.sent.bytes_sent,
            outbound.outbound.encoder_implementation,
            outbound.outbound.active,
            outbound.outbound.target_bitrate,
        );
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
    if args.list_cameras {
        return list_cameras();
    }

    // LiveKit connection details
    let url = args
        .url
        .or_else(|| env::var("LIVEKIT_URL").ok())
        .expect("LIVEKIT_URL must be provided via --url or env");
    let api_key = args
        .api_key
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LIVEKIT_API_KEY must be provided via --api-key or env");
    let api_secret = args
        .api_secret
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LIVEKIT_API_SECRET must be provided via --api-secret or env");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            can_publish: true,
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    room_options.dynacast = true;

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
    let room = std::sync::Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    if args.e2ee_key.is_some() {
        room.e2ee_manager().set_enabled(true);
        info!("End-to-end encryption activated");
    }

    {
        let room_clone = room.clone();
        tokio::spawn(async move {
            let mut events = room_clone.subscribe();
            info!("Subscribed to room events");
            while let Some(evt) = events.recv().await {
                debug!("Room event: {:?}", evt);
            }
        });
    }

    // --- Build the capture backend ---

    let capture_cfg = CaptureConfig {
        camera_index: args.camera_index,
        device_path: None,
        width: args.width,
        height: args.height,
        fps: args.fps,
    };

    let (width, height, fps) = (args.width, args.height, args.fps);

    // Create the RTC video source up front so we can hand it to the
    // Publisher and use the same handle to build the LocalVideoTrack.
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
    let track =
        LocalVideoTrack::create_video_track("camera", RtcVideoSource::Native(rtc_source.clone()));

    // --- Configure encoding ---

    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    let target_fps = fps as f64;
    let main_encoding = {
        let base = options::compute_appropriate_encoding(false, width, height, VideoCodec::H264);
        VideoEncoding {
            max_bitrate: args.max_bitrate.unwrap_or(base.max_bitrate),
            max_framerate: target_fps,
        }
    };
    let simulcast_presets = compute_simulcast_presets_30fps(width, height, target_fps);
    info!(
        "Video encoding: {}x{} @ {:.0} fps, {} bps (simulcast layers: {})",
        width,
        height,
        target_fps,
        main_encoding.max_bitrate,
        simulcast_presets
            .iter()
            .map(|p| format!(
                "{}x{}@{:.0}fps/{}bps",
                p.width, p.height, p.encoding.max_framerate, p.encoding.max_bitrate
            ))
            .collect::<Vec<_>>()
            .join(", "),
    );

    let mut packet_trailer_features = PacketTrailerFeatures::default();
    packet_trailer_features.user_timestamp = args.attach_timestamp;
    packet_trailer_features.frame_id = args.attach_frame_id;

    let publish_opts = |codec: VideoCodec| TrackPublishOptions {
        source: TrackSource::Camera,
        simulcast: args.simulcast,
        video_codec: codec,
        packet_trailer_features,
        video_encoding: Some(main_encoding.clone()),
        simulcast_layers: Some(simulcast_presets.clone()),
        ..Default::default()
    };

    let publish_result = room
        .local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_opts(requested_codec))
        .await;

    if let Err(e) = publish_result {
        if matches!(requested_codec, VideoCodec::H265) {
            log::warn!("H.265 publish failed ({}). Falling back to H.264...", e);
            room.local_participant()
                .publish_track(LocalTrack::Video(track.clone()), publish_opts(VideoCodec::H264))
                .await?;
            info!("Published camera track with H.264 fallback");
        } else {
            return Err(e.into());
        }
    } else {
        info!("Published camera track");
    }

    // --- Build the optional burned-in timestamp overlay hook ---
    //
    // The overlay needs CPU access to the Y plane, which is only
    // available on I420 frames. Native (DMABUF) frames are not
    // CPU-accessible without a mapping pass, so the hook simply logs a
    // one-time warning and leaves those frames untouched.

    let mut overlay_state = (args.attach_timestamp && args.burn_timestamp)
        .then(|| (TimestampOverlay::new(width, height), false));
    let hook: Option<livekit_capture::CaptureHook> = if overlay_state.is_some() {
        Some(Box::new(move |frame, _ctx| {
            let Some((overlay, warned)) = overlay_state.as_mut() else { return };
            match frame {
                CaptureFrame::I420 { buffer, capture_ts_us } => {
                    let Some(ts) = *capture_ts_us else { return };
                    let stride_y = buffer.strides().0 as usize;
                    let (data_y, _, _) = buffer.data_mut();
                    overlay.draw(data_y, stride_y, ts);
                }
                CaptureFrame::Native { .. } => {
                    if !*warned {
                        log::warn!(
                            "--burn-timestamp is not supported on the libcamera DMABUF \
                             path (would require CPU mapping); overlay disabled for this run"
                        );
                        *warned = true;
                    }
                }
            }
        }))
    } else {
        None
    };

    // --- Start the publisher actor ---

    let publisher_cfg = PublisherConfig {
        capture: capture_cfg,
        attach_timestamp: args.attach_timestamp,
        attach_frame_id: args.attach_frame_id,
    };

    let publisher: Publisher = match args.source {
        CaptureSource::Uvc => {
            let capture = livekit_capture::uvc::UvcCapture::new();
            Publisher::start(capture, rtc_source.clone(), publisher_cfg, hook)?
        }
        CaptureSource::Libcamera => {
            #[cfg(target_os = "linux")]
            {
                let capture = livekit_capture::libcamera_src::LibCameraCapture::new();
                Publisher::start(capture, rtc_source.clone(), publisher_cfg, hook)?
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = (publisher_cfg, hook, rtc_source);
                anyhow::bail!(
                    "--source libcamera is only available on Linux builds with the \
                     `livekit-capture/libcamera` feature enabled"
                );
            }
        }
    };

    let fmt = publisher.stream_format();
    info!(
        "Publisher running: {}x{} @ {} fps (source: {:?})",
        fmt.width, fmt.height, fmt.fps, args.source
    );

    // --- Periodically log progress until Ctrl-C ---

    let mut last_log = std::time::Instant::now();
    let mut last_published = 0u64;
    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let stats = publisher.stats();
        let secs = last_log.elapsed().as_secs_f64();
        let delta = stats.frames_published.saturating_sub(last_published);
        let fps_est = delta as f64 / secs;
        info!(
            "Video status: {}x{} | ~{:.1} fps | total published {} | dropped {}",
            fmt.width, fmt.height, fps_est, stats.frames_published, stats.frames_dropped,
        );
        match track.get_stats().await {
            Ok(stats) => log_video_outbound_stats(&stats),
            Err(e) => debug!("Unable to read outbound WebRTC stats: {e:?}"),
        }
        last_log = std::time::Instant::now();
        last_published = stats.frames_published;
    }

    publisher.stop();
    Ok(())
}

/// Build simulcast presets that match the SDK defaults but with a uniform frame rate.
/// The SDK's built-in `DEFAULT_SIMULCAST_PRESETS` use 15/20 fps for lower layers;
/// this keeps the same resolutions and bitrates but overrides fps to `target_fps`.
fn compute_simulcast_presets_30fps(width: u32, height: u32, target_fps: f64) -> Vec<VideoPreset> {
    let ar = width as f32 / height as f32;
    let defaults: &[VideoPreset] = if f32::abs(ar - 16.0 / 9.0) < f32::abs(ar - 4.0 / 3.0) {
        video_presets::DEFAULT_SIMULCAST_PRESETS
    } else {
        livekit::options::video43::DEFAULT_SIMULCAST_PRESETS
    };
    defaults
        .iter()
        .map(|p| VideoPreset::new(p.width, p.height, p.encoding.max_bitrate, target_fps))
        .collect()
}
