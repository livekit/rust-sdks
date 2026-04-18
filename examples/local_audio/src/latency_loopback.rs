mod audio_capture;
#[allow(dead_code)]
mod db_meter;

use anyhow::{anyhow, Result};
use audio_capture::{AudioCapture, CapturedAudioChunk};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, FromSample, Sample, SampleFormat, SampleRate, SizedSample, Stream,
    StreamConfig, SupportedBufferSize, SupportedStreamConfig,
};
use env_logger::Env;
use futures_util::StreamExt;
use livekit::{
    options::{PacketTrailerFeatures, TrackPublishOptions},
    prelude::*,
    track::{LocalAudioTrack, LocalTrack, TrackSource},
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::native::NativeAudioSource,
        audio_stream::native::NativeAudioStream,
        prelude::{AudioSourceOptions, RtcAudioSource},
        video_frame::FrameMetadata,
    },
    Room, RoomEvent, RoomOptions,
};
use livekit_api::access_token;
use log::{info, warn};
use std::{
    collections::VecDeque,
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, watch};
use tokio::time::{self, Duration, MissedTickBehavior};

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

#[derive(Default)]
struct StageStats {
    count: u64,
    sum_us: u128,
    min_us: u64,
    max_us: u64,
}

impl StageStats {
    fn record(&mut self, latency_us: u64) {
        self.count += 1;
        self.sum_us += latency_us as u128;
        self.min_us = if self.count == 1 { latency_us } else { self.min_us.min(latency_us) };
        self.max_us = self.max_us.max(latency_us);
    }

    fn avg_us(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            (self.sum_us / self.count as u128) as u64
        }
    }
}

#[derive(Default)]
struct FrameTracking {
    last_frame_id: Option<u32>,
    missing_frame_ids: u64,
}

impl FrameTracking {
    fn record(&mut self, frame_id: Option<u32>) {
        if let Some(frame_id) = frame_id {
            if let Some(previous) = self.last_frame_id {
                let gap = frame_id.wrapping_sub(previous);
                if gap > 1 {
                    self.missing_frame_ids += (gap - 1) as u64;
                }
            }
            self.last_frame_id = Some(frame_id);
        }
    }
}

#[derive(Default)]
struct BenchmarkStats {
    capture_to_callback: StageStats,
    callback_to_publish: StageStats,
    capture_to_publish: StageStats,
    capture_to_decoded: StageStats,
    decode_to_playout: StageStats,
    capture_to_playout: StageStats,
    published_frames: u64,
    decoded_frames: u64,
    untimed_decoded_frames: u64,
    played_frames: u64,
    untimed_played_frames: u64,
    frame_tracking: FrameTracking,
}

fn format_stage(stats: &StageStats) -> String {
    if stats.count == 0 {
        "n/a".to_string()
    } else {
        format!(
            "{:.2}/{:.2}/{:.2}ms",
            stats.avg_us() as f64 / 1000.0,
            stats.min_us as f64 / 1000.0,
            stats.max_us as f64 / 1000.0
        )
    }
}

struct PlaybackChunk {
    samples: Vec<i16>,
    offset: usize,
    capture_timestamp_us: Option<u64>,
    decode_callback_us: u64,
}

#[derive(Clone)]
struct PlaybackSender {
    queue: Arc<Mutex<VecDeque<PlaybackChunk>>>,
}

impl PlaybackSender {
    fn push(
        &self,
        samples: Vec<i16>,
        capture_timestamp_us: Option<u64>,
        decode_callback_us: u64,
    ) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(PlaybackChunk {
                samples,
                offset: 0,
                capture_timestamp_us,
                decode_callback_us,
            });
        }
    }
}

struct AudioPlayout {
    _stream: Stream,
    queue: Arc<Mutex<VecDeque<PlaybackChunk>>>,
}

impl AudioPlayout {
    fn sender(&self) -> PlaybackSender {
        PlaybackSender { queue: self.queue.clone() }
    }

    fn new(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        stats: Arc<Mutex<BenchmarkStats>>,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let playout_delay_log_state = Arc::new(AtomicU64::new(u64::MAX));

        let stream = match sample_format {
            SampleFormat::F32 => Self::create_output_stream::<f32>(
                device,
                config,
                queue.clone(),
                stats,
                playout_delay_log_state,
            )?,
            SampleFormat::I16 => Self::create_output_stream::<i16>(
                device,
                config,
                queue.clone(),
                stats,
                playout_delay_log_state,
            )?,
            SampleFormat::U16 => Self::create_output_stream::<u16>(
                device,
                config,
                queue.clone(),
                stats,
                playout_delay_log_state,
            )?,
            sample_format => {
                return Err(anyhow!("Unsupported output sample format: {:?}", sample_format));
            }
        };

        stream.play()?;

        Ok(Self { _stream: stream, queue })
    }

    fn create_output_stream<T>(
        device: Device,
        config: StreamConfig,
        queue: Arc<Mutex<VecDeque<PlaybackChunk>>>,
        stats: Arc<Mutex<BenchmarkStats>>,
        playout_delay_log_state: Arc<AtomicU64>,
    ) -> Result<Stream>
    where
        T: SizedSample + Sample + Send + 'static + FromSample<f32>,
    {
        let sample_rate = config.sample_rate.0;
        let channels = config.channels as usize;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let timestamp = info.timestamp();
                let callback_lead = timestamp
                    .playback
                    .duration_since(&timestamp.callback)
                    .unwrap_or(Duration::ZERO);
                let callback_lead_us = callback_lead.as_micros() as u64;

                if playout_delay_log_state
                    .compare_exchange(
                        u64::MAX,
                        callback_lead_us,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    info!("Audio playout callback lead: {:.2} ms", callback_lead_us as f64 / 1000.0);
                }

                let callback_wall_clock_us = unix_time_us_now();
                let base_playout_us = callback_wall_clock_us.saturating_add(callback_lead_us);

                let mut queue = match queue.lock() {
                    Ok(queue) => queue,
                    Err(_) => {
                        for sample in data.iter_mut() {
                            *sample = T::from_sample(0.0f32);
                        }
                        return;
                    }
                };

                for frame_index in 0..(data.len() / channels) {
                    let mut mono_sample = 0i16;

                    while let Some(front) = queue.front() {
                        if front.offset >= front.samples.len() {
                            queue.pop_front();
                        } else {
                            break;
                        }
                    }

                    if let Some(front) = queue.front_mut() {
                        if front.offset == 0 {
                            let playout_us = base_playout_us.saturating_add(
                                (frame_index as u64 * 1_000_000) / sample_rate as u64,
                            );
                            if let Ok(mut stats) = stats.lock() {
                                stats.played_frames += 1;
                                stats
                                    .decode_to_playout
                                    .record(playout_us.saturating_sub(front.decode_callback_us));
                                if let Some(capture_timestamp_us) = front.capture_timestamp_us {
                                    stats
                                        .capture_to_playout
                                        .record(playout_us.saturating_sub(capture_timestamp_us));
                                } else {
                                    stats.untimed_played_frames += 1;
                                }
                            }
                        }

                        mono_sample = front.samples[front.offset];
                        front.offset += 1;
                    }

                    let converted = T::from_sample(mono_sample as f32 / i16::MAX as f32);
                    let frame_start = frame_index * channels;
                    let frame_end = frame_start + channels;
                    for sample in &mut data[frame_start..frame_end] {
                        *sample = converted;
                    }
                }
            },
            move |err| {
                warn!("Audio output stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    list_devices: bool,

    #[arg(short = 'i', long)]
    input_device: Option<String>,

    #[arg(short = 'o', long)]
    output_device: Option<String>,

    #[arg(short, long, default_value_t = 48_000)]
    sample_rate: u32,

    #[arg(long, default_value_t = 256)]
    output_buffer_frames: u32,

    /// Attach packet-trailer metadata every N milliseconds while still sending exact 10 ms audio frames.
    /// 20 ms matches the default Opus packetization cadence and avoids trailer metadata backlog.
    #[arg(long, default_value_t = 20)]
    metadata_interval_ms: u32,

    #[arg(long, default_value_t = 0)]
    channel: u32,

    #[arg(long, default_value = "audio-room")]
    room_name: String,

    #[arg(long, default_value = "rust-audio-latency-publisher")]
    publisher_identity: String,

    #[arg(long, default_value = "rust-audio-latency-subscriber")]
    subscriber_identity: String,

    #[arg(long, default_value = "latency-microphone")]
    track_name: String,

    #[arg(long)]
    url: Option<String>,

    #[arg(long)]
    api_key: Option<String>,

    #[arg(long)]
    api_secret: Option<String>,
}

fn list_audio_devices() -> Result<()> {
    let host = cpal::default_host();
    println!("Available audio input devices:");
    println!("─────────────────────────────");

    for (index, device) in host.input_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("{}. {}", index + 1, name);
        if let Ok(config) = device.default_input_config() {
            println!("   └─ Sample rate: {} Hz", config.sample_rate().0);
            println!("   └─ Channels: {}", config.channels());
            println!("   └─ Sample format: {:?}", config.sample_format());
        }
        println!();
    }

    if let Some(device) = host.default_input_device() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("Default input device: {}", name);
    }

    println!("\nAvailable audio output devices:");
    println!("─────────────────────────────");

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("{}. {}", index + 1, name);
        if let Ok(config) = device.default_output_config() {
            println!("   └─ Sample rate: {} Hz", config.sample_rate().0);
            println!("   └─ Channels: {}", config.channels());
            println!("   └─ Sample format: {:?}", config.sample_format());
        }
        println!();
    }

    if let Some(device) = host.default_output_device() {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        println!("Default output device: {}", name);
    }

    Ok(())
}

fn find_input_device_by_name(name: &str) -> Result<Device> {
    let host = cpal::default_host();
    for device in host.input_devices()? {
        if let Ok(device_name) = device.name() {
            if device_name.contains(name) {
                return Ok(device);
            }
        }
    }

    Err(anyhow!("Input device '{}' not found", name))
}

fn find_output_device_by_name(name: &str) -> Result<Device> {
    let host = cpal::default_host();
    for device in host.output_devices()? {
        if let Ok(device_name) = device.name() {
            if device_name.contains(name) {
                return Ok(device);
            }
        }
    }

    Err(anyhow!("Output device '{}' not found", name))
}

fn requested_output_buffer_size(
    supported_config: &SupportedStreamConfig,
    requested_frames: u32,
) -> BufferSize {
    match supported_config.buffer_size() {
        SupportedBufferSize::Range { min, max } => BufferSize::Fixed(requested_frames.clamp(*min, *max)),
        SupportedBufferSize::Unknown => BufferSize::Default,
    }
}

fn build_token(api_key: &str, api_secret: &str, identity: &str, room_name: &str) -> Result<String> {
    Ok(access_token::AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            ..Default::default()
        })
        .to_jwt()?)
}

async fn connect_room(
    url: &str,
    token: &str,
    identity: &str,
    auto_subscribe: bool,
) -> Result<Room> {
    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = auto_subscribe;
    info!("Connecting as '{}' (auto_subscribe={})", identity, auto_subscribe);
    let (room, _) = Room::connect(url, token, room_options).await?;
    info!("Connected as '{}' to room '{}' ({})", identity, room.name(), room.sid().await);
    Ok(room)
}

async fn stream_audio_to_livekit(
    mut audio_rx: mpsc::UnboundedReceiver<CapturedAudioChunk>,
    livekit_source: NativeAudioSource,
    sample_rate: u32,
    metadata_interval_ms: u32,
    stats: Arc<Mutex<BenchmarkStats>>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let samples_per_10ms = (sample_rate / 100) as usize;
    let metadata_interval_frames = metadata_interval_ms / 10;
    if metadata_interval_frames == 0 {
        return Err(anyhow!(
            "metadata_interval_ms={} must be at least 10 and divisible by 10",
            metadata_interval_ms
        ));
    }

    let mut buffer = Vec::<i16>::new();
    let mut next_frame_id: u32 = 1;
    let mut oldest_capture_us_in_buffer: Option<u64> = None;
    let mut oldest_callback_us_in_buffer: Option<u64> = None;
    let mut frames_since_metadata = 0u32;

    loop {
        let audio_chunk = tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
            audio_chunk = audio_rx.recv() => {
                let Some(audio_chunk) = audio_chunk else {
                    break;
                };
                audio_chunk
            }
        };

        if oldest_capture_us_in_buffer.is_none() {
            oldest_capture_us_in_buffer = Some(audio_chunk.captured_at_us);
            oldest_callback_us_in_buffer = Some(audio_chunk.callback_at_us);
        }
        buffer.extend_from_slice(&audio_chunk.samples);

        while buffer.len() >= samples_per_10ms {
            let chunk: Vec<i16> = buffer.drain(..samples_per_10ms).collect();
            // Audio callbacks can deliver larger chunks than 10 ms, so the
            // benchmark estimates each emitted frame's capture time by walking
            // forward in 10 ms steps from the oldest chunk timestamp.
            let captured_at_us = oldest_capture_us_in_buffer.unwrap_or_else(unix_time_us_now);
            let callback_at_us = oldest_callback_us_in_buffer.unwrap_or(captured_at_us);
            let attach_metadata = frames_since_metadata == 0;
            let frame_id = attach_metadata.then_some(next_frame_id);
            if attach_metadata {
                next_frame_id = next_frame_id.wrapping_add(1);
            }

            let audio_frame = AudioFrame {
                data: chunk.into(),
                sample_rate,
                num_channels: 1,
                samples_per_channel: samples_per_10ms as u32,
                timestamp: None,
                frame_metadata: attach_metadata
                    .then(|| FrameMetadata { user_timestamp: Some(captured_at_us), frame_id }),
            };

            livekit_source.capture_frame(&audio_frame).await?;
            let publish_completed_us = unix_time_us_now();
            if let Ok(mut stats) = stats.lock() {
                stats.published_frames += 1;
                stats
                    .capture_to_callback
                    .record(callback_at_us.saturating_sub(captured_at_us));
                stats
                    .callback_to_publish
                    .record(publish_completed_us.saturating_sub(callback_at_us));
                stats
                    .capture_to_publish
                    .record(publish_completed_us.saturating_sub(captured_at_us));
            }
            frames_since_metadata = (frames_since_metadata + 1) % metadata_interval_frames;

            oldest_capture_us_in_buffer = Some(captured_at_us.saturating_add(10_000));
            oldest_callback_us_in_buffer = Some(callback_at_us.saturating_add(10_000));
        }
    }

    Ok(())
}

async fn run_summary_loop(
    stats: Arc<Mutex<BenchmarkStats>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut summary_interval = time::interval(Duration::from_secs(1));
    summary_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            _ = summary_interval.tick() => {
                if let Ok(stats) = stats.lock() {
                    info!(
                        "Latency summary: capture->callback={} callback->publish={} capture->publish={} capture->decoded={} decode->playout={} capture->playout={} published_frames={} decoded_frames={} timed_decoded={} untimed_decoded={} played_frames={} timed_played={} untimed_played={} last_frame_id={:?} missing_frame_ids={}",
                        format_stage(&stats.capture_to_callback),
                        format_stage(&stats.callback_to_publish),
                        format_stage(&stats.capture_to_publish),
                        format_stage(&stats.capture_to_decoded),
                        format_stage(&stats.decode_to_playout),
                        format_stage(&stats.capture_to_playout),
                        stats.published_frames,
                        stats.decoded_frames,
                        stats.capture_to_decoded.count,
                        stats.untimed_decoded_frames,
                        stats.played_frames,
                        stats.capture_to_playout.count,
                        stats.untimed_played_frames,
                        stats.frame_tracking.last_frame_id,
                        stats.frame_tracking.missing_frame_ids
                    );
                }
            }
        }
    }
}

fn is_target_audio_publication(
    participant: &RemoteParticipant,
    publication: &RemoteTrackPublication,
    target_identity: &str,
) -> bool {
    participant.identity().as_str() == target_identity && publication.kind() == TrackKind::Audio
}

fn try_subscribe_publication(
    participant: &RemoteParticipant,
    publication: &RemoteTrackPublication,
    target_identity: &str,
    requested_sid: &mut Option<TrackSid>,
    active_sid: &Option<TrackSid>,
) -> bool {
    if !is_target_audio_publication(participant, publication, target_identity) {
        return false;
    }

    let sid = publication.sid();
    if active_sid.as_ref() == Some(&sid) || requested_sid.as_ref() == Some(&sid) {
        return false;
    }

    info!(
        "Requesting subscription to '{}' track '{}' (sid {}, trailer_features={:?})",
        participant.identity(),
        publication.name(),
        sid,
        publication.packet_trailer_features()
    );
    publication.set_subscribed(true);
    *requested_sid = Some(sid);
    true
}

fn try_subscribe_existing_participant_tracks(
    room: &Room,
    target_identity: &str,
    requested_sid: &mut Option<TrackSid>,
    active_sid: &Option<TrackSid>,
) -> bool {
    room.remote_participants().into_values().any(|participant| {
        participant.track_publications().into_values().any(|publication| {
            try_subscribe_publication(
                &participant,
                &publication,
                target_identity,
                requested_sid,
                active_sid,
            )
        })
    })
}

async fn run_audio_stream(
    participant: RemoteParticipant,
    publication: RemoteTrackPublication,
    track: RemoteAudioTrack,
    sample_rate: u32,
    stats: Arc<Mutex<BenchmarkStats>>,
    playback_sender: PlaybackSender,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut audio_stream = NativeAudioStream::new(track.rtc_track(), sample_rate as i32, 1);

    info!(
        "Receiving audio from '{}' track '{}' (sid {}, trailer_features={:?})",
        participant.identity(),
        publication.name(),
        publication.sid(),
        publication.packet_trailer_features()
    );

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            frame = audio_stream.next() => {
                let Some(frame) = frame else {
                    break;
                };

                let decoded_callback_us = unix_time_us_now();
                let metadata = frame.frame_metadata.clone();

                if let Ok(mut stats) = stats.lock() {
                    stats.decoded_frames += 1;
                    if let Some(metadata) = &metadata {
                        if let Some(user_timestamp) = metadata.user_timestamp {
                            if decoded_callback_us >= user_timestamp {
                                stats
                                    .capture_to_decoded
                                    .record(decoded_callback_us.saturating_sub(user_timestamp));
                                stats.frame_tracking.record(metadata.frame_id);
                            } else {
                                warn!(
                                    "Skipping frame with future timestamp: now_us={} user_timestamp={}",
                                    decoded_callback_us, user_timestamp
                                );
                                continue;
                            }
                        } else {
                            stats.untimed_decoded_frames += 1;
                        }
                    } else {
                        stats.untimed_decoded_frames += 1;
                    }
                }

                playback_sender.push(
                    frame.data.into_owned(),
                    metadata.and_then(|metadata| metadata.user_timestamp),
                    decoded_callback_us,
                );
            }
        }
    }

    warn!(
        "Audio stream ended for '{}' track '{}' after {} decoded frames and {} latency samples",
        participant.identity(),
        publication.name(),
        stats.lock().map(|stats| stats.decoded_frames).unwrap_or_default(),
        stats.lock().map(|stats| stats.capture_to_decoded.count).unwrap_or_default()
    );
}

async fn run_subscriber(
    room: Arc<Room>,
    target_identity: String,
    sample_rate: u32,
    stats: Arc<Mutex<BenchmarkStats>>,
    playback_sender: PlaybackSender,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut room_events = room.subscribe();
    let mut requested_sid: Option<TrackSid> = None;
    let mut active_sid: Option<TrackSid> = None;

    if !try_subscribe_existing_participant_tracks(
        &room,
        &target_identity,
        &mut requested_sid,
        &active_sid,
    ) {
        info!("No published audio track for '{}' yet; waiting for room events", target_identity);
    }

    loop {
        let event = tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break Ok(());
                }
                continue;
            }
            event = room_events.recv() => {
                let Some(event) = event else {
                    return Err(anyhow!("Subscriber room event stream closed"));
                };
                event
            }
        };

        match event {
            RoomEvent::ParticipantConnected(participant)
                if participant.identity().as_str() == target_identity =>
            {
                info!("Target publisher '{}' connected", participant.identity());
                let _ = try_subscribe_existing_participant_tracks(
                    &room,
                    &target_identity,
                    &mut requested_sid,
                    &active_sid,
                );
            }
            RoomEvent::TrackPublished { participant, publication } => {
                if participant.identity().as_str() == target_identity {
                    let _ = try_subscribe_publication(
                        &participant,
                        &publication,
                        &target_identity,
                        &mut requested_sid,
                        &active_sid,
                    );
                }
            }
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                if !is_target_audio_publication(&participant, &publication, &target_identity) {
                    continue;
                }

                let RemoteTrack::Audio(audio_track) = track else {
                    continue;
                };

                active_sid = Some(publication.sid());
                requested_sid = active_sid.clone();

                tokio::spawn(run_audio_stream(
                    participant,
                    publication,
                    audio_track,
                    sample_rate,
                    stats.clone(),
                    playback_sender.clone(),
                    shutdown_rx.clone(),
                ));
            }
            RoomEvent::TrackUnsubscribed { publication, participant, .. } => {
                if participant.identity().as_str() == target_identity
                    && active_sid.as_ref() == Some(&publication.sid())
                {
                    warn!("Target audio track unsubscribed; waiting for it to republish");
                    active_sid = None;
                    requested_sid = None;
                }
            }
            RoomEvent::TrackSubscriptionFailed { participant, track_sid, error }
                if participant.identity().as_str() == target_identity =>
            {
                warn!(
                    "Subscription failed for '{}' sid {}: {:?}",
                    participant.identity(),
                    track_sid,
                    error
                );
                requested_sid = None;
            }
            RoomEvent::ParticipantDisconnected(participant)
                if participant.identity().as_str() == target_identity =>
            {
                warn!("Target publisher '{}' disconnected", participant.identity());
                active_sid = None;
                requested_sid = None;
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    if args.list_devices {
        return list_audio_devices();
    }

    if args.publisher_identity == args.subscriber_identity {
        return Err(anyhow!("--publisher-identity and --subscriber-identity must be different"));
    }
    if args.metadata_interval_ms == 0 || args.metadata_interval_ms % 10 != 0 {
        return Err(anyhow!("--metadata-interval-ms must be a positive multiple of 10"));
    }

    let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok()).expect(
        "LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable",
    );
    let api_key = args.api_key.or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LiveKit API key must be provided via --api-key argument or LIVEKIT_API_KEY environment variable");
    let api_secret = args.api_secret.or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LiveKit API secret must be provided via --api-secret argument or LIVEKIT_API_SECRET environment variable");

    let host = cpal::default_host();
    let input_device = if let Some(device_name) = &args.input_device {
        find_input_device_by_name(device_name)?
    } else {
        host.default_input_device().ok_or_else(|| anyhow!("No default input device available"))?
    };

    let input_supported_config = input_device.default_input_config()?;
    let supported_channels = input_supported_config.channels() as u32;
    if args.channel >= supported_channels {
        return Err(anyhow!(
            "Invalid channel index: {}. Device supports {} channels.",
            args.channel,
            supported_channels
        ));
    }

    let input_device_name = input_device.name().unwrap_or_else(|_| "Unknown".to_string());
    info!(
        "Using input device '{}' at {} Hz (capturing channel {} of {}, attaching metadata every {} ms)",
        input_device_name,
        args.sample_rate,
        args.channel,
        supported_channels,
        args.metadata_interval_ms
    );

    let output_device = if let Some(device_name) = &args.output_device {
        find_output_device_by_name(device_name)?
    } else {
        host.default_output_device().ok_or_else(|| anyhow!("No default output device available"))?
    };
    let output_supported_config = output_device.default_output_config()?;
    let output_buffer_size =
        requested_output_buffer_size(&output_supported_config, args.output_buffer_frames);
    let output_device_name = output_device.name().unwrap_or_else(|_| "Unknown".to_string());
    let output_buffer_description = match output_buffer_size {
        BufferSize::Fixed(frames) => format!("{} frames", frames),
        BufferSize::Default => "default".to_string(),
    };
    info!(
        "Using output device '{}' at {} Hz (playout channels={}, requested buffer={})",
        output_device_name,
        args.sample_rate,
        output_supported_config.channels(),
        output_buffer_description
    );

    let input_config = StreamConfig {
        channels: input_supported_config.channels(),
        sample_rate: SampleRate(args.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };
    let output_config = StreamConfig {
        channels: output_supported_config.channels(),
        sample_rate: SampleRate(args.sample_rate),
        buffer_size: output_buffer_size,
    };

    let publisher_token =
        build_token(&api_key, &api_secret, &args.publisher_identity, &args.room_name)?;
    let subscriber_token =
        build_token(&api_key, &api_secret, &args.subscriber_identity, &args.room_name)?;

    let publisher_room =
        Arc::new(connect_room(&url, &publisher_token, &args.publisher_identity, true).await?);
    let subscriber_room =
        Arc::new(connect_room(&url, &subscriber_token, &args.subscriber_identity, false).await?);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let stats = Arc::new(Mutex::new(BenchmarkStats::default()));
    let audio_playout = AudioPlayout::new(
        output_device,
        output_config,
        output_supported_config.sample_format(),
        stats.clone(),
    )?;
    let playback_sender = audio_playout.sender();

    let livekit_source = NativeAudioSource::new(
        AudioSourceOptions {
            echo_cancellation: false,
            noise_suppression: false,
            auto_gain_control: false,
        },
        args.sample_rate,
        1,
        0,
    );

    let track = LocalAudioTrack::create_audio_track(
        &args.track_name,
        RtcAudioSource::Native(livekit_source.clone()),
    );

    let mut packet_trailer_features = PacketTrailerFeatures::default();
    packet_trailer_features.user_timestamp = true;
    packet_trailer_features.frame_id = true;

    publisher_room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(track),
            TrackPublishOptions {
                source: TrackSource::Microphone,
                packet_trailer_features,
                red: false,
                ..Default::default()
            },
        )
        .await?;

    info!(
        "Published track '{}' from '{}' with packet-trailer user_timestamp + frame_id enabled",
        args.track_name, args.publisher_identity
    );

    let mut subscriber_task = tokio::spawn(run_subscriber(
        subscriber_room.clone(),
        args.publisher_identity.clone(),
        args.sample_rate,
        stats.clone(),
        playback_sender,
        shutdown_rx.clone(),
    ));
    let mut summary_task = tokio::spawn(run_summary_loop(stats.clone(), shutdown_rx.clone()));

    let (audio_tx, audio_rx) = mpsc::unbounded_channel();
    let audio_capture = AudioCapture::new(
        input_device,
        input_config,
        input_supported_config.sample_format(),
        audio_tx,
        None,
        args.channel,
        supported_channels,
    )
    .await?;

    let mut publisher_task = tokio::spawn(stream_audio_to_livekit(
        audio_rx,
        livekit_source,
        args.sample_rate,
        args.metadata_interval_ms,
        stats,
        shutdown_rx.clone(),
    ));

    info!(
        "Loopback latency benchmark is running for room '{}'. Press Ctrl+C to stop.",
        args.room_name
    );

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Stopping loopback latency benchmark");
        }
        result = &mut subscriber_task => {
            result??;
        }
        result = &mut publisher_task => {
            result??;
        }
        _ = &mut summary_task => {}
    }

    let _ = shutdown_tx.send(true);
    audio_capture.stop();
    publisher_task.abort();
    subscriber_task.abort();
    summary_task.abort();
    let _ = publisher_task.await;
    let _ = subscriber_task.await;
    let _ = summary_task.await;
    drop(audio_playout);
    publisher_room.close().await?;
    subscriber_room.close().await?;
    Ok(())
}
