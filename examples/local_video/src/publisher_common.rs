use anyhow::{Context, Result};
use clap::Args;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{debug, info, warn};
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

#[derive(Args, Debug, Clone)]
pub struct PublishCommonArgs {
    /// Max video bitrate for the main layer in bps (optional)
    #[arg(long)]
    pub max_bitrate: Option<u64>,

    /// Enable simulcast publishing (low/medium/high layers as appropriate)
    #[arg(long, default_value_t = false)]
    pub simulcast: bool,

    /// LiveKit participant identity
    #[arg(long, default_value = "rust-camera-pub")]
    pub identity: String,

    /// LiveKit room name
    #[arg(long, default_value = "video-room")]
    pub room_name: String,

    /// LiveKit server URL
    #[arg(long)]
    pub url: Option<String>,

    /// LiveKit API key
    #[arg(long)]
    pub api_key: Option<String>,

    /// LiveKit API secret
    #[arg(long)]
    pub api_secret: Option<String>,

    /// Use H.265/HEVC encoding if supported (falls back to H.264 on failure)
    #[arg(long, default_value_t = false)]
    pub h265: bool,
}

pub struct PublishedVideoContext {
    pub room: Arc<Room>,
    pub rtc_source: NativeVideoSource,
}

pub fn spawn_ctrl_c_handler() -> Arc<AtomicBool> {
    let ctrl_c_received = Arc::new(AtomicBool::new(false));
    tokio::spawn({
        let ctrl_c_received = Arc::clone(&ctrl_c_received);
        async move {
            let _ = tokio::signal::ctrl_c().await;
            ctrl_c_received.store(true, Ordering::Release);
            info!("Ctrl-C received, exiting...");
        }
    });
    ctrl_c_received
}

pub async fn connect_and_publish_video(
    args: &PublishCommonArgs,
    track_name: &str,
    resolution: VideoResolution,
    max_framerate: f64,
) -> Result<PublishedVideoContext> {
    let url = resolve_required(&args.url, "LIVEKIT_URL")?;
    let api_key = resolve_required(&args.api_key, "LIVEKIT_API_KEY")?;
    let api_secret = resolve_required(&args.api_secret, "LIVEKIT_API_SECRET")?;

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
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    {
        let room_clone = Arc::clone(&room);
        tokio::spawn(async move {
            let mut events = room_clone.subscribe();
            info!("Subscribed to room events");
            while let Some(evt) = events.recv().await {
                debug!("Room event: {:?}", evt);
            }
        });
    }

    let rtc_source = NativeVideoSource::new(resolution, false);
    let track =
        LocalVideoTrack::create_video_track(track_name, RtcVideoSource::Native(rtc_source.clone()));

    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    let publish_opts = |codec: VideoCodec| {
        let mut opts = TrackPublishOptions {
            source: TrackSource::Camera,
            simulcast: args.simulcast,
            video_codec: codec,
            ..Default::default()
        };
        if let Some(bitrate) = args.max_bitrate {
            opts.video_encoding = Some(VideoEncoding { max_bitrate: bitrate, max_framerate });
        }
        opts
    };

    let publish_result = room
        .local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_opts(requested_codec))
        .await;

    if let Err(error) = publish_result {
        if matches!(requested_codec, VideoCodec::H265) {
            warn!("H.265 publish failed ({}). Falling back to H.264...", error);
            room.local_participant()
                .publish_track(LocalTrack::Video(track), publish_opts(VideoCodec::H264))
                .await?;
            info!("Published camera track with H.264 fallback");
        } else {
            return Err(error.into());
        }
    } else {
        info!("Published camera track");
    }

    Ok(PublishedVideoContext { room, rtc_source })
}

pub fn timestamp_us_from_duration(timestamp: Option<Duration>) -> i64 {
    timestamp.map(|value| value.as_micros().min(i64::MAX as u128) as i64).unwrap_or(0)
}

fn resolve_required(cli_value: &Option<String>, env_key: &str) -> Result<String> {
    cli_value
        .clone()
        .or_else(|| env::var(env_key).ok())
        .with_context(|| format!("{env_key} must be provided via flag or env"))
}
