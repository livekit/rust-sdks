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

//! Tests for `RoomOptions::publish_tracks`, which publishes tracks while connecting.
//!
//! Run:
//!   cargo test -p livekit --features "__lk-e2e-test,native-tls" --test publish_with_join_test -- --nocapture

#![cfg(feature = "__lk-e2e-test")]

use anyhow::{anyhow, bail, Result};
use common::{test_rooms_with_options, TestRoomOptions};
use futures_util::StreamExt;
use libwebrtc::{
    audio_source::native::NativeAudioSource,
    native::create_random_uuid,
    prelude::{
        AudioFrame, AudioSourceOptions, I420Buffer, RtcAudioSource, RtcVideoSource, VideoFrame,
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
    E2eeOptions, InitialTrack, Room, RoomError, RoomEvent, RoomOptions, SimulateScenario,
};
use livekit_api::services::room::RoomClient;
use livekit_token::{AccessToken, VideoGrants};
use std::{env, net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc::UnboundedReceiver,
    task::JoinHandle,
    time::{self, timeout},
};

mod common;

fn join_tracks() -> Vec<InitialTrack> {
    let audio = LocalAudioTrack::create_audio_track(
        "join-audio",
        RtcAudioSource::Native(NativeAudioSource::new(
            AudioSourceOptions::default(),
            48_000,
            1,
            1000,
        )),
    );
    let video = LocalVideoTrack::create_video_track(
        "join-video",
        RtcVideoSource::Native(NativeVideoSource::new(
            VideoResolution { width: 640, height: 360 },
            false,
        )),
    );
    vec![
        InitialTrack::new(LocalTrack::Audio(audio)).with_options(TrackPublishOptions {
            source: TrackSource::Microphone,
            ..Default::default()
        }),
        (
            LocalTrack::Video(video),
            TrackPublishOptions { source: TrackSource::Camera, ..Default::default() },
        )
            .into(),
    ]
}

fn publisher_options(single_peer_connection: bool) -> RoomOptions {
    let mut options = RoomOptions::default();
    options.single_peer_connection = single_peer_connection;
    options.publish_tracks = join_tracks();
    options
}

fn shared_key_encryption() -> Option<E2eeOptions> {
    let key_provider =
        KeyProvider::with_shared_key(KeyProviderOptions::default(), b"password".to_vec());
    Some(E2eeOptions { key_provider, encryption_type: EncryptionType::Gcm })
}

/// Connects `publisher` and a subscriber with the same connection mode and encryption.
async fn connect_pair(
    publisher: RoomOptions,
) -> Result<((Room, UnboundedReceiver<RoomEvent>), (Room, UnboundedReceiver<RoomEvent>))> {
    let mut subscriber = RoomOptions::default();
    subscriber.single_peer_connection = publisher.single_peer_connection;
    subscriber.encryption = publisher.encryption.as_ref().and_then(|_| shared_key_encryption());

    let mut rooms = test_rooms_with_options([
        TestRoomOptions::from(publisher),
        TestRoomOptions::from(subscriber),
    ])
    .await?;
    let subscriber = rooms.pop().unwrap();
    let publisher = rooms.pop().unwrap();
    Ok((publisher, subscriber))
}

/// Waits until `subscriber` sees exactly `expected` publications from the other participant.
async fn wait_remote_publications(subscriber: &Room, expected: usize) -> Result<()> {
    timeout(Duration::from_secs(10), async {
        loop {
            let count: usize = subscriber
                .remote_participants()
                .values()
                .map(|p| p.track_publications().len())
                .sum();
            if count == expected {
                return;
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("subscriber never saw {expected} remote publications"))
}

async fn assert_published_with_join(publisher: RoomOptions) -> Result<()> {
    let ((publisher, mut publisher_events), (subscriber, _)) = connect_pair(publisher).await?;

    // Publishing completes before `connect` returns.
    let publications = publisher.local_participant().track_publications();
    assert_eq!(publications.len(), 2);
    assert!(publications.values().all(|p| p.track().is_some()));

    let mut published = 0;
    while let Ok(event) = publisher_events.try_recv() {
        if matches!(event, RoomEvent::LocalTrackPublished { .. }) {
            published += 1;
        }
    }
    assert_eq!(published, 2, "each join-time track emits LocalTrackPublished");

    wait_remote_publications(&subscriber, 2).await
}

#[test_log::test(tokio::test)]
async fn test_publish_with_join_single_pc() -> Result<()> {
    assert_published_with_join(publisher_options(true)).await
}

/// Without single PC mode the tracks cannot travel with the join and are published normally.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_dual_pc() -> Result<()> {
    assert_published_with_join(publisher_options(false)).await
}

/// In an encrypted room the tracks are published normally, so each sender gets its frame
/// cryptor before any media is sent.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_encrypted() -> Result<()> {
    let mut publisher = publisher_options(true);
    publisher.encryption = shared_key_encryption();
    let ((publisher, _), (subscriber, _)) = connect_pair(publisher).await?;

    let publications = publisher.local_participant().track_publications();
    assert_eq!(publications.len(), 2);
    assert!(publications.values().all(|p| p.encryption_type() == EncryptionType::Gcm));

    wait_remote_publications(&subscriber, 2).await?;
    for participant in subscriber.remote_participants().values() {
        for publication in participant.track_publications().values() {
            assert_eq!(publication.encryption_type(), EncryptionType::Gcm);
        }
    }
    Ok(())
}

/// A full reconnect must republish the join-time tracks once, not replay them with the join.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_full_reconnect_no_duplicates() -> Result<()> {
    let ((publisher, mut publisher_events), (subscriber, _)) =
        connect_pair(publisher_options(true)).await?;
    wait_remote_publications(&subscriber, 2).await?;

    publisher
        .simulate_scenario(SimulateScenario::FullReconnect)
        .await
        .map_err(|e| anyhow!("simulate_scenario failed: {e:?}"))?;
    timeout(Duration::from_secs(30), async {
        while let Some(event) = publisher_events.recv().await {
            match event {
                RoomEvent::Reconnected => return Ok(()),
                RoomEvent::Disconnected { reason } => bail!("disconnected: {reason:?}"),
                _ => {}
            }
        }
        bail!("event stream ended before reconnecting")
    })
    .await??;

    // Give any duplicate publication time to show up before asserting.
    time::sleep(Duration::from_secs(2)).await;
    assert_eq!(publisher.local_participant().track_publications().len(), 2);
    wait_remote_publications(&subscriber, 2).await
}

struct ServerEnv {
    url: String,
    key: String,
    secret: String,
}

impl ServerEnv {
    fn from_env_or_defaults() -> Self {
        Self {
            url: env::var("LIVEKIT_URL").unwrap_or_else(|_| "http://localhost:7880".into()),
            key: env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into()),
            secret: env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into()),
        }
    }

    fn token(&self, room: &str, grants: VideoGrants) -> Result<String> {
        Ok(AccessToken::with_api_key(&self.key, &self.secret)
            .with_grants(VideoGrants { room: room.to_string(), ..grants })
            .with_identity("p0")
            .to_jwt()?)
    }

    /// The server's `host:port`, which a proxy in front of it forwards to.
    fn host_port(&self) -> String {
        let without_scheme = self.url.split("://").nth(1).unwrap_or(&self.url);
        without_scheme.trim_end_matches('/').to_string()
    }
}

fn single_audio_track() -> (InitialTrack, impl Fn() -> bool) {
    let track = LocalAudioTrack::create_audio_track(
        "join-audio",
        RtcAudioSource::Native(NativeAudioSource::new(
            AudioSourceOptions::default(),
            48_000,
            1,
            1000,
        )),
    );
    let released = track.drop_probe();
    (LocalTrack::Audio(track).into(), released)
}

async fn wait_released(released: impl Fn() -> bool) -> Result<()> {
    timeout(Duration::from_secs(10), async {
        while !released() {
            time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("join-time track retained"))
}

/// Unpublishing a join-time track must release it while the room stays connected: neither the
/// engine nor the room may keep the tracks they were given at connect.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_releases_unpublished_track() -> Result<()> {
    let (track, released) = single_audio_track();
    let mut options = RoomOptions::default();
    options.publish_tracks = vec![track];
    let (room, events) = test_rooms_with_options([TestRoomOptions::from(options)]).await?.remove(0);
    // Queued events hold publications, and so the track.
    drop(events);

    let sid = room
        .local_participant()
        .track_publications()
        .into_keys()
        .next()
        .ok_or_else(|| anyhow!("join-time track was not published"))?;
    room.local_participant().unpublish_track(&sid).await?;

    wait_released(released).await?;
    room.close().await?;
    Ok(())
}

/// Accepts connections on a local port and forwards them to `target`, except WebSocket upgrades
/// to `/rtc/v1`, which get a 404 as from a server without the v1 signalling path.
async fn start_v1_rejecting_proxy(target: String) -> Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        while let Ok((mut inbound, _)) = listener.accept().await {
            let target = target.clone();
            tokio::spawn(async move {
                let mut head = vec![0u8; 4096];
                let Ok(n) = inbound.read(&mut head).await else { return };
                head.truncate(n);
                let request_line = head.split(|b| *b == b'\n').next().unwrap_or_default();
                if String::from_utf8_lossy(request_line).contains(" /rtc/v1") {
                    let _ = inbound
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                        .await;
                    return;
                }
                let Ok(mut outbound) = TcpStream::connect(&target).await else { return };
                if outbound.write_all(&head).await.is_ok() {
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                }
            });
        }
    });
    Ok(addr)
}

/// When the server has no v1 path, signalling falls back to v0 and the join-time tracks are
/// published normally instead of waiting out the publish timeout.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_falls_back_without_v1() -> Result<()> {
    let server = ServerEnv::from_env_or_defaults();
    let proxy = start_v1_rejecting_proxy(server.host_port()).await?;
    let room_name = format!("test_room_{}", create_random_uuid());
    let token = server.token(&room_name, VideoGrants { room_join: true, ..Default::default() })?;

    let url = format!("ws://{proxy}");
    let connect = Room::connect(&url, &token, publisher_options(true));
    // Well under the 10 s publish timeout a lost join-time publish would wait out.
    let (room, _) = timeout(Duration::from_secs(8), connect)
        .await
        .map_err(|_| anyhow!("connect did not fall back in time"))??;

    let publications = room.local_participant().track_publications();
    assert_eq!(publications.len(), 2);
    assert!(publications.values().all(|p| p.track().is_some()));
    room.close().await?;
    Ok(())
}

/// A join-time track the server refuses fails `connect`, leaves no participant behind and does
/// not leak the track.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_failure_closes_room() -> Result<()> {
    let server = ServerEnv::from_env_or_defaults();
    let room_name = format!("test_room_{}", create_random_uuid());
    let grants = VideoGrants { room_join: true, can_publish: Some(false), ..Default::default() };
    let token = server.token(&room_name, grants)?;

    let (track, released) = single_audio_track();
    let mut options = RoomOptions::default();
    options.publish_tracks = vec![track];
    // The join-time confirmation wait is short, far below the 10 s publish timeout.
    let connected = timeout(Duration::from_secs(5), Room::connect(&server.url, &token, options))
        .await
        .map_err(|_| anyhow!("connect waited too long for a refused track"))?;
    let Err(err) = connected else {
        bail!("connect succeeded although the track could not be published");
    };
    log::info!("connect failed as expected: {err}");

    let http_url = server.url.replacen("ws", "http", 1);
    let rooms = RoomClient::with_api_key(&http_url, &server.key, &server.secret);
    timeout(Duration::from_secs(10), async {
        while !rooms.list_participants(&room_name).await.unwrap_or_default().is_empty() {
            time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("participant left behind after the failed connect"))?;

    wait_released(released).await
}

/// Listing a track twice is rejected before connecting, as publishing it twice would be.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_rejects_duplicate_track() -> Result<()> {
    let server = ServerEnv::from_env_or_defaults();
    let room_name = format!("test_room_{}", create_random_uuid());
    let token = server.token(&room_name, VideoGrants { room_join: true, ..Default::default() })?;

    let (track, _) = single_audio_track();
    let mut options = RoomOptions::default();
    options.publish_tracks = vec![track.clone(), track];
    match Room::connect(&server.url, &token, options).await {
        Err(RoomError::Internal(_)) => {}
        Err(err) => bail!("unexpected error: {err}"),
        Ok(_) => bail!("connect accepted a duplicate join-time track"),
    }

    let http_url = server.url.replacen("ws", "http", 1);
    let rooms = RoomClient::with_api_key(&http_url, &server.key, &server.secret);
    assert!(
        rooms.list_participants(&room_name).await.unwrap_or_default().is_empty(),
        "nothing should reach the server"
    );
    Ok(())
}

/// Creates a microphone and camera that are muted and have not captured anything yet.
fn muted_tracks() -> (LocalAudioTrack, LocalVideoTrack) {
    let audio = LocalAudioTrack::create_audio_track(
        "muted-audio",
        RtcAudioSource::Native(NativeAudioSource::new(
            AudioSourceOptions::default(),
            48_000,
            1,
            1000,
        )),
    );
    let video = LocalVideoTrack::create_video_track(
        "muted-video",
        RtcVideoSource::Native(NativeVideoSource::new(
            VideoResolution { width: 320, height: 180 },
            false,
        )),
    );
    audio.mute();
    video.mute();
    (audio, video)
}

/// Captures silence and black frames into the tracks' sources until aborted.
fn capture(audio: &LocalAudioTrack, video: &LocalVideoTrack) -> JoinHandle<()> {
    let RtcAudioSource::Native(audio) = audio.rtc_source() else { unreachable!() };
    let RtcVideoSource::Native(video) = video.rtc_source() else { unreachable!() };
    tokio::spawn(async move {
        let samples = vec![0i16; 480];
        loop {
            let frame = AudioFrame {
                data: samples.as_slice().into(),
                sample_rate: 48_000,
                num_channels: 1,
                samples_per_channel: 480,
            };
            audio.capture_frame(&frame).await.expect("capture audio frame");
            video.capture_frame(&VideoFrame::new(
                VideoRotation::VideoRotation0,
                I420Buffer::new(320, 180),
            ));
        }
    })
}

/// Waits until every remote publication `subscriber` sees is subscribed and has `muted` set.
async fn wait_remote_muted(subscriber: &Room, muted: bool) -> Result<()> {
    timeout(Duration::from_secs(10), async {
        loop {
            let publications: Vec<_> = subscriber
                .remote_participants()
                .values()
                .flat_map(|p| p.track_publications().into_values())
                .collect();
            if publications.len() == 2
                && publications.iter().all(|p| p.is_muted() == muted && p.track().is_some())
            {
                return;
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| anyhow!("subscriber never saw both tracks subscribed with muted={muted}"))
}

/// Tracks muted before joining are published like any muted track: the server publishes them
/// without waiting for media, and they carry media once unmuted.
#[test_log::test(tokio::test)]
async fn test_publish_with_join_muted_tracks() -> Result<()> {
    let (audio, video) = muted_tracks();
    let mut publisher = RoomOptions::default();
    publisher.publish_tracks = vec![
        InitialTrack::new(LocalTrack::Audio(audio.clone())).with_options(TrackPublishOptions {
            source: TrackSource::Microphone,
            ..Default::default()
        }),
        InitialTrack::new(LocalTrack::Video(video.clone())).with_options(TrackPublishOptions {
            source: TrackSource::Camera,
            ..Default::default()
        }),
    ];
    let ((publisher, _), (subscriber, mut subscriber_events)) = connect_pair(publisher).await?;

    let publications = publisher.local_participant().track_publications();
    assert_eq!(publications.len(), 2);
    assert!(publications.values().all(|p| p.is_muted()));
    wait_remote_muted(&subscriber, true).await?;

    audio.unmute();
    video.unmute();
    let capture = capture(&audio, &video);
    wait_remote_muted(&subscriber, false).await?;

    let remote_video = timeout(Duration::from_secs(10), async {
        while let Some(event) = subscriber_events.recv().await {
            if let RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. } = event {
                return Ok(track);
            }
        }
        bail!("event stream ended before the video track was subscribed")
    })
    .await??;
    let mut frames = NativeVideoStream::new(remote_video.rtc_track());
    timeout(Duration::from_secs(10), frames.next())
        .await
        .map_err(|_| anyhow!("no video frame after unmuting"))?
        .ok_or_else(|| anyhow!("video stream ended"))?;

    capture.abort();
    Ok(())
}
