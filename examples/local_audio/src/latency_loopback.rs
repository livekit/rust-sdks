mod audio_capture;
#[allow(dead_code)]
mod db_meter;

use anyhow::{anyhow, Result};
use audio_capture::{AudioCapture, CapturedAudioChunk};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleRate, StreamConfig};
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
    env,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, watch};
use tokio::time::{self, Duration, MissedTickBehavior};

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

#[derive(Default)]
struct LatencyStats {
    count: u64,
    sum_us: u128,
    min_us: u64,
    max_us: u64,
    last_frame_id: Option<u32>,
    missing_frame_ids: u64,
}

impl LatencyStats {
    fn record(&mut self, latency_us: u64, frame_id: Option<u32>) {
        self.count += 1;
        self.sum_us += latency_us as u128;
        self.min_us = if self.count == 1 { latency_us } else { self.min_us.min(latency_us) };
        self.max_us = self.max_us.max(latency_us);

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

    fn avg_us(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            (self.sum_us / self.count as u128) as u64
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    list_devices: bool,

    #[arg(short = 'i', long)]
    input_device: Option<String>,

    #[arg(short, long, default_value_t = 48_000)]
    sample_rate: u32,

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
        }
        buffer.extend_from_slice(&audio_chunk.samples);

        while buffer.len() >= samples_per_10ms {
            let chunk: Vec<i16> = buffer.drain(..samples_per_10ms).collect();
            // Audio callbacks can deliver larger chunks than 10 ms, so the
            // benchmark estimates each emitted frame's capture time by walking
            // forward in 10 ms steps from the oldest chunk timestamp.
            let captured_at_us = oldest_capture_us_in_buffer.unwrap_or_else(unix_time_us_now);
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
            frames_since_metadata = (frames_since_metadata + 1) % metadata_interval_frames;

            oldest_capture_us_in_buffer = Some(captured_at_us.saturating_add(10_000));
        }
    }

    Ok(())
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
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut audio_stream = NativeAudioStream::new(track.rtc_track(), sample_rate as i32, 1);
    let mut stats = LatencyStats::default();
    let mut decoded_frames = 0u64;
    let mut untimed_frames = 0u64;
    let mut summary_interval = time::interval(Duration::from_secs(1));
    summary_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

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
            _ = summary_interval.tick() => {
                info!(
                    "Latency summary: avg={:.2}ms min={:.2}ms max={:.2}ms timed_samples={} decoded_frames={} untimed_frames={} last_frame_id={:?} missing_frame_ids={}",
                    stats.avg_us() as f64 / 1000.0,
                    stats.min_us as f64 / 1000.0,
                    stats.max_us as f64 / 1000.0,
                    stats.count,
                    decoded_frames,
                    untimed_frames,
                    stats.last_frame_id,
                    stats.missing_frame_ids
                );
            }
            frame = audio_stream.next() => {
                let Some(frame) = frame else {
                    break;
                };

                decoded_frames += 1;

                if let Some(metadata) = frame.frame_metadata {
                    if let Some(user_timestamp) = metadata.user_timestamp {
                        let now_us = unix_time_us_now();
                        if now_us < user_timestamp {
                            warn!(
                                "Skipping frame with future timestamp: now_us={} user_timestamp={}",
                                now_us, user_timestamp
                            );
                            continue;
                        }

                        let latency_us = now_us - user_timestamp;
                        stats.record(latency_us, metadata.frame_id);

                    } else {
                        untimed_frames += 1;
                    }
                } else {
                    untimed_frames += 1;
                }
            }
        }
    }

    warn!(
        "Audio stream ended for '{}' track '{}' after {} decoded frames and {} latency samples",
        participant.identity(),
        publication.name(),
        decoded_frames,
        stats.count
    );
}

async fn run_subscriber(
    room: Arc<Room>,
    target_identity: String,
    sample_rate: u32,
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

    let input_config = StreamConfig {
        channels: input_supported_config.channels(),
        sample_rate: SampleRate(args.sample_rate),
        buffer_size: cpal::BufferSize::Default,
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
        shutdown_rx.clone(),
    ));

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
    }

    let _ = shutdown_tx.send(true);
    audio_capture.stop();
    publisher_task.abort();
    subscriber_task.abort();
    let _ = publisher_task.await;
    let _ = subscriber_task.await;
    publisher_room.close().await?;
    subscriber_room.close().await?;
    Ok(())
}
