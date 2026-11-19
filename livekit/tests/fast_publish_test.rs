// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Exercises JS-style ordinary fast publication independently from publish-with-JOIN.
//! Run against livekit-server --dev with --features __lk-e2e-test,native-tls.
#![cfg(feature = "__lk-e2e-test")]

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use libwebrtc::{
    audio_source::native::NativeAudioSource,
    native::create_random_uuid,
    prelude::{
        AudioSourceOptions, I420Buffer, RtcAudioSource, RtcVideoSource, VideoFrame,
        VideoResolution, VideoRotation,
    },
    video_source::native::NativeVideoSource,
    video_stream::native::NativeVideoStream,
};
use livekit::{
    e2ee::{
        key_provider::{KeyProvider, KeyProviderOptions},
        EncryptionType,
    },
    options::TrackPublishOptions,
    track::{LocalAudioTrack, LocalTrack, LocalVideoTrack, RemoteTrack, TrackSource},
    E2eeOptions, Room, RoomEvent, RoomOptions, SimulateScenario,
};
use livekit_token::{AccessToken, VideoGrants};
use test_case::test_case;
use tokio::{
    sync::mpsc::UnboundedReceiver,
    task::JoinHandle,
    time::{self, timeout, Instant},
};

#[path = "common/fast_publish/mod.rs"]
mod proxy;
use proxy::{ProxyOptions, SignalProxy};

/// Selects the transport topology through the existing connect options.
fn room_options(single_pc: bool) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.single_peer_connection = single_pc;
    options
}

/// Creates a synthetic microphone, without accessing a platform recording device.
fn microphone() -> LocalTrack {
    LocalTrack::Audio(LocalAudioTrack::create_audio_track(
        "fast-microphone",
        RtcAudioSource::Native(NativeAudioSource::new(
            AudioSourceOptions::default(),
            48_000,
            1,
            1000,
        )),
    ))
}

/// Creates a synthetic camera source for publication and first-frame measurements.
fn camera() -> (LocalTrack, NativeVideoSource) {
    let source = NativeVideoSource::new(VideoResolution { width: 640, height: 360 }, false);
    let track =
        LocalVideoTrack::create_video_track("fast-camera", RtcVideoSource::Native(source.clone()));
    (LocalTrack::Video(track), source)
}

/// Gives the test participant explicit publish and subscribe permission in one fresh room.
fn token(room: &str, identity: &str) -> Result<String> {
    let key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
    let secret = std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
    Ok(AccessToken::with_api_key(&key, &secret)
        .with_identity(identity)
        .with_grants(VideoGrants {
            room_join: true,
            room: room.into(),
            can_publish: Some(true),
            can_publish_data: Some(true),
            can_subscribe: Some(true),
            ..Default::default()
        })
        .to_jwt()?)
}

/// Connects one participant through a proxy, preserving ordinary public connect options.
async fn connect(
    proxy: &SignalProxy,
    name: &str,
    identity: &str,
    options: RoomOptions,
) -> Result<(Room, UnboundedReceiver<RoomEvent>)> {
    let single_pc = options.single_peer_connection;
    let (room, events) = Room::connect(&proxy.url, &token(name, identity)?, options).await?;
    assert_eq!(room.is_single_peer_connection_active(), single_pc, "transport mode fell back");
    Ok((room, events))
}

/// Waits for the SFU to announce the exact expected publication count to the subscriber.
async fn wait_publications(room: &Room, expected: usize) -> Result<()> {
    timeout(Duration::from_secs(10), async {
        loop {
            let count: usize =
                room.remote_participants().values().map(|p| p.track_publications().len()).sum();
            if count == expected {
                return;
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("subscriber did not see {expected} publications"))
}

/// Withheld ACKs prove overlap without depending on wall-clock latency thresholds.
#[test_case(false, false; "dual_sequential")]
#[test_case(false, true; "dual_fast")]
#[test_case(true, false; "single_sequential")]
#[test_case(true, true; "single_fast")]
#[test_log::test(tokio::test)]
async fn publication_negotiates_before_ack_only_in_fast_mode(
    single_pc: bool,
    fast: bool,
) -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: fast,
        hold_ack: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("fast_{}", create_random_uuid());
        let (publisher, _) = connect(&proxy, &name, "publisher", room_options(single_pc)).await?;
        let track = microphone();
        let cid = track.rtc_track().id();
        let participant = publisher.local_participant();
        let publishing_track = track.clone();
        let mut publishing = Box::pin(async move {
            participant.publish_track(publishing_track, TrackPublishOptions::default()).await
        });
        tokio::select! {
            result = &mut publishing => bail!("publication completed before ACK release: {result:?}"),
            observed = proxy.wait_for(&cid, "ack") => observed?,
        }
        if fast {
            tokio::select! {
                result = &mut publishing => bail!("publication completed before ACK release: {result:?}"),
                observed = proxy.wait_for(&cid, "offer") => observed?,
            }
            assert!(track.has_sender(), "fast publication must prepare its sender before ACK");
        } else {
            assert!(!track.has_sender(), "legacy publication must not speculate before ACK");
            assert!(!proxy.observations(&cid).iter().any(|event| event.kind == "offer"));
        }
        proxy.release_ack();
        let publication = timeout(Duration::from_secs(10), publishing).await??;
        proxy.wait_for(&cid, "offer").await?;
        assert_eq!(publisher.local_participant().track_publications().len(), 1);
        publisher.local_participant().unpublish_track(&publication.sid()).await?;
        assert!(!track.has_sender());
        publisher.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Initial-track connect options and ordinary A/V publication work in both transport modes.
#[test_case(false, false; "dual_sequential")]
#[test_case(false, true; "dual_fast")]
#[test_case(true, false; "single_sequential")]
#[test_case(true, true; "single_fast")]
#[test_log::test(tokio::test)]
async fn connect_with_and_without_initial_tracks(single_pc: bool, fast: bool) -> Result<()> {
    for with_initial in [false, true] {
        let proxy =
            SignalProxy::start(ProxyOptions { fast_publish: fast, ..Default::default() }).await?;
        let outcome: Result<()> = async {
            let name = format!("initial_fast_{}", create_random_uuid());
            let initial = microphone();
            let mut options = room_options(single_pc);
            if with_initial {
                options.publish_tracks = vec![(initial, TrackPublishOptions::default())];
            }
            let (publisher, _) = connect(&proxy, &name, "publisher", options).await?;
            assert_eq!(
                publisher.local_participant().track_publications().len(),
                usize::from(with_initial)
            );
            let (subscriber, _) =
                connect(&proxy, &name, "subscriber", room_options(single_pc)).await?;
            let audio = microphone();
            audio.mute();
            let (video, _) = camera();
            let participant = publisher.local_participant();
            let (audio_pub, video_pub) = tokio::try_join!(
                participant.publish_track(
                    audio.clone(),
                    TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() }
                ),
                participant.publish_track(
                    video,
                    TrackPublishOptions { source: TrackSource::Camera, ..Default::default() }
                ),
            )?;
            assert!(audio.is_muted());
            assert!(audio_pub.is_muted());
            assert_ne!(audio_pub.sid(), video_pub.sid());
            wait_publications(&subscriber, 2 + usize::from(with_initial)).await?;
            publisher.close().await?;
            subscriber.close().await?;
            Ok(())
        }
        .await;
        proxy.shutdown().await;
        outcome?;
    }
    Ok(())
}

/// Missing codec information preserves sequential publication even with fast_publish enabled.
#[test_log::test(tokio::test)]
async fn missing_codecs_keep_sequential_fallback() -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: true,
        hold_ack: true,
        clear_codecs: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("fallback_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let track = microphone();
        let cid = track.rtc_track().id();
        let participant = room.local_participant();
        let copy = track.clone();
        let mut task = Box::pin(async move {
            participant.publish_track(copy, TrackPublishOptions::default()).await
        });
        tokio::select! {
            result = &mut task => bail!("publication completed before ACK release: {result:?}"),
            observed = proxy.wait_for(&cid, "ack") => observed?,
        }
        assert!(!track.has_sender());
        proxy.release_ack();
        timeout(Duration::from_secs(10), task).await??;
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Encrypted senders retain the sequential path so their cryptor is installed before negotiation.
#[test_log::test(tokio::test)]
async fn encryption_keeps_sequential_fallback() -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: true,
        hold_ack: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("encrypted_fast_{}", create_random_uuid());
        let mut options = RoomOptions::default();
        options.encryption = Some(E2eeOptions {
            key_provider: KeyProvider::with_shared_key(
                KeyProviderOptions::default(),
                b"test-password".to_vec(),
            ),
            encryption_type: EncryptionType::Gcm,
        });
        let (room, _) = connect(&proxy, &name, "publisher", options).await?;
        let track = microphone();
        let cid = track.rtc_track().id();
        let participant = room.local_participant();
        let copy = track.clone();
        let mut task = Box::pin(async move {
            participant.publish_track(copy, TrackPublishOptions::default()).await
        });
        tokio::select! {
            result = &mut task => bail!("publication completed before ACK release: {result:?}"),
            observed = proxy.wait_for(&cid, "ack") => observed?,
        }
        assert!(!track.has_sender());
        proxy.release_ack();
        let publication = timeout(Duration::from_secs(10), task).await??;
        assert_eq!(publication.encryption_type(), EncryptionType::Gcm);
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Packet-trailer negotiation remains response-dependent even on a fast-publish server.
#[test_log::test(tokio::test)]
async fn packet_trailers_keep_sequential_fallback() -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: true,
        hold_ack: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("trailer_fast_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let (track, _) = camera();
        let cid = track.rtc_track().id();
        let participant = room.local_participant();
        let copy = track.clone();
        let mut options = TrackPublishOptions::default();
        options.frame_metadata_features.user_timestamp = true;
        let mut publishing = Box::pin(participant.publish_track(copy, options));
        tokio::select! {
            result = &mut publishing => bail!("publication completed before ACK release: {result:?}"),
            observed = proxy.wait_for(&cid, "ack") => observed?,
        }
        assert!(!track.has_sender());
        proxy.release_ack();
        timeout(Duration::from_secs(10), publishing).await??;
        room.close().await?;
        Ok(())
    }.await;
    proxy.shutdown().await;
    outcome
}

/// Duplicate publication cannot replace or roll back an accepted track's sender.
#[test_log::test(tokio::test)]
async fn duplicate_publication_preserves_original_sender() -> Result<()> {
    let proxy =
        SignalProxy::start(ProxyOptions { fast_publish: true, ..Default::default() }).await?;
    let outcome: Result<()> = async {
        let name = format!("duplicate_fast_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let track = microphone();
        let participant = room.local_participant();
        let publication =
            participant.publish_track(track.clone(), TrackPublishOptions::default()).await?;
        assert!(participant
            .publish_track(track.clone(), TrackPublishOptions::default())
            .await
            .is_err());
        assert!(track.has_sender());
        assert_eq!(participant.track_publications().len(), 1);
        participant.unpublish_track(&publication.sid()).await?;
        assert!(!track.has_sender());
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Rejection releases speculative senders and the Rust track without closing the room.
#[test_log::test(tokio::test)]
async fn rejected_publication_releases_track() -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: true,
        reject_track: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("rejected_fast_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let track = microphone();
        let LocalTrack::Audio(audio) = &track else { unreachable!() };
        let released = audio.drop_probe();
        assert!(room
            .local_participant()
            .publish_track(track.clone(), TrackPublishOptions::default())
            .await
            .is_err());
        assert!(!track.has_sender());
        assert!(room.local_participant().track_publications().is_empty());
        drop(track);
        assert!(released(), "failed publication retained the Rust track");
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Aborting publication removes its native sender and pending response registration.
#[test_log::test(tokio::test)]
async fn cancelled_publication_releases_track() -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: true,
        hold_ack: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("cancelled_fast_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let track = microphone();
        let LocalTrack::Audio(audio) = &track else { unreachable!() };
        let released = audio.drop_probe();
        let cid = track.rtc_track().id();
        let participant = room.local_participant();
        let copy = track.clone();
        let mut task = Box::pin(async move {
            participant.publish_track(copy, TrackPublishOptions::default()).await
        });
        tokio::select! {
            result = &mut task => bail!("publication completed before cancellation: {result:?}"),
            observed = proxy.wait_for(&cid, "offer") => observed?,
        }
        assert!(
            room.local_participant()
                .publish_track(track.clone(), TrackPublishOptions::default())
                .await
                .is_err(),
            "an in-flight publication must retain its exclusive claim"
        );
        assert!(track.has_sender(), "a duplicate must not remove the speculative sender");
        drop(task);
        assert!(!track.has_sender());
        assert!(room.local_participant().track_publications().is_empty());
        drop(track);
        assert!(released(), "cancelled publication retained the Rust track");
        proxy.release_ack();
        // A fresh publication must still work after cancellation and late responses.
        room.local_participant()
            .publish_track(microphone(), TrackPublishOptions::default())
            .await?;
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Closing a room cancels pending publication in both policies and releases its track.
#[test_case(false; "sequential")]
#[test_case(true; "fast")]
#[test_log::test(tokio::test)]
async fn disconnect_cancels_pending_publication(fast: bool) -> Result<()> {
    let proxy = SignalProxy::start(ProxyOptions {
        fast_publish: fast,
        hold_ack: true,
        ..Default::default()
    })
    .await?;
    let outcome: Result<()> = async {
        let name = format!("disconnect_fast_{}", create_random_uuid());
        let (room, _) = connect(&proxy, &name, "publisher", RoomOptions::default()).await?;
        let track = microphone();
        let LocalTrack::Audio(audio) = &track else { unreachable!() };
        let released = audio.drop_probe();
        let cid = track.rtc_track().id();
        let participant = room.local_participant();
        let mut publishing = Box::pin(participant.publish_track(track.clone(), TrackPublishOptions::default()));
        tokio::select! {
            result = &mut publishing => bail!("publication completed before disconnect: {result:?}"),
            observed = proxy.wait_for(&cid, "ack") => observed?,
        }
        let (closed, published) = timeout(Duration::from_secs(10), async {
            tokio::join!(room.close(), publishing)
        }).await?;
        closed?;
        assert!(published.is_err());
        assert!(!track.has_sender());
        drop(track);
        assert!(released(), "disconnect retained the pending track");
        Ok(())
    }.await;
    proxy.shutdown().await;
    outcome
}

/// Both server policies republish current tracks once after a full reconnect.
#[test_case(false; "sequential")]
#[test_case(true; "fast")]
#[test_log::test(tokio::test)]
async fn full_reconnect_does_not_duplicate_publications(fast: bool) -> Result<()> {
    let proxy =
        SignalProxy::start(ProxyOptions { fast_publish: fast, ..Default::default() }).await?;
    let outcome: Result<()> = async {
        let name = format!("reconnect_fast_{}", create_random_uuid());
        let (room, mut events) = connect(&proxy, &name, "publisher", room_options(true)).await?;
        room.local_participant()
            .publish_track(microphone(), TrackPublishOptions::default())
            .await?;
        room.simulate_scenario(SimulateScenario::FullReconnect)
            .await
            .map_err(|error| anyhow!("{error:?}"))?;
        timeout(Duration::from_secs(30), async {
            while let Some(event) = events.recv().await {
                if matches!(event, RoomEvent::Reconnected) {
                    return Ok(());
                }
                if matches!(event, RoomEvent::Disconnected { .. }) {
                    bail!("disconnected during reconnect")
                }
            }
            bail!("event stream closed")
        })
        .await??;
        assert_eq!(room.local_participant().track_publications().len(), 1);
        room.close().await?;
        Ok(())
    }
    .await;
    proxy.shutdown().await;
    outcome
}

/// Owns continuous camera capture; drop stops it and explicit shutdown also joins the task.
struct CameraPump(Option<JoinHandle<()>>);

impl CameraPump {
    /// Starts synthetic 30fps capture without a camera device.
    fn start(source: NativeVideoSource) -> Self {
        Self(Some(tokio::spawn(async move {
            let mut ticks = time::interval(Duration::from_millis(33));
            loop {
                ticks.tick().await;
                source.capture_frame(&VideoFrame::new(
                    VideoRotation::VideoRotation0,
                    I420Buffer::new(640, 360),
                ));
            }
        })))
    }

    /// Cancels and joins the capture task.
    async fn shutdown(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for CameraPump {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

/// Measures first received video against the sequential fallback with identical eager negotiation.
#[test_log::test(tokio::test)]
async fn publication_latency_benchmark() -> Result<()> {
    let iterations = std::env::var("FAST_PUBLISH_BENCH_ITERATIONS")
        .ok()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(5);
    let mut sequential = Vec::new();
    let mut parallel = Vec::new();
    let mut sequential_offers = Vec::new();
    let mut parallel_offers = Vec::new();
    for i in 0..iterations {
        for fast in if i % 2 == 0 { [false, true] } else { [true, false] } {
            let proxy = SignalProxy::start(ProxyOptions {
                fast_publish: true,
                clear_codecs: !fast,
                one_way_delay: Duration::from_millis(50),
                ..Default::default()
            })
            .await?;
            let outcome: Result<(u128, u128)> = async {
                let name = format!("benchmark_fast_{}", create_random_uuid());
                let (publisher, _) =
                    connect(&proxy, &name, "publisher", room_options(false)).await?;
                let (subscriber, mut events) =
                    connect(&proxy, &name, "subscriber", room_options(false)).await?;
                // Sending data waits for the publisher transport, excluding its warm-up.
                publisher.local_participant().publish_data(Default::default()).await?;
                let (track, source) = camera();
                let cid = track.rtc_track().id();
                let pump = CameraPump::start(source);
                let started = Instant::now();
                let measured: Result<(u128, u128)> = async {
                    publisher.local_participant().publish_track(track, TrackPublishOptions {
                        source: TrackSource::Camera,
                        ..Default::default()
                    }).await?;
                    let publication_ms = started.elapsed().as_millis();
                    let remote = timeout(Duration::from_secs(10), async {
                        while let Some(event) = events.recv().await {
                            if let RoomEvent::TrackSubscribed {
                                track: RemoteTrack::Video(video), ..
                            } = event {
                                return Ok(video);
                            }
                        }
                        Err(anyhow!("subscriber event stream closed"))
                    }).await??;
                    let mut frames = NativeVideoStream::new(remote.rtc_track());
                    timeout(Duration::from_secs(10), frames.next()).await?
                        .ok_or_else(|| anyhow!("video stream closed"))?;
                    let media_ms = started.elapsed().as_millis();
                    let trace = proxy.observations(&cid);
                    let add = trace.iter().find(|event| event.kind == "add")
                        .ok_or_else(|| anyhow!("missing AddTrack observation"))?;
                    let offer = trace.iter().find(|event| event.kind == "offer")
                        .ok_or_else(|| anyhow!("missing offer observation"))?;
                    let offer_ms = offer.at.duration_since(add.at).as_millis();
                    println!(
                        "FAST_PUBLISH_SAMPLE mode={} publication_ms={publication_ms} first_frame_ms={media_ms} offer_after_add_ms={offer_ms}",
                        if fast { "parallel" } else { "sequential" }
                    );
                    Ok((media_ms, offer_ms))
                }.await;
                pump.shutdown().await;
                publisher.close().await?;
                subscriber.close().await?;
                measured
            }.await;
            proxy.shutdown().await;
            let (elapsed, offer) = outcome?;
            if fast {
                parallel.push(elapsed);
                parallel_offers.push(offer);
            } else {
                sequential.push(elapsed);
                sequential_offers.push(offer);
            }
        }
    }
    sequential.sort_unstable();
    parallel.sort_unstable();
    sequential_offers.sort_unstable();
    parallel_offers.sort_unstable();
    if iterations != 0 {
        let sequential = sequential[iterations / 2];
        let parallel = parallel[iterations / 2];
        let sequential_offer = sequential_offers[iterations / 2];
        let parallel_offer = parallel_offers[iterations / 2];
        println!(
            "FAST_PUBLISH_RESULT iterations={iterations} signaling_rtt_ms=100 topology=dual sequential_first_frame_p50_ms={sequential} parallel_first_frame_p50_ms={parallel} saved_ms={} sequential_offer_after_add_p50_ms={sequential_offer} parallel_offer_after_add_p50_ms={parallel_offer}",
            sequential.saturating_sub(parallel)
        );
    }
    Ok(())
}
