use anyhow::{anyhow, Result};
use clap::Parser;
use env_logger::Env;
use futures_util::StreamExt;
use livekit::{
    prelude::*, webrtc::audio_stream::native::NativeAudioStream, Room, RoomEvent, RoomOptions,
};
use livekit_api::access_token;
use log::{info, warn};
use std::{
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

#[derive(Default)]
struct LatencyStats {
    count: u64,
    sum_us: u128,
    min_us: u64,
    max_us: u64,
}

impl LatencyStats {
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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// LiveKit participant identity
    #[arg(long, default_value = "rust-audio-latency-subscriber")]
    identity: String,

    /// Remote participant identity to subscribe to
    #[arg(long)]
    participant: String,

    /// LiveKit room name
    #[arg(long, default_value = "audio-room")]
    room_name: String,

    /// Desired sample rate for the received PCM stream
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,

    /// Desired channel count for the received PCM stream
    #[arg(long, default_value_t = 1)]
    channels: u32,

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
        "Requesting audio subscription for '{}' track '{}' (sid {}, trailer_features={:?})",
        participant.identity(),
        publication.name(),
        sid,
        publication.packet_trailer_features(),
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
    channels: u32,
) {
    let advertised_timestamp =
        publication.packet_trailer_features().contains(&PacketTrailerFeature::PtfUserTimestamp);

    if !advertised_timestamp {
        warn!(
            "Track '{}' from '{}' does not advertise PTF_USER_TIMESTAMP; waiting to see if timestamps still arrive",
            publication.name(),
            participant.identity()
        );
    }

    let mut audio_stream =
        NativeAudioStream::new(track.rtc_track(), sample_rate as i32, channels as i32);
    let packet_trailer_handler = track.packet_trailer_handler();
    let mut stats = LatencyStats::default();
    let mut decoded_frames = 0u64;
    let mut last_timed_frame_at = tokio::time::Instant::now();
    let mut untimed_frames = 0u64;
    let mut timed_frames = 0u64;

    info!(
        "Receiving audio from '{}' track '{}' (sid {}, advertised_timestamp={}, handler_available={}, sample_rate={}Hz, channels={})",
        participant.identity(),
        publication.name(),
        publication.sid(),
        advertised_timestamp,
        packet_trailer_handler.is_some(),
        sample_rate,
        channels
    );

    while let Some(frame) = audio_stream.next().await {
        decoded_frames += 1;
        if let Some(publish_us) = frame.frame_metadata.and_then(|metadata| metadata.user_timestamp)
        {
            timed_frames += 1;
            let now_us = unix_time_us_now();
            if now_us < publish_us {
                warn!(
                    "Skipping future packet-trailer timestamp from '{}' track '{}': publish_us={} now_us={}",
                    participant.identity(),
                    publication.name(),
                    publish_us,
                    now_us
                );
                continue;
            }

            let latency_us = now_us - publish_us;
            stats.record(latency_us);
            last_timed_frame_at = tokio::time::Instant::now();

            if stats.count == 1 || stats.count % 50 == 0 {
                info!(
                    "Latency from '{}' track '{}': last={:.2}ms avg={:.2}ms min={:.2}ms max={:.2}ms samples={}",
                    participant.identity(),
                    publication.name(),
                    latency_us as f64 / 1000.0,
                    stats.avg_us() as f64 / 1000.0,
                    stats.min_us as f64 / 1000.0,
                    stats.max_us as f64 / 1000.0,
                    stats.count
                );
            }
        } else {
            untimed_frames += 1;
            if decoded_frames == 1
                || (decoded_frames % 200 == 0
                    && last_timed_frame_at.elapsed() >= Duration::from_millis(500))
            {
                if packet_trailer_handler.is_some() {
                    info!(
                        "Audio timing coverage for '{}' track '{}': timed={} untimed={} decoded={}",
                        participant.identity(),
                        publication.name(),
                        timed_frames,
                        untimed_frames,
                        decoded_frames
                    );
                } else {
                    warn!(
                        "Decoded {} frame(s) from '{}' track '{}' without an attached packet-trailer handler",
                        decoded_frames,
                        participant.identity(),
                        publication.name()
                    );
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let url = args.url.or_else(|| env::var("LIVEKIT_URL").ok()).expect(
        "LiveKit URL must be provided via --url argument or LIVEKIT_URL environment variable",
    );
    let api_key = args.api_key.or_else(|| env::var("LIVEKIT_API_KEY").ok())
        .expect("LiveKit API key must be provided via --api-key argument or LIVEKIT_API_KEY environment variable");
    let api_secret = args.api_secret.or_else(|| env::var("LIVEKIT_API_SECRET").ok())
        .expect("LiveKit API secret must be provided via --api-secret argument or LIVEKIT_API_SECRET environment variable");

    let token = access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(&args.identity)
        .with_name(&args.identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: args.room_name.clone(),
            ..Default::default()
        })
        .to_jwt()?;

    let mut room_options = RoomOptions::default();
    room_options.auto_subscribe = false;

    info!(
        "Connecting to LiveKit room '{}' as '{}' and waiting for participant '{}'",
        args.room_name, args.identity, args.participant
    );
    let (room, _) = Room::connect(&url, &token, room_options).await?;
    info!("Connected to room: {} - {}", room.name(), room.sid().await);

    let mut room_events = room.subscribe();
    let mut requested_sid: Option<TrackSid> = None;
    let mut active_sid: Option<TrackSid> = None;

    if !try_subscribe_existing_participant_tracks(
        &room,
        &args.participant,
        &mut requested_sid,
        &active_sid,
    ) {
        info!("No published audio track for '{}' yet; waiting for room events", args.participant);
    }

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down subscriber");
                room.close().await?;
                return Ok(());
            }
            event = room_events.recv() => {
                let Some(event) = event else {
                    return Err(anyhow!("Room event stream closed"));
                };

                match event {
                    RoomEvent::ParticipantConnected(participant)
                        if participant.identity().as_str() == args.participant =>
                    {
                        info!("Target participant '{}' connected", participant.identity());
                        let _ = try_subscribe_existing_participant_tracks(
                            &room,
                            &args.participant,
                            &mut requested_sid,
                            &active_sid,
                        );
                    }
                    RoomEvent::TrackPublished { participant, publication } => {
                        info!(
                            "Observed TrackPublished from '{}' name='{}' kind={:?} sid={} trailer_features={:?}",
                            participant.identity(),
                            publication.name(),
                            publication.kind(),
                            publication.sid(),
                            publication.packet_trailer_features(),
                        );
                        if participant.identity().as_str() == args.participant {
                            let _ = try_subscribe_publication(
                                &participant,
                                &publication,
                                &args.participant,
                                &mut requested_sid,
                                &active_sid,
                            );
                        }
                    }
                    RoomEvent::TrackSubscribed { track, publication, participant } => {
                        info!(
                            "Observed TrackSubscribed from '{}' name='{}' kind={:?} sid={} trailer_features={:?}",
                            participant.identity(),
                            publication.name(),
                            publication.kind(),
                            publication.sid(),
                            publication.packet_trailer_features(),
                        );
                        if !is_target_audio_publication(&participant, &publication, &args.participant) {
                            continue;
                        }

                        let RemoteTrack::Audio(audio_track) = track else {
                            continue;
                        };

                        let sid = publication.sid();
                        if active_sid.as_ref() == Some(&sid) {
                            continue;
                        }

                        active_sid = Some(sid.clone());
                        requested_sid = Some(sid.clone());

                        tokio::spawn(run_audio_stream(
                            participant,
                            publication,
                            audio_track,
                            args.sample_rate,
                            args.channels,
                        ));
                    }
                    RoomEvent::TrackUnsubscribed { track, publication, participant } => {
                        if !is_target_audio_publication(&participant, &publication, &args.participant) {
                            continue;
                        }
                        if active_sid.as_ref() == Some(&publication.sid()) {
                            info!(
                                "Target audio track '{}' unsubscribed from '{}'",
                                track.name(),
                                participant.identity()
                            );
                            active_sid = None;
                            requested_sid = None;
                            let _ = try_subscribe_existing_participant_tracks(
                                &room,
                                &args.participant,
                                &mut requested_sid,
                                &active_sid,
                            );
                        }
                    }
                    RoomEvent::TrackSubscriptionFailed { participant, track_sid, error }
                        if participant.identity().as_str() == args.participant =>
                    {
                        warn!(
                            "Subscription failed for target participant '{}' sid {}: {:?}",
                            participant.identity(),
                            track_sid,
                            error
                        );
                        requested_sid = None;
                        let _ = try_subscribe_existing_participant_tracks(
                            &room,
                            &args.participant,
                            &mut requested_sid,
                            &active_sid,
                        );
                    }
                    RoomEvent::ParticipantDisconnected(participant)
                        if participant.identity().as_str() == args.participant =>
                    {
                        warn!("Target participant '{}' disconnected", participant.identity());
                        active_sid = None;
                        requested_sid = None;
                    }
                    _ => {}
                }
            }
        }
    }
}
