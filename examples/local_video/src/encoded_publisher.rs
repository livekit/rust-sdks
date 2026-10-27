//! Publishes a pre-encoded H.264/H.265 stream received over TCP, without
//! re-encoding.
//!
//! The stream is length-prefixed NAL units (`avc3`/`hev1` framing, parameter
//! sets in-band), as produced by hardware encoder bridges. Access units are
//! assembled with `livekit-capture`'s `LengthPrefixedAccessUnitParser` and fed
//! to an `EncodedVideoPump`.

use anyhow::Result;
use clap::{Parser, ValueEnum};
use livekit::options::{FrameMetadataFeatures, TrackPublishOptions};
use livekit::prelude::*;
use livekit_api::access_token;
use livekit_capture::{
    encoded::{
        h26x::{frame_metadata_from_sei, LengthPrefixedAccessUnitParser},
        EncodedVideoCodec, EncodedVideoPump, EncodedVideoSource, OwnedEncodedAccessUnit,
    },
    error::SourceError,
    primitive::VideoResolution,
    pump::PumpStop,
};
use log::info;
use std::env;
use std::io::{ErrorKind, Read};
use std::net::TcpStream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Codec {
    H264,
    H265,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// TCP address of the encoder's length-prefixed stream
    #[arg(long, default_value = "127.0.0.1:5004")]
    src: String,

    /// Codec of the stream
    #[arg(long, value_enum, default_value_t = Codec::H265)]
    codec: Codec,

    /// Size of each NAL length prefix in bytes
    #[arg(long, default_value_t = 4)]
    nal_length_size: u8,

    /// Publish each picture as soon as its slice arrives (single-slice streams)
    #[arg(long, default_value_t = false)]
    single_slice_flush: bool,

    /// Attach the user timestamp and frame id from an LKTS SEI as packet trailers
    #[arg(long, default_value_t = false)]
    packet_trailer: bool,

    /// Stream width
    #[arg(long, default_value_t = 1920)]
    width: u32,

    /// Stream height
    #[arg(long, default_value_t = 1080)]
    height: u32,

    /// Log the sender's pacer delay and bandwidth estimate every 2 s
    #[arg(long, default_value_t = false)]
    stats: bool,

    /// LiveKit participant identity
    #[arg(long, default_value = "encoded-publisher")]
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
}

/// Reads length-prefixed access units from a TCP stream.
///
/// The connection is opened on the first read, i.e. once the pump runs
/// after the track is published, so no frames queue up in the socket while
/// the publication is negotiated.
struct TcpLengthPrefixedSource {
    addr: String,
    stream: Option<TcpStream>,
    parser: LengthPrefixedAccessUnitParser,
    codec: EncodedVideoCodec,
    resolution: VideoResolution,
    buf: Vec<u8>,
    eof: bool,
}

impl TcpLengthPrefixedSource {
    fn stream(&mut self) -> Result<&mut TcpStream, SourceError> {
        if self.stream.is_none() {
            let stream = TcpStream::connect(&self.addr).map_err(SourceError::new)?;
            stream.set_nodelay(true).map_err(SourceError::new)?;
            // Bounded reads so a stop request is noticed promptly.
            stream.set_read_timeout(Some(Duration::from_millis(100))).map_err(SourceError::new)?;
            info!("Connected to encoder at {}", self.addr);
            self.stream = Some(stream);
        }
        Ok(self.stream.as_mut().expect("TCP stream initialized above"))
    }
}

fn now_us() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as i64
}

impl EncodedVideoSource for TcpLengthPrefixedSource {
    fn resolution(&self) -> VideoResolution {
        self.resolution
    }

    fn codec(&self) -> EncodedVideoCodec {
        self.codec
    }

    fn next_access_unit(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<OwnedEncodedAccessUnit>, SourceError> {
        // Drain access units already buffered before reading more.
        let mut bytes_read = 0;
        loop {
            let unit = if self.eof {
                self.parser.flush()
            } else {
                self.parser.push(&self.buf[..bytes_read])
            }
            .map_err(SourceError::new)?;
            if let Some(mut unit) = unit {
                // Stamp the capture time on arrival rather than the
                // parser's nominal frame clock.
                unit.timestamp_us = now_us();
                return Ok(Some(unit));
            }
            if self.eof || stop.is_stopped() {
                return Ok(None);
            }

            let mut buf = std::mem::take(&mut self.buf);
            let read = self.stream()?.read(&mut buf);
            self.buf = buf;
            bytes_read = match read {
                Ok(0) => {
                    self.eof = true;
                    0
                }
                Ok(n) => n,
                Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => 0,
                Err(err) if err.kind() == ErrorKind::Interrupted => 0,
                Err(err) => return Err(SourceError::new(err)),
            };
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

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
            can_publish: Some(true),
            can_subscribe: Some(false),
            ..Default::default()
        })
        .to_jwt()?;

    info!("Connecting to LiveKit room '{}' as '{}'...", args.room_name, args.identity);
    let (room, _events) = Room::connect(&url, &token, RoomOptions::default()).await?;

    let codec = match args.codec {
        Codec::H264 => EncodedVideoCodec::H264,
        Codec::H265 => EncodedVideoCodec::H265,
    };
    let resolution = VideoResolution::new(args.width, args.height);
    let parser =
        LengthPrefixedAccessUnitParser::new(codec, args.nal_length_size, 0, 0, resolution)?
            .with_single_slice_flush(args.single_slice_flush);
    let source = TcpLengthPrefixedSource {
        addr: args.src.clone(),
        stream: None,
        parser,
        codec,
        resolution,
        buf: vec![0; 256 * 1024],
        eof: false,
    };

    let mut pump = EncodedVideoPump::new(source);
    if args.packet_trailer {
        pump = pump.with_frame_metadata(move |unit| frame_metadata_from_sei(codec, &unit.payload));
    }

    let track = LocalVideoTrack::create_video_track("encoded", pump.rtc_source());
    let mut frame_metadata_features = FrameMetadataFeatures::default();
    frame_metadata_features.user_timestamp = args.packet_trailer;
    frame_metadata_features.frame_id = args.packet_trailer;
    let publish_options = TrackPublishOptions {
        source: TrackSource::Camera,
        frame_metadata_features,
        ..pump.publish_options()
    };
    room.local_participant()
        .publish_track(LocalTrack::Video(track.clone()), publish_options)
        .await?;
    info!(
        "Published {:?} track from {} (single-slice flush: {}, packet trailer: {})",
        args.codec, args.src, args.single_slice_flush, args.packet_trailer
    );

    let stats_task = args.stats.then(|| tokio::spawn(log_sender_stats(track)));

    let running = pump.spawn()?;
    let stop = running.stop_handle();
    let join = running.join();
    tokio::pin!(join);
    let stats = tokio::select! {
        stats = &mut join => stats,
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl-C received, exiting...");
            stop.stop();
            join.await
        }
    }?;
    info!("Encoder stream ended: {:?}", stats);
    if let Some(task) = stats_task {
        task.abort();
        let _ = task.await;
    }
    room.close().await?;
    Ok(())
}

/// Logs, from standard WebRTC stats, the negotiated codec and frames sent,
/// how long packets wait in the pacer (`totalPacketSendDelay` /
/// `packetsSent` over the interval), the sender's target bitrate and the
/// bandwidth estimate.
async fn log_sender_stats(track: LocalVideoTrack) {
    use livekit::webrtc::stats::RtcStats;

    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let (mut last_delay, mut last_packets) = (0.0, 0);
    loop {
        interval.tick().await;
        let Ok(stats) = track.get_stats().await else { continue };
        let mut available_outgoing_bitrate = 0.0;
        let mut outbound = None;
        let mut codecs = Vec::new();
        for stat in stats {
            match stat {
                RtcStats::OutboundRtp(rtp) if rtp.stream.kind == "video" => outbound = Some(rtp),
                RtcStats::Codec(codec) => codecs.push(codec),
                RtcStats::CandidatePair(pair) if pair.candidate_pair.nominated => {
                    available_outgoing_bitrate = pair.candidate_pair.available_outgoing_bitrate;
                }
                _ => {}
            }
        }
        let Some(rtp) = outbound else { continue };
        let codec = codecs
            .iter()
            .find(|codec| codec.rtc.id == rtp.stream.codec_id)
            .map_or("unknown", |codec| codec.codec.mime_type.as_str());
        let delay = rtp.outbound.total_packet_send_delay;
        let packets = rtp.sent.packets_sent;
        if packets > last_packets {
            info!(
                "{codec}, {} frames sent, pacer delay {:.2} ms/packet over {} packets, \
                 target bitrate {:.0} kbps, available outgoing bitrate {:.0} kbps",
                rtp.outbound.frames_sent,
                (delay - last_delay) * 1000.0 / (packets - last_packets) as f64,
                packets - last_packets,
                rtp.outbound.target_bitrate / 1000.0,
                available_outgoing_bitrate / 1000.0,
            );
        } else {
            info!(
                "{codec}, {} frames sent, no packets sent in the last 2 s",
                rtp.outbound.frames_sent
            );
        }
        (last_delay, last_packets) = (delay, packets);
    }
}
