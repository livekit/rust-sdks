use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser};
use livekit_api::access_token::{AccessToken, VideoGrants};
use log::{debug, info, warn};
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Level, Profile, RateControlMode,
};
use openh264::formats::YUVSlices;
use openh264::OpenH264API;
use serde::Deserialize;
use std::env;
use std::time::Duration;
use tokio::time::{interval, sleep, Instant, MissedTickBehavior};
use url::Url;
use web_transport_quinn::http::header::AUTHORIZATION;
use web_transport_quinn::http::HeaderValue;
use web_transport_quinn::proto::ConnectRequest;

mod test_pattern;
mod timestamp_burn;

use test_pattern::TestPattern;
use timestamp_burn::TextBurner;

const MOQ_LITE_PROTOCOL: &str = "moq-lite-04";
const MOQ_LITE_STREAM_GROUP: u64 = 0;
const MOQ_LITE_STREAM_PUBLISH_CONTROL: u64 = 6;
const MOQ_LITE_MAX_CONTROL_BYTES: usize = 1024 * 1024;

#[derive(Parser, Debug)]
#[command(author, version, about = "Experimental MoQ H264 publisher for LiveKit SFU ingest")]
struct Args {
    /// MoQ WebTransport URL, usually https://host:port/moq/v1.
    #[arg(long, default_value = "https://127.0.0.1:7880/moq/v1")]
    url: Url,

    /// Existing LiveKit JWT. If omitted, --api-key/--api-secret or env vars are used.
    #[arg(long)]
    token: Option<String>,

    /// LiveKit API key, or LIVEKIT_API_KEY.
    #[arg(long)]
    api_key: Option<String>,

    /// LiveKit API secret, or LIVEKIT_API_SECRET.
    #[arg(long)]
    api_secret: Option<String>,

    /// LiveKit room name.
    #[arg(long, default_value = "video-room")]
    room: String,

    /// LiveKit participant identity for the synthetic publisher.
    #[arg(long, default_value = "rust-moq-pub")]
    identity: String,

    /// LiveKit track name to publish.
    #[arg(long, default_value = "camera")]
    track: String,

    /// Encoded video width. Must be even for I420/H264.
    #[arg(long, default_value_t = 640)]
    width: u32,

    /// Encoded video height. Must be even for I420/H264.
    #[arg(long, default_value_t = 480)]
    height: u32,

    /// Video frame rate.
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Target H264 bitrate in bits per second.
    #[arg(long, default_value_t = 1_500_000)]
    bitrate: u32,

    /// Force an IDR every N frames. Set 0 to rely only on reconnect-triggered IDRs.
    #[arg(long, default_value_t = 60)]
    idr_interval: u32,

    /// Enable reconnect/resume. Use --resume=false to force fresh sessions.
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    resume: bool,

    /// SHA-256 certificate hash accepted for the WebTransport server. May repeat.
    #[arg(long = "cert-hash")]
    cert_hashes: Vec<String>,

    /// Disable TLS certificate verification for local testing.
    #[arg(long, default_value_t = false)]
    tls_disable_verify: bool,

    /// Send auth as access_token query parameter instead of Authorization bearer header.
    #[arg(long, default_value_t = false)]
    auth_query_param: bool,

    /// Delay before reconnect attempts after transport loss.
    #[arg(long, default_value_t = 500)]
    reconnect_delay_ms: u64,

    /// Optional run duration. The default of 0 runs until Ctrl-C.
    #[arg(long, default_value_t = 0)]
    duration_seconds: u64,
}

#[derive(Clone, Debug, Default)]
struct ResumeState {
    enabled: bool,
    resume_token: Option<String>,
    track_sid: Option<String>,
    next_sequence: u64,
}

impl ResumeState {
    fn clear(&mut self) {
        self.resume_token = None;
        self.track_sid = None;
        self.next_sequence = 0;
    }

    fn can_resume(&self) -> bool {
        self.enabled && self.resume_token.is_some() && self.track_sid.is_some()
    }

    fn apply_control(&mut self, control: &PublishControlResponse) {
        if !self.enabled {
            self.clear();
            return;
        }
        self.resume_token = Some(control.resume_token.clone());
        self.track_sid = Some(control.track_sid.clone());
        self.next_sequence = self.next_sequence.max(control.next_sequence);
    }

    fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    fn mark_sequence_sent(&mut self, sequence: u64) {
        if sequence >= self.next_sequence {
            self.next_sequence = sequence + 1;
        }
    }
}

#[derive(Debug, Deserialize)]
struct PublishControlResponse {
    publish_session_id: String,
    resume_token: String,
    track_sid: String,
    next_sequence: u64,
    resume_deadline_ms: i64,
}

struct H264AnnexBSource {
    width: u32,
    height: u32,
    chroma_width: usize,
    y_plane: Vec<u8>,
    u_plane: Vec<u8>,
    v_plane: Vec<u8>,
    pattern: TestPattern,
    overlay: TextBurner,
    encoder: Encoder,
    frame_id: u64,
    idr_interval: u32,
    force_next_idr: bool,
}

impl H264AnnexBSource {
    fn new(args: &Args) -> Result<Self> {
        if args.width == 0 || args.height == 0 {
            bail!("width and height must be non-zero");
        }
        if args.width % 2 != 0 || args.height % 2 != 0 {
            bail!("width and height must be even for I420/H264");
        }
        if args.fps == 0 {
            bail!("fps must be non-zero");
        }

        let width = args.width as usize;
        let height = args.height as usize;
        let chroma_width = width / 2;
        let chroma_height = height / 2;
        let encoder_config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(args.bitrate))
            .max_frame_rate(FrameRate::from_hz(args.fps as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            .profile(Profile::Baseline)
            .level(Level::Level_3_1)
            .intra_frame_period(IntraFramePeriod::from_num_frames(args.idr_interval))
            .skip_frames(false);
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), encoder_config)
            .context("create OpenH264 encoder")?;

        Ok(Self {
            width: args.width,
            height: args.height,
            chroma_width,
            y_plane: vec![0; width * height],
            u_plane: vec![128; chroma_width * chroma_height],
            v_plane: vec![128; chroma_width * chroma_height],
            pattern: TestPattern::new(args.width, args.height),
            overlay: TextBurner::new_top_left(args.width, args.height, 2),
            encoder,
            frame_id: 0,
            idr_interval: args.idr_interval,
            force_next_idr: false,
        })
    }

    fn force_idr(&mut self) {
        self.force_next_idr = true;
    }

    fn next_access_unit(&mut self) -> Result<Vec<u8>> {
        let width = self.width as usize;
        let height = self.height as usize;
        self.pattern.render(
            &mut self.y_plane,
            width as i32,
            &mut self.u_plane,
            self.chroma_width as i32,
            &mut self.v_plane,
            self.chroma_width as i32,
        );
        let overlay = format!("MOQ {}", self.frame_id);
        self.overlay.draw_lines(&mut self.y_plane, width, &[overlay.as_str()]);

        if self.frame_id > 0
            && (self.force_next_idr
                || (self.idr_interval != 0 && self.frame_id % u64::from(self.idr_interval) == 0))
        {
            self.encoder.force_intra_frame();
        }
        self.force_next_idr = false;

        let yuv = YUVSlices::new(
            (&self.y_plane, &self.u_plane, &self.v_plane),
            (width, height),
            (width, self.chroma_width, self.chroma_width),
        );
        let au = self.encoder.encode(&yuv).context("encode H264 frame")?.to_vec();
        self.frame_id += 1;
        if au.is_empty() {
            bail!("OpenH264 returned an empty access unit");
        }
        Ok(au)
    }

    fn frame_id(&self) -> u64 {
        self.frame_id
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let token = resolve_token(&args)?;
    let client = build_webtransport_client(&args)?;
    let mut source = H264AnnexBSource::new(&args)?;
    let mut resume = ResumeState { enabled: args.resume, ..Default::default() };
    let stop_at = stop_deadline(args.duration_seconds);

    loop {
        let result =
            run_publish_generation(&args, &token, &client, &mut source, &mut resume, stop_at).await;
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                warn!("MoQ publish generation ended: {err:?}");
                if !resume.enabled {
                    resume.clear();
                }
                if deadline_elapsed(stop_at) {
                    return Ok(());
                }
                sleep(Duration::from_millis(args.reconnect_delay_ms)).await;
            }
        }
    }
}

async fn run_publish_generation(
    args: &Args,
    token: &str,
    client: &web_transport_quinn::Client,
    source: &mut H264AnnexBSource,
    resume: &mut ResumeState,
    stop_at: Option<Instant>,
) -> Result<()> {
    let publish_url = build_publish_url(args, token, resume)?;
    let mut request = ConnectRequest::new(publish_url.clone()).with_protocol(moq_lite_protocol());
    if !args.auth_query_param {
        request = request.with_header(AUTHORIZATION, bearer_header(token)?);
    }

    info!(
        "connecting MoQ publisher url={} resume={} next_sequence={}",
        publish_url,
        resume.can_resume(),
        resume.next_sequence()
    );
    let attempted_resume = resume.can_resume();
    let session = match client.connect(request).await.context("connect WebTransport") {
        Ok(session) => session,
        Err(err) => {
            if attempted_resume {
                warn!("resume connection failed; clearing resume state for a fresh publish");
                resume.clear();
            }
            return Err(err);
        }
    };
    let control = match read_publish_control(&session).await.context("read publish control") {
        Ok(control) => control,
        Err(err) => {
            if attempted_resume {
                warn!("resume was not accepted; clearing resume state for a fresh publish");
                resume.clear();
            }
            return Err(err);
        }
    };
    resume.apply_control(&control);
    source.force_idr();
    info!(
        "MoQ publish session={} track_sid={} next_sequence={} resume_deadline_ms={}",
        control.publish_session_id,
        control.track_sid,
        resume.next_sequence(),
        control.resume_deadline_ms
    );

    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(args.fps));
    let mut ticker = interval(frame_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received; closing MoQ publisher");
                session.close(0, b"client shutdown");
                return Ok(());
            }
            _ = ticker.tick() => {
                if deadline_elapsed(stop_at) {
                    session.close(0, b"duration elapsed");
                    return Ok(());
                }
                let sequence = resume.next_sequence();
                let access_unit = source.next_access_unit()?;
                send_access_unit(&session, sequence, &access_unit).await?;
                resume.mark_sequence_sent(sequence);
                debug!(
                    "sent moq H264 access unit sequence={} bytes={} frame_id={}",
                    sequence,
                    access_unit.len(),
                    source.frame_id()
                );
            }
        }
    }
}

fn resolve_token(args: &Args) -> Result<String> {
    if let Some(token) = args.token.clone().or_else(|| env::var("LIVEKIT_TOKEN").ok()) {
        return Ok(token);
    }
    let api_key = args
        .api_key
        .clone()
        .or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .context("provide --token, LIVEKIT_TOKEN, or --api-key/LIVEKIT_API_KEY")?;
    let api_secret = args
        .api_secret
        .clone()
        .or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .context("provide --token, LIVEKIT_TOKEN, or --api-secret/LIVEKIT_API_SECRET")?;

    AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(VideoGrants {
            room_join: true,
            room: args.room.clone(),
            can_publish: true,
            can_subscribe: false,
            can_publish_sources: vec!["camera".to_string()],
            ..Default::default()
        })
        .to_jwt()
        .context("mint LiveKit token")
}

fn build_webtransport_client(args: &Args) -> Result<web_transport_quinn::Client> {
    let builder = web_transport_quinn::ClientBuilder::new()
        .with_congestion_control(web_transport_quinn::CongestionControl::LowLatency);
    if args.tls_disable_verify {
        warn!("TLS certificate verification is disabled");
        return builder
            .dangerous()
            .with_no_certificate_verification()
            .context("build insecure WebTransport client");
    }
    if !args.cert_hashes.is_empty() {
        let hashes = args
            .cert_hashes
            .iter()
            .map(|hash| decode_cert_hash(hash))
            .collect::<Result<Vec<_>>>()?;
        return builder
            .with_server_certificate_hashes(hashes)
            .context("build certificate-pinned WebTransport client");
    }
    builder.with_system_roots().context("build WebTransport client")
}

fn decode_cert_hash(hash: &str) -> Result<Vec<u8>> {
    let normalized: String = hash.chars().filter(|ch| *ch != ':' && !ch.is_whitespace()).collect();
    let decoded = hex::decode(&normalized).context("decode certificate hash hex")?;
    if decoded.len() != 32 {
        bail!("certificate hash must be a SHA-256 digest, got {} bytes", decoded.len());
    }
    Ok(decoded)
}

fn bearer_header(token: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(&format!("Bearer {token}")).context("build Authorization header")
}

fn build_publish_url(args: &Args, token: &str, resume: &ResumeState) -> Result<Url> {
    let mut url = args.url.clone();
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("role", "publish")
            .append_pair("room", &args.room)
            .append_pair("track", &args.track)
            .append_pair("width", &args.width.to_string())
            .append_pair("height", &args.height.to_string())
            .append_pair("fps", &args.fps.to_string());
        if args.auth_query_param {
            pairs.append_pair("access_token", token);
        }
        if resume.can_resume() {
            pairs
                .append_pair("resume_token", resume.resume_token.as_deref().unwrap_or_default())
                .append_pair("track_sid", resume.track_sid.as_deref().unwrap_or_default())
                .append_pair("next_sequence", &resume.next_sequence.to_string());
        }
    }
    Ok(url)
}

async fn read_publish_control(
    session: &web_transport_quinn::Session,
) -> Result<PublishControlResponse> {
    let mut stream = session.accept_uni().await.context("accept control stream")?;
    let data =
        stream.read_to_end(MOQ_LITE_MAX_CONTROL_BYTES).await.context("read control stream")?;
    let mut input = data.as_slice();
    let stream_type = read_varint(&mut input)?;
    if stream_type != MOQ_LITE_STREAM_PUBLISH_CONTROL {
        bail!("unexpected publish control stream type: {stream_type}");
    }
    let payload = read_moq_message(&mut input)?;
    if !input.is_empty() {
        bail!("publish control stream has {} trailing bytes", input.len());
    }
    serde_json::from_slice(payload).context("decode publish control JSON")
}

async fn send_access_unit(
    session: &web_transport_quinn::Session,
    sequence: u64,
    access_unit: &[u8],
) -> Result<()> {
    let data = build_publish_group(sequence, access_unit);
    let mut stream = session.open_uni().await.context("open frame stream")?;
    stream.write_all(&data).await.context("write frame stream")?;
    stream.finish().context("finish frame stream")?;
    Ok(())
}

fn build_publish_group(sequence: u64, access_unit: &[u8]) -> Vec<u8> {
    let mut meta = Vec::with_capacity(16);
    append_varint(&mut meta, 0);
    append_varint(&mut meta, sequence);

    let mut data = Vec::with_capacity(1 + meta.len() + access_unit.len() + 16);
    append_varint(&mut data, MOQ_LITE_STREAM_GROUP);
    append_varint(&mut data, meta.len() as u64);
    data.extend_from_slice(&meta);
    append_varint(&mut data, access_unit.len() as u64);
    data.extend_from_slice(access_unit);
    data
}

fn append_varint(dst: &mut Vec<u8>, value: u64) {
    if value < (1 << 6) {
        dst.push(value as u8);
    } else if value < (1 << 14) {
        let encoded = (value as u16) | (0b01 << 14);
        dst.extend_from_slice(&encoded.to_be_bytes());
    } else if value < (1 << 30) {
        let encoded = (value as u32) | (0b10 << 30);
        dst.extend_from_slice(&encoded.to_be_bytes());
    } else if value < (1 << 62) {
        let encoded = value | (0b11u64 << 62);
        dst.extend_from_slice(&encoded.to_be_bytes());
    } else {
        panic!("QUIC varint value out of range: {value}");
    }
}

fn read_varint(input: &mut &[u8]) -> Result<u64> {
    if input.is_empty() {
        bail!("unexpected end of varint");
    }
    let first = input[0];
    let len = 1usize << (first >> 6);
    if input.len() < len {
        bail!("truncated varint");
    }
    let mut value = u64::from(first & 0x3f);
    for byte in &input[1..len] {
        value = (value << 8) | u64::from(*byte);
    }
    *input = &input[len..];
    Ok(value)
}

fn read_moq_message<'a>(input: &mut &'a [u8]) -> Result<&'a [u8]> {
    let size = read_varint(input)? as usize;
    if input.len() < size {
        bail!("truncated moq-lite message");
    }
    let (message, rest) = input.split_at(size);
    *input = rest;
    Ok(message)
}

fn moq_lite_protocol() -> &'static str {
    let version: moq_net::Version = MOQ_LITE_PROTOCOL.parse().expect("known moq-lite version");
    version.alpn()
}

fn stop_deadline(duration_seconds: u64) -> Option<Instant> {
    (duration_seconds != 0).then(|| Instant::now() + Duration::from_secs(duration_seconds))
}

fn deadline_elapsed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_args() -> Args {
        Args {
            url: "https://example.test/moq/v1".parse().unwrap(),
            token: Some("token".to_string()),
            api_key: None,
            api_secret: None,
            room: "room".to_string(),
            identity: "identity".to_string(),
            track: "camera".to_string(),
            width: 640,
            height: 480,
            fps: 30,
            bitrate: 500_000,
            idr_interval: 30,
            resume: true,
            cert_hashes: Vec::new(),
            tls_disable_verify: true,
            auth_query_param: false,
            reconnect_delay_ms: 1,
            duration_seconds: 0,
        }
    }

    #[test]
    fn publish_url_includes_resume_state_when_available() {
        let args = test_args();
        let resume = ResumeState {
            enabled: true,
            resume_token: Some("resume".to_string()),
            track_sid: Some("TR_test".to_string()),
            next_sequence: 42,
        };

        let url = build_publish_url(&args, "token", &resume).unwrap();
        let query = url.query().unwrap();

        assert!(query.contains("role=publish"));
        assert!(query.contains("room=room"));
        assert!(query.contains("track=camera"));
        assert!(query.contains("resume_token=resume"));
        assert!(query.contains("track_sid=TR_test"));
        assert!(query.contains("next_sequence=42"));
    }

    #[test]
    fn resume_state_advances_sequences_monotonically() {
        let mut resume = ResumeState { enabled: true, next_sequence: 7, ..Default::default() };

        assert_eq!(resume.next_sequence(), 7);
        resume.mark_sequence_sent(7);
        assert_eq!(resume.next_sequence(), 8);
        resume.mark_sequence_sent(3);
        assert_eq!(resume.next_sequence(), 8);
        resume.mark_sequence_sent(10);
        assert_eq!(resume.next_sequence(), 11);
    }

    #[test]
    fn publish_group_carries_sequence_and_access_unit() {
        let access_unit = [0, 0, 0, 1, 0x65, 1, 2, 3];
        let data = build_publish_group(123, &access_unit);
        let mut input = data.as_slice();

        assert_eq!(read_varint(&mut input).unwrap(), MOQ_LITE_STREAM_GROUP);
        let meta = read_moq_message(&mut input).unwrap();
        let mut meta_input = meta;
        assert_eq!(read_varint(&mut meta_input).unwrap(), 0);
        assert_eq!(read_varint(&mut meta_input).unwrap(), 123);
        assert!(meta_input.is_empty());
        assert_eq!(read_moq_message(&mut input).unwrap(), access_unit);
        assert!(input.is_empty());
    }

    #[test]
    fn h264_source_produces_annex_b_and_advances_frame_id() {
        let mut args = test_args();
        args.width = 64;
        args.height = 48;
        args.bitrate = 120_000;
        let mut source = H264AnnexBSource::new(&args).unwrap();

        let first = source.next_access_unit().unwrap();
        let second = source.next_access_unit().unwrap();

        assert!(first.windows(4).any(|window| window == [0, 0, 0, 1]));
        assert!(second.windows(4).any(|window| window == [0, 0, 0, 1]));
        assert_eq!(source.frame_id(), 2);
    }

    #[test]
    fn cert_hash_accepts_colon_delimited_sha256() {
        let hash = "00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99:aa:bb:cc:dd:ee:ff";
        assert_eq!(decode_cert_hash(hash).unwrap().len(), 32);
    }
}
