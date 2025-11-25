use anyhow::Result;
use clap::Parser;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use gstreamer as gst;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use gstreamer_video as gst_video;
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use gstreamer_allocators as gst_alloc;
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{NV12Buffer, VideoFrame, VideoRotation};
#[cfg(all(feature = "dmabuf", target_os = "linux"))]
use livekit::webrtc::video_frame::native::NativeBuffer;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token;
use log::{debug, info};
use std::env;
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct BaseArgs {
    /// Desired width
    #[arg(long, default_value_t = 1280)]
    pub width: u32,
    /// Desired height
    #[arg(long, default_value_t = 720)]
    pub height: u32,
    /// Desired framerate
    #[arg(long, default_value_t = 30)]
    pub fps: u32,
    /// Max video bitrate for the main layer in bps (optional)
    #[arg(long)]
    pub max_bitrate: Option<u64>,
    /// LiveKit participant identity
    #[arg(long, default_value = "jetson-pub")]
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

#[cfg(all(feature = "dmabuf", target_os = "linux"))]
pub unsafe fn push_dmabuf_nv12(
    rtc_source: &NativeVideoSource,
    info: &gst_video::VideoInfo,
    buffer: &gst::BufferRef,
) -> anyhow::Result<()> {
    // Extract VideoMeta (strides/offsets).
    let vmeta = gst_video::VideoMeta::from_buffer_ref_readable(buffer)
        .ok_or_else(|| anyhow::anyhow!("no VideoMeta on buffer"))?;
    let stride_y = vmeta.stride(0) as u32;
    let stride_uv = vmeta.stride(1) as u32;
    let offset_y = vmeta.offset(0) as u32;
    let offset_uv = vmeta.offset(1) as u32;
    let width = info.width() as u32;
    let height = info.height() as u32;

    // Extract dmabuf FD from memory (assumes a single dmabuf).
    let mem = buffer
        .peek_memory(0)
        .ok_or_else(|| anyhow::anyhow!("no memory in buffer"))?;
    let fd = gst_alloc::dmabuf_memory_get_fd(mem)
        .ok_or_else(|| anyhow::anyhow!("buffer memory is not dmabuf"))?;

    // Wrap into NativeBuffer and push
    let native = NativeBuffer::from_dmabuf_nv12(
        fd, width, height, stride_y, stride_uv, offset_y, offset_uv,
    );
    let ts = 0; // let source fill with now if 0
    let frame = VideoFrame {
        rotation: VideoRotation::VideoRotation0,
        timestamp_us: ts,
        buffer: native,
    };
    rtc_source.capture_frame(&frame);
    Ok(())
}

pub async fn connect_and_publish(
    args: &BaseArgs,
    width: u32,
    height: u32,
) -> Result<(std::sync::Arc<Room>, NativeVideoSource, LocalVideoTrack)> {
    let url = args.url.clone().or_else(|| env::var("LIVEKIT_URL").ok()).expect(
        "LIVEKIT_URL must be provided via --url or env",
    );
    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LIVEKIT_API_KEY must be provided via --api-key or env");
    let api_secret = args
        .api_secret
        .clone()
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

    info!(
        "Connecting to LiveKit room '{}' as '{}'...",
        args.room_name, args.identity
    );
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = true;
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    let room = std::sync::Arc::new(room);
    info!("Connected: {} - {}", room.name(), room.sid().await);

    // Log room events
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

    // Create LiveKit video source and track
    let rtc_source = NativeVideoSource::new(VideoResolution { width, height });
    let track = LocalVideoTrack::create_video_track(
        "camera",
        RtcVideoSource::Native(rtc_source.clone()),
    );

    // Choose requested codec and attempt to publish; if H.265 fails, retry with H.264
    let requested_codec = if args.h265 { VideoCodec::H265 } else { VideoCodec::H264 };
    info!("Attempting publish with codec: {}", requested_codec.as_str());

    let publish_opts = |codec: VideoCodec| {
        let mut opts = TrackPublishOptions {
            source: TrackSource::Camera,
            simulcast: false,
            video_codec: codec,
            ..Default::default()
        };
        if let Some(bitrate) = args.max_bitrate {
            opts.video_encoding = Some(VideoEncoding {
                max_bitrate: bitrate,
                max_framerate: args.fps as f64,
            });
        }
        opts
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

    Ok((room, rtc_source, track))
}

pub struct CpuNv12Pusher {
    pub width: u32,
    pub height: u32,
    pub frame: VideoFrame<NV12Buffer>,
    pub start_ts: Instant,
}

impl CpuNv12Pusher {
    pub fn new(width: u32, height: u32, stride_y: u32, stride_uv: u32) -> Self {
        let buffer = NV12Buffer::with_strides(width, height, stride_y, stride_uv);
        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            buffer,
        };
        Self { width, height, frame, start_ts: Instant::now() }
    }

    pub fn push(&mut self, rtc_source: &NativeVideoSource, plane_y: &[u8], plane_uv: &[u8]) {
        let (dst_y, dst_uv) = self.frame.buffer.data_mut();
        // Copy with identical stride layout
        dst_y.copy_from_slice(plane_y);
        dst_uv.copy_from_slice(plane_uv);
        self.frame.timestamp_us = self.start_ts.elapsed().as_micros() as i64;
        rtc_source.capture_frame(&self.frame);
    }
}


