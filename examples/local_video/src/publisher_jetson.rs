#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("publisher_jetson requires Linux aarch64 on NVIDIA Jetson")
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[path = "argus.rs"]
mod argus;

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod app {
    use anyhow::Result;
    use clap::Parser;
    use livekit::e2ee::{key_provider::*, E2eeOptions, EncryptionType};
    use livekit::options::{
        self, video as video_presets, PacketTrailerFeatures, TrackPublishOptions, VideoCodec,
        VideoEncoding, VideoPreset,
    };
    use livekit::prelude::*;
    use livekit::webrtc::video_frame::FrameMetadata;
    use livekit::webrtc::video_source::native::NativeVideoSource;
    use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
    use livekit_api::access_token;
    use log::{debug, info};
    use std::env;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use crate::argus;

    fn unix_time_us_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock is before UNIX epoch")
            .as_micros() as u64
    }

    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }

    extern "C" {
        fn clock_gettime(clk_id: i32, tp: *mut Timespec) -> i32;
    }

    fn monotonic_time_ns_now() -> Option<u64> {
        const CLOCK_MONOTONIC: i32 = 1;
        let mut ts = Timespec { tv_sec: 0, tv_nsec: 0 };
        let ret = unsafe {
            // SAFETY: `ts` is a valid, writable timespec pointer for the duration of the call.
            clock_gettime(CLOCK_MONOTONIC, &mut ts)
        };
        if ret != 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
            return None;
        }
        Some(ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
    }

    fn sensor_monotonic_ns_to_unix_us(sensor_timestamp_ns: u64, wall_time_us: u64) -> Option<u64> {
        let monotonic_now_ns = monotonic_time_ns_now()?;
        let monotonic_delta_us = monotonic_now_ns.abs_diff(sensor_timestamp_ns) / 1_000;
        if sensor_timestamp_ns <= monotonic_now_ns {
            Some(wall_time_us.saturating_sub(monotonic_delta_us))
        } else {
            Some(wall_time_us.saturating_add(monotonic_delta_us))
        }
    }

    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    struct Args {
        /// MIPI CSI sensor index.
        #[arg(long, default_value_t = 0)]
        camera_index: u32,

        /// Desired width.
        #[arg(long, default_value_t = 1280)]
        width: u32,

        /// Desired height.
        #[arg(long, default_value_t = 720)]
        height: u32,

        /// Desired framerate.
        #[arg(long, default_value_t = 30)]
        fps: u32,

        /// Max video bitrate for the main layer in bps.
        #[arg(long)]
        max_bitrate: Option<u64>,

        /// Enable simulcast publishing.
        #[arg(long, default_value_t = false)]
        simulcast: bool,

        /// LiveKit participant identity.
        #[arg(long, default_value = "rust-jetson-pub")]
        identity: String,

        /// LiveKit room name.
        #[arg(long, default_value = "video-room")]
        room_name: String,

        /// LiveKit server URL.
        #[arg(long)]
        url: Option<String>,

        /// LiveKit API key.
        #[arg(long)]
        api_key: Option<String>,

        /// LiveKit API secret.
        #[arg(long)]
        api_secret: Option<String>,

        /// Use H.265/HEVC encoding if supported, falling back to H.264 on failure.
        #[arg(long, default_value_t = false)]
        h265: bool,

        /// Attach packet-trailer user timestamps where supported.
        #[arg(long, default_value_t = false)]
        attach_timestamp: bool,

        /// Attach monotonically increasing packet-trailer frame IDs where supported.
        #[arg(long, default_value_t = false)]
        attach_frame_id: bool,

        /// Shared encryption key for E2EE.
        #[arg(long)]
        e2ee_key: Option<String>,
    }

    #[tokio::main]
    pub async fn main() -> Result<()> {
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
        let width = args.width;
        let height = args.height;
        let fps = args.fps;

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
        let room = Arc::new(room);
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

        let rtc_source = NativeVideoSource::new(VideoResolution { width, height }, false);
        let track = LocalVideoTrack::create_video_track(
            "mipi-camera",
            RtcVideoSource::Native(rtc_source.clone()),
        );

        let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
        info!("Attempting Jetson publish with codec: {}", requested_codec.as_str());

        let target_fps = fps as f64;
        let main_encoding = {
            let base =
                options::compute_appropriate_encoding(false, width, height, VideoCodec::H264);
            VideoEncoding {
                max_bitrate: args.max_bitrate.unwrap_or(base.max_bitrate),
                max_framerate: target_fps,
            }
        };
        let simulcast_presets = compute_simulcast_presets_30fps(width, height, target_fps);

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
                info!("Published Jetson MIPI camera track with H.264 fallback");
            } else {
                return Err(e.into());
            }
        } else {
            info!("Published Jetson MIPI camera track");
        }

        let session = argus::ArgusCaptureSession::new(args.camera_index, width, height, fps)?;
        info!(
            "Argus MIPI capture session opened: {}x{} @ {} fps (camera {})",
            session.width(),
            session.height(),
            fps,
            args.camera_index
        );

        let ctrl_c_capture = ctrl_c_received.clone();
        let capture_handle = std::thread::Builder::new()
            .name("mipi-capture".into())
            .spawn(move || -> Result<()> {
                let mut session = session;
                let start_ts = Instant::now();
                let mut frames: u64 = 0;
                let mut last_fps_log = Instant::now();
                let mut sum_acquire_ms = 0.0;
                let mut sum_capture_ms = 0.0;
                let mut sum_iter_ms = 0.0;
                let mut consecutive_failures: u32 = 0;
                let mut frame_counter: u32 = 1;
                let mut logged_sensor_ts_source = false;
                let mut logged_sensor_ts_missing = false;
                let mut logged_sensor_ts_conversion_failed = false;
                let mut sensor_timestamp_frames: u64 = 0;
                let mut backup_timestamp_frames: u64 = 0;
                let mut sum_sensor_to_acquire_ms = 0.0;

                loop {
                    if ctrl_c_capture.load(Ordering::Acquire) {
                        break;
                    }

                    let iter_start = Instant::now();
                    let t0 = Instant::now();
                    let argus_frame = match session.acquire_frame() {
                        Ok(frame) => {
                            consecutive_failures = 0;
                            frame
                        }
                        Err(e) => {
                            consecutive_failures += 1;
                            if consecutive_failures <= 3 {
                                log::warn!(
                                    "MIPI frame acquisition failed (attempt {}): {}",
                                    consecutive_failures,
                                    e
                                );
                            }
                            let backoff =
                                Duration::from_millis(5 * (consecutive_failures as u64).min(20));
                            std::thread::sleep(backoff);
                            continue;
                        }
                    };
                    let t1 = Instant::now();
                    let fallback_wall_time_us =
                        if args.attach_timestamp { unix_time_us_now() } else { 0 };

                    let (capture_wall_time_us, timestamp_from_sensor) = if args.attach_timestamp {
                        match argus_frame.sensor_timestamp_ns {
                            Some(sensor_timestamp_ns) => match sensor_monotonic_ns_to_unix_us(
                                sensor_timestamp_ns,
                                fallback_wall_time_us,
                            ) {
                                Some(sensor_wall_time_us) => {
                                    if !logged_sensor_ts_source {
                                        info!(
                                            "Using Argus sensor timestamp for packet trailer user_timestamp"
                                        );
                                        logged_sensor_ts_source = true;
                                    }
                                    (sensor_wall_time_us, true)
                                }
                                None => {
                                    if !logged_sensor_ts_conversion_failed {
                                        log::warn!(
                                            "Failed to convert Argus sensor timestamp to wall time; using backup system wall clock for packet trailer user_timestamp"
                                        );
                                        logged_sensor_ts_conversion_failed = true;
                                    }
                                    (fallback_wall_time_us, false)
                                }
                            },
                            None => {
                                if !logged_sensor_ts_missing {
                                    log::warn!(
                                        "Argus sensor timestamp not available; using backup system wall clock for packet trailer user_timestamp"
                                    );
                                    logged_sensor_ts_missing = true;
                                }
                                (fallback_wall_time_us, false)
                            }
                        }
                    } else {
                        (0, false)
                    };
                    if args.attach_timestamp {
                        if timestamp_from_sensor {
                            sensor_timestamp_frames += 1;
                            sum_sensor_to_acquire_ms += fallback_wall_time_us
                                .saturating_sub(capture_wall_time_us)
                                as f64
                                / 1_000.0;
                        } else {
                            backup_timestamp_frames += 1;
                        }
                    }
                    let user_ts =
                        if args.attach_timestamp { Some(capture_wall_time_us) } else { None };
                    let fid = if args.attach_frame_id {
                        let id = frame_counter;
                        frame_counter = frame_counter.wrapping_add(1);
                        Some(id)
                    } else {
                        None
                    };
                    let frame_metadata = if user_ts.is_some() || fid.is_some() {
                        Some(FrameMetadata { user_timestamp: user_ts, frame_id: fid })
                    } else {
                        None
                    };

                    rtc_source.capture_dmabuf_frame_with_metadata(
                        argus_frame.dmabuf_fd,
                        width,
                        height,
                        0,
                        start_ts.elapsed().as_micros() as i64,
                        frame_metadata,
                    );
                    let t2 = Instant::now();

                    frames += 1;
                    sum_acquire_ms += (t1 - t0).as_secs_f64() * 1000.0;
                    sum_capture_ms += (t2 - t1).as_secs_f64() * 1000.0;
                    sum_iter_ms += (Instant::now() - iter_start).as_secs_f64() * 1000.0;

                    if last_fps_log.elapsed() >= Duration::from_secs(2) {
                        let secs = last_fps_log.elapsed().as_secs_f64();
                        let fps_est = frames as f64 / secs;
                        let n = frames.max(1) as f64;
                        if args.attach_timestamp {
                            let sensor_age_ms = if sensor_timestamp_frames > 0 {
                                sum_sensor_to_acquire_ms / sensor_timestamp_frames as f64
                            } else {
                                0.0
                            };
                            info!(
                                "MIPI publishing: {}x{}, ~{:.1} fps | packet trailer timestamp source: sensor {} frames, backup system {} frames | avg ms: sensor_to_acquire {:.2}, acquire {:.2}, capture {:.2}, iter {:.2}",
                                width,
                                height,
                                fps_est,
                                sensor_timestamp_frames,
                                backup_timestamp_frames,
                                sensor_age_ms,
                                sum_acquire_ms / n,
                                sum_capture_ms / n,
                                sum_iter_ms / n,
                            );
                        } else {
                            info!(
                                "MIPI publishing: {}x{}, ~{:.1} fps | packet trailer timestamp: disabled | avg ms: acquire {:.2}, capture {:.2}, iter {:.2}",
                                width,
                                height,
                                fps_est,
                                sum_acquire_ms / n,
                                sum_capture_ms / n,
                                sum_iter_ms / n,
                            );
                        }
                        frames = 0;
                        sensor_timestamp_frames = 0;
                        backup_timestamp_frames = 0;
                        sum_acquire_ms = 0.0;
                        sum_capture_ms = 0.0;
                        sum_iter_ms = 0.0;
                        sum_sensor_to_acquire_ms = 0.0;
                        last_fps_log = Instant::now();
                    }
                }

                Ok(())
            })?;

        capture_handle
            .join()
            .map_err(|e| anyhow::anyhow!("MIPI capture thread panicked: {:?}", e))??;

        Ok(())
    }

    fn compute_simulcast_presets_30fps(
        width: u32,
        height: u32,
        target_fps: f64,
    ) -> Vec<VideoPreset> {
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
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn main() -> anyhow::Result<()> {
    app::main()
}
