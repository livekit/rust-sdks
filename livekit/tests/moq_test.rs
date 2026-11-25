// Copyright 2026 LiveKit, Inc.
// SPDX-License-Identifier: Apache-2.0
#![cfg(all(feature = "moq", feature = "__lk-e2e-test"))]
mod common;

use anyhow::{anyhow, Result};
use common::{
    audio::{SineParameters, SineTrack},
    test_rooms_with_options,
    video::{SolidColorParams, SolidColorTrack},
    TestRoomOptions,
};
use futures_util::StreamExt;
use libwebrtc::prelude::VideoBuffer;
use libwebrtc::{audio_stream::native::NativeAudioStream, video_stream::native::NativeVideoStream};
use livekit::{options::VideoCodec, prelude::*};
use std::{sync::Arc, time::Duration};

#[test_log::test(tokio::test)]
#[ignore = "requires a LiveKit server with a TLS MoQ endpoint and DataTracks enabled"]
async fn room_transport_matrix() -> Result<()> {
    for (sender, receiver) in [
        (RoomTransport::Moq, RoomTransport::WebRtc),
        (RoomTransport::WebRtc, RoomTransport::Moq),
        (RoomTransport::Moq, RoomTransport::Moq),
    ] {
        tokio::time::timeout(Duration::from_secs(100), exchange(sender, receiver)).await??;
    }
    Ok(())
}

/// Exercise per-track routing independently of the media codecs on this host.
#[test_log::test(tokio::test)]
#[ignore = "requires a LiveKit server with a TLS MoQ endpoint"]
async fn datagram_data_tracks() -> Result<()> {
    for (sender, receiver) in [
        (RoomTransport::Moq, RoomTransport::Moq),
        (RoomTransport::Moq, RoomTransport::WebRtc),
        (RoomTransport::WebRtc, RoomTransport::Moq),
    ] {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut rooms = test_rooms_with_options([sender, receiver].map(|transport| {
                let mut opts = TestRoomOptions::default();
                opts.room.transport = transport;
                opts
            })).await?;
            let (subscriber, mut events) = rooms.pop().unwrap();
            let (publisher, _events) = rooms.pop().unwrap();
            let telemetry = publisher.local_participant().publish_data_track("telemetry").await?;
            let images = publisher.local_participant().publish_data_track("images").await?;
            let (mut small, mut large) = (None, None);
            while small.is_none() || large.is_none() {
                if let RoomEvent::DataTrackPublished(track) = events.recv().await.ok_or(anyhow!("room closed"))? {
                    if track.info().sid() == telemetry.info().sid() {
                        small = Some(track.subscribe().await?);
                    } else if track.info().sid() == images.info().sid() {
                        large = Some(track.subscribe().await?);
                    }
                }
            }
            let (mut small, mut large) = (small.unwrap(), large.unwrap());
            let payload = vec![71; 128 * 1024];
            let mut tick = tokio::time::interval(Duration::from_millis(80));
            let (mut got_small, mut got_large) = (false, false);
            while !got_small || !got_large {
                tokio::select! {
                    _ = tick.tick() => {
                        telemetry.try_push(vec![11; 24].into())?;
                        // The WebRTC publisher has one shared freshest-frame slot.
                        if sender == RoomTransport::WebRtc { tokio::time::sleep(Duration::from_millis(5)).await; }
                        images.try_push(payload.clone().into())?;
                    }
                    f = small.next(), if !got_small => {
                        assert_eq!(&f.ok_or(anyhow!("telemetry ended"))?.payload()[..], &[11; 24]);
                        got_small = true;
                    }
                    f = large.next(), if !got_large => {
                        assert_eq!(&f.ok_or(anyhow!("images ended"))?.payload()[..], &payload);
                        got_large = true;
                    }
                }
            }
            images.unpublish();
            loop {
                if let RoomEvent::DataTrackUnpublished(sid) = events.recv().await.ok_or(anyhow!("room closed"))? {
                    if sid == images.info().sid() { break; }
                }
            }
            loop {
                tokio::select! {
                    _ = tick.tick() => telemetry.try_push(vec![22; 24].into())?,
                    f = small.next() => if f.ok_or(anyhow!("remaining track ended"))?.payload()[..] == [22; 24] { break; }
                }
            }
            publisher.close().await?;
            subscriber.close().await?;
            Ok::<_, anyhow::Error>(())
        }).await.map_err(|_| anyhow!("DataTrack timeout: {sender:?} to {receiver:?}"))??;
    }
    Ok(())
}

async fn exchange(sender: RoomTransport, receiver: RoomTransport) -> Result<()> {
    let mut rooms = test_rooms_with_options([sender, receiver].map(|transport| {
        let mut options = TestRoomOptions::default();
        options.room.transport = transport;
        options
    }))
    .await?;
    let (subscriber, mut events) = rooms.pop().unwrap();
    let (publisher, mut publisher_events) = rooms.pop().unwrap();
    let publisher = Arc::new(publisher);
    let local = publisher.local_participant().publish_data_track("moq-data").await?;
    let remote = loop {
        if let RoomEvent::DataTrackPublished(track) =
            events.recv().await.ok_or(anyhow!("room ended waiting for DataTrack"))?
        {
            break track;
        }
    };
    let mut subscription = remote.subscribe().await?;
    let payload = vec![123u8; 128 * 1024];
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    loop {
        tokio::select! {
            _=tick.tick()=>local.try_push(payload.clone().into())?,
            frame=subscription.next()=>{
                let frame=frame.ok_or(anyhow!("DataTrack ended"))?;
                assert_eq!(&frame.payload()[..],&payload);
                break;
            }
        }
    }

    if receiver == RoomTransport::Moq {
        let sid = subscriber.local_participant().sid();
        subscriber.simulate_scenario(livekit::SimulateScenario::SignalReconnect).await?;
        loop {
            match events.recv().await {
                Some(RoomEvent::Reconnected) => break,
                Some(RoomEvent::Disconnected { reason }) => {
                    return Err(anyhow!("signal resume disconnected: {reason:?}"))
                }
                None => return Err(anyhow!("room closed during signaling resume")),
                _ => {}
            }
        }
        assert_eq!(sid, subscriber.local_participant().sid());
        // The existing data subscription and QUIC connection survive the WebSocket restart.
        let resumed = vec![45u8; 4096];
        loop {
            tokio::select! {
                _ = tick.tick() => local.try_push(resumed.clone().into())?,
                frame = subscription.next() => {
                    if frame.ok_or(anyhow!("DataTrack ended during resume"))?.payload()[..] == resumed { break; }
                }
            }
        }
    }

    let mut audio = SineTrack::new(
        publisher.clone(),
        SineParameters { freq: 440.0, amplitude: 0.9, sample_rate: 48_000, num_channels: 1 },
    );
    audio
        .publish_with_options(livekit::options::TrackPublishOptions {
            source: TrackSource::Microphone,
            ..Default::default()
        })
        .await?;
    let mut remote_audio = None;
    let mut active = false;
    let audio_track = loop {
        match events.recv().await.ok_or(anyhow!("room ended waiting for the active microphone"))? {
            RoomEvent::TrackSubscribed { track: RemoteTrack::Audio(track), .. } => {
                remote_audio = Some(track)
            }
            RoomEvent::ActiveSpeakersChanged { speakers } => {
                active |= speakers.iter().any(|p| p.sid() == publisher.local_participant().sid())
            }
            _ => {}
        }
        if active {
            if let Some(track) = remote_audio.take() {
                break track;
            }
        }
    };
    let mut audio_stream = NativeAudioStream::new(audio_track.rtc_track(), 48000, 1);
    loop {
        let frame = audio_stream.next().await.ok_or(anyhow!("audio ended"))?;
        if frame.data.iter().any(|s| s.unsigned_abs() > 2000) {
            break;
        }
    }
    audio.unpublish().await?;

    for codec in
        [VideoCodec::VP8, VideoCodec::H264, VideoCodec::H265, VideoCodec::VP9, VideoCodec::AV1]
    {
        log::info!("publishing {codec:?} from {sender:?} to {receiver:?}");
        let (width, height) = (1280, 720);
        let mut video =
            SolidColorTrack::new(publisher.clone(), SolidColorParams { width, height, luma: 185 })
                .alternate_luma(55);
        video.publish(codec, false).await?;
        let video_track = loop {
            if let RoomEvent::TrackSubscribed {
                track: RemoteTrack::Video(track),
                publication,
                ..
            } = events.recv().await.ok_or(anyhow!("room ended waiting for video"))?
            {
                assert_eq!(
                    publication.mime_type().to_lowercase(),
                    format!("video/{}", codec.as_str()),
                    "codec fallback for {sender:?} -> {receiver:?}"
                );
                break track;
            }
        };
        loop {
            match publisher_events.recv().await {
                Some(RoomEvent::LocalTrackSubscribed { track })
                    if track.sid() == video_track.sid() =>
                {
                    break
                }
                None => return Err(anyhow!("publisher closed waiting for its first subscriber")),
                _ => {}
            }
        }
        let mut video_stream = NativeVideoStream::new(video_track.rtc_track());
        let mut decoded = 0;
        let (mut bright, mut dark) = (false, false);
        loop {
            let frame = video_stream.next().await.ok_or(anyhow!("video ended"))?;
            let buf = frame.buffer.to_i420();
            let (y, _, _) = buf.data();
            assert_eq!((buf.width(), buf.height()), (width, height));
            let luma = y.iter().map(|v| u64::from(*v)).sum::<u64>() / y.len() as u64;
            bright |= luma.abs_diff(185) < 15;
            dark |= luma.abs_diff(55) < 15;
            decoded += 1;
            if decoded >= 5 && bright && dark {
                break;
            }
        }
        video.unpublish().await?;
    }
    publisher.close().await?;
    subscriber.close().await?;
    Ok(())
}

/// Capture a recognizable frame after each network change so buffered video cannot pass the test.
fn capture_handoff_frame(source: &libwebrtc::video_source::native::NativeVideoSource, luma: u8) {
    use libwebrtc::prelude::{I420Buffer, VideoFrame, VideoRotation};
    let mut pixels = I420Buffer::new(320, 180);
    let (y, u, v) = pixels.data_mut();
    y.fill(luma);
    u.fill(128);
    v.fill(128);
    source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, pixels));
}

#[test_log::test(tokio::test)]
#[ignore = "requires a live MoQ SFU; LIVEKIT_MOQ_TEST_HANDOFF_IP optionally selects a second local address"]
async fn live_network_handoff_keeps_video_and_data_tracks() -> Result<()> {
    use libwebrtc::{
        prelude::{RtcVideoSource, VideoResolution},
        video_source::native::NativeVideoSource,
    };
    use livekit::{options::TrackPublishOptions, SimulateScenario};

    tokio::time::timeout(Duration::from_secs(60), async {
        let alternate_ip = std::env::var("LIVEKIT_MOQ_TEST_HANDOFF_IP")
            .ok().map(|ip| ip.parse()).transpose()?;
        let mut options = TestRoomOptions::default();
        options.room.transport = RoomTransport::Moq;
        let mut rooms = test_rooms_with_options([options.clone(), options]).await?;
        let (subscriber, mut events) = rooms.pop().unwrap();
        let (publisher, mut local_events) = rooms.pop().unwrap();
        let publisher_sid = publisher.local_participant().sid();
        let subscriber_sid = subscriber.local_participant().sid();
        let source = NativeVideoSource::new(VideoResolution { width: 320, height: 180 }, false);
        let video = LocalVideoTrack::create_video_track("handoff", RtcVideoSource::Native(source.clone()));
        let publication = publisher.local_participant().publish_track(
            LocalTrack::Video(video),
            TrackPublishOptions { video_codec: VideoCodec::VP8, simulcast: false, ..Default::default() },
        ).await?;
        let data = publisher.local_participant().publish_data_track("handoff").await?;
        let mut video_stream = None;
        let mut data_stream = None;
        let mut tick = tokio::time::interval(Duration::from_millis(80));
        while video_stream.is_none() || data_stream.is_none() {
            tokio::select! {
                _ = tick.tick() => capture_handoff_frame(&source, 32),
                event = events.recv() => match event.ok_or(anyhow!("subscriber disconnected"))? {
                    RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), publication: remote, .. } => {
                        assert_eq!(remote.sid(), publication.sid());
                        video_stream = Some(NativeVideoStream::new(track.rtc_track()));
                    }
                    RoomEvent::DataTrackPublished(track) => {
                        assert_eq!(track.info().sid(), data.info().sid());
                        data_stream = Some(track.subscribe().await?);
                    }
                    _ => {}
                }
            }
        }
        let mut video_stream = video_stream.unwrap();
        let mut data_stream = data_stream.unwrap();
        for (phase, luma) in [32u8, 95, 155, 218].into_iter().enumerate() {
            match phase {
                1 => {
                    publisher.simulate_scenario(SimulateScenario::MoqNetworkHandoff(alternate_ip)).await?;
                    // A TCP signaling socket can break while QUIC migrates.
                    publisher.simulate_scenario(SimulateScenario::SignalReconnect).await?;
                    loop {
                        match local_events.recv().await.ok_or(anyhow!("publisher disconnected"))? {
                            RoomEvent::Reconnected => break,
                            RoomEvent::LocalTrackRepublished { .. } | RoomEvent::Disconnected { .. } =>
                                return Err(anyhow!("publisher restarted during network handoff")),
                            _ => {}
                        }
                    }
                }
                2 => subscriber.simulate_scenario(SimulateScenario::MoqNetworkHandoff(alternate_ip)).await?,
                3 => {
                    publisher.simulate_scenario(SimulateScenario::MoqNetworkHandoff(None)).await?;
                    subscriber.simulate_scenario(SimulateScenario::MoqNetworkHandoff(None)).await?;
                }
                _ => {}
            }
            let payload = vec![luma; 128 * 1024];
            let (mut saw_video, mut saw_data) = (false, false);
            while !saw_video || !saw_data {
                tokio::select! {
                    _ = tick.tick() => {
                        capture_handoff_frame(&source, luma);
                        data.try_push(payload.clone().into())?;
                    }
                    frame = video_stream.next(), if !saw_video => {
                        let frame = frame.ok_or(anyhow!("original video subscription ended"))?;
                        let i420 = frame.buffer.to_i420();
                        saw_video = i420.data().0[0].abs_diff(luma) < 12;
                    }
                    frame = data_stream.next(), if !saw_data => {
                        saw_data = frame.ok_or(anyhow!("original data subscription ended"))?.payload()[..] == payload;
                    }
                }
            }
            assert_eq!(publisher.local_participant().sid(), publisher_sid);
            assert_eq!(subscriber.local_participant().sid(), subscriber_sid);
        }
        publisher.close().await?;
        subscriber.close().await?;
        Ok(())
    }).await?
}

#[test_log::test(tokio::test)]
#[ignore = "requires a LiveKit server with a TLS MoQ endpoint and DataTracks enabled"]
async fn full_reconnect_republishes_video_and_data() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut options = TestRoomOptions::default();
        options.room.transport = RoomTransport::Moq;
        let mut rooms = test_rooms_with_options([options, TestRoomOptions::default()]).await?;
        let (subscriber, mut events) = rooms.pop().unwrap();
        let (publisher, mut local_events) = rooms.pop().unwrap();
        let publisher = Arc::new(publisher);
        let data = publisher.local_participant().publish_data_track("reconnect").await?;
        let mut video = SolidColorTrack::new(
            publisher.clone(),
            SolidColorParams { width: 320, height: 180, luma: 185 },
        );
        video.publish(VideoCodec::VP8, false).await?;

        for phase in 0..2 {
            if phase == 1 {
                publisher.simulate_scenario(livekit::SimulateScenario::FullReconnect).await?;
                loop {
                    match local_events.recv().await {
                        Some(RoomEvent::Reconnected) => break,
                        Some(RoomEvent::Disconnected { reason }) => return Err(anyhow!("publisher disconnected: {reason:?}")),
                        None => return Err(anyhow!("publisher closed during restart")),
                        _ => {}
                    }
                }
            }
            let mut remote_video = None;
            let mut remote_data = None;
            while remote_video.is_none() || remote_data.is_none() {
                match events.recv().await.ok_or(anyhow!("subscriber closed during restart"))? {
                    RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. } => remote_video = Some(track),
                    RoomEvent::DataTrackPublished(track) => remote_data = Some(track),
                    _ => {}
                }
            }
            let mut stream = NativeVideoStream::new(remote_video.unwrap().rtc_track());
            let frame = stream.next().await.ok_or(anyhow!("republished video ended"))?;
            let pixels = frame.buffer.to_i420();
            assert_eq!((pixels.width(), pixels.height()), (320, 180));
            assert!(pixels.data().0[0] > 155);
            let mut subscription = remote_data.unwrap().subscribe().await?;
            let payload = vec![phase + 1; 128 * 1024];
            let mut tick = tokio::time::interval(Duration::from_millis(80));
            loop {
                tokio::select! {
                    _ = tick.tick() => data.try_push(payload.clone().into())?,
                    frame = subscription.next() => {
                        assert_eq!(&frame.ok_or(anyhow!("republished DataTrack ended"))?.payload()[..], &payload);
                        break;
                    }
                }
            }
        }
        video.unpublish().await?;
        publisher.close().await?;
        subscriber.close().await?;
        Ok(())
    }).await?
}

#[test_log::test(tokio::test)]
#[ignore = "requires a LiveKit server with a TLS MoQ endpoint and DataTracks enabled"]
async fn grant_video_and_data_after_join() -> Result<()> {
    use livekit_api::services::room::{RoomClient, UpdateParticipantOptions};
    tokio::time::timeout(Duration::from_secs(40), async {
        let mut options = TestRoomOptions::default();
        options.room.transport = RoomTransport::Moq;
        options.grants.can_subscribe = Some(false);
        options.grants.can_publish_data = Some(false);
        let mut rooms = test_rooms_with_options([options, TestRoomOptions::default()]).await?;
        let (webrtc, mut rtc_events) = rooms.pop().unwrap();
        let (moq, mut moq_events) = rooms.pop().unwrap();
        let sid = moq.local_participant().sid();
        let webrtc = Arc::new(webrtc);
        let mut video = SolidColorTrack::new(webrtc.clone(), SolidColorParams { width: 320, height: 180, luma: 185 });
        video.publish(VideoCodec::VP8, false).await?;
        let url = std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "http://localhost:7880".into());
        let client = RoomClient::with_api_key(
            &url.replacen("ws", "http", 1),
            &std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into()),
            &std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into()),
        );
        let mut permission = moq.local_participant().permission().ok_or(anyhow!("missing permissions"))?;
        permission.can_subscribe = true;
        permission.can_publish_data = true;
        client.update_participant(&moq.name(), moq.local_participant().identity().as_str(),
            UpdateParticipantOptions { permission: Some(permission.clone()), ..Default::default() }).await?;

        let track = loop {
            match moq_events.recv().await.ok_or(anyhow!("MoQ room ended"))? {
                RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), .. } => break track,
                RoomEvent::Reconnecting => return Err(anyhow!("permission update restarted the room")),
                _ => {}
            }
        };
        let mut stream = NativeVideoStream::new(track.rtc_track());
        let frame = stream.next().await.ok_or(anyhow!("video ended after promotion"))?;
        assert_eq!(frame.buffer.width(), 320);
        let local = moq.local_participant().publish_data_track("promoted").await?;
        let remote = loop {
            if let RoomEvent::DataTrackPublished(track) = rtc_events.recv().await.ok_or(anyhow!("WebRTC room ended"))? {
                break track;
            }
        };
        let mut subscription = remote.subscribe().await?;
        let payload = vec![37; 128 * 1024];
        let mut tick = tokio::time::interval(Duration::from_millis(80));
        loop {
            tokio::select! {
                _ = tick.tick() => local.try_push(payload.clone().into())?,
                frame = subscription.next() => {
                    assert_eq!(&frame.ok_or(anyhow!("DataTrack ended after promotion"))?.payload()[..], &payload);
                    break;
                }
            }
        }
        permission.can_subscribe = false;
        client.update_participant(&moq.name(), moq.local_participant().identity().as_str(),
            UpdateParticipantOptions { permission: Some(permission.clone()), ..Default::default() }).await?;
        loop {
            match moq_events.recv().await.ok_or(anyhow!("MoQ room ended during revocation"))? {
                RoomEvent::TrackUnsubscribed { publication, .. } if publication.sid() == track.sid() => break,
                RoomEvent::Reconnecting => return Err(anyhow!("revocation restarted the room")),
                _ => {}
            }
        }
        permission.can_subscribe = true;
        client.update_participant(&moq.name(), moq.local_participant().identity().as_str(),
            UpdateParticipantOptions { permission: Some(permission), ..Default::default() }).await?;
        loop {
            match moq_events.recv().await.ok_or(anyhow!("MoQ room ended during regrant"))? {
                RoomEvent::TrackSubscribed { track: RemoteTrack::Video(restored), .. } if restored.sid() == track.sid() => {
                    let mut frames = NativeVideoStream::new(restored.rtc_track());
                    let frame = tokio::time::timeout(Duration::from_secs(15), frames.next())
                        .await.map_err(|_| anyhow!("resubscribed video did not deliver a decoded frame"))?
                        .ok_or(anyhow!("restored video ended"))?;
                    assert_eq!(frame.buffer.width(), 320);
                    break;
                }
                RoomEvent::Reconnecting => return Err(anyhow!("regrant restarted the room")),
                _ => {}
            }
        }
        assert_eq!(sid, moq.local_participant().sid());
        video.unpublish().await?;
        moq.close().await?;
        webrtc.close().await?;
        Ok(())
    }).await?
}

/// Consume a standard catalog rendition using moqtap directly, without the SDK's
/// MoQ media engine or the LiveKit extension fields of the MSF catalog.
#[tokio::test]
#[ignore = "requires a LiveKit server with a TLS MoQ endpoint"]
async fn independent_subscriber() -> Result<()> {
    use livekit_signaling::{SignalClient, SignalOptions};
    use livekit_token::{AccessToken, VideoGrants};
    use moqtap_client::draft23::connection::{ClientConfig, Connection, TransportType};
    use moqtap_codec::{
        dispatch::AnySubgroupHeader,
        draft23::message::ControlMessage,
        kvp::{KeyValuePair, KvpValue},
        types::TrackNamespace,
        varint::VarInt,
        version::DraftVersion,
    };
    use serde::Deserialize;
    use url::{Position, Url};

    #[derive(Deserialize)]
    struct Catalog {
        version: String,
        tracks: Vec<Track>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Track {
        name: String,
        codec: String,
        role: String,
        packaging: String,
        packaging_metadata: String,
    }

    tokio::time::timeout(Duration::from_secs(30), async {
        let mut rooms = common::test_rooms(1).await?;
        let publisher = Arc::new(rooms.pop().unwrap().0);
        let mut video = SolidColorTrack::new(
            publisher.clone(),
            SolidColorParams { width: 320, height: 180, luma: 185 },
        );
        video.publish(VideoCodec::VP8, false).await?;
        let key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| "devkey".into());
        let secret = std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| "secret".into());
        let url = std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".into());
        let token = AccessToken::with_api_key(&key, &secret)
            .with_identity("standards-reader")
            .with_grants(VideoGrants {
                room: publisher.name(),
                room_join: true,
                can_publish: Some(false),
                can_publish_data: Some(false),
                can_subscribe: Some(true),
                ..Default::default()
            })
            .to_jwt()?;
        let mut options = SignalOptions::default();
        options.use_moq = true;
        options.auto_subscribe = true;
        let (ws, join, _events) = SignalClient::connect(&url, &token, options, None).await?;
        let info = join.moq.ok_or(anyhow!("missing MoQ ticket"))?;
        let mut endpoint = Url::parse(&info.endpoint)?;
        let authority = endpoint[Position::BeforeHost..Position::AfterPort].to_owned();
        endpoint
            .query_pairs_mut()
            .append_pair("room", &publisher.name())
            .append_pair("sid", &info.participant_sid)
            .append_pair("generation", &info.generation.to_string())
            .append_pair("ticket", &info.attachment_token);
        let mut cfg = ClientConfig::new(DraftVersion::Draft23, TransportType::Quic)
            .with_setup_parameters(vec![
                KeyValuePair {
                    key: VarInt::from_u64(1)?,
                    value: KvpValue::Bytes(endpoint[Position::BeforePath..].as_bytes().to_vec()),
                },
                KeyValuePair {
                    key: VarInt::from_u64(5)?,
                    value: KvpValue::Bytes(authority.as_bytes().to_vec()),
                },
            ]);
        if let Some(ca) = std::env::var_os("LIVEKIT_MOQ_TEST_CA_DER") {
            cfg = cfg.with_ca_certs(vec![std::fs::read(ca)?]);
        }
        let mut conn = Connection::connect(&authority, cfg).await?;
        let namespace =
            TrackNamespace(vec![b"livekit".to_vec(), info.participant_sid.into_bytes()]);
        let mut upload = conn
            .publish(
                namespace.clone(),
                b"dt/up/DTR_denied".to_vec(),
                VarInt::from_u64(1)?,
                vec![],
                vec![],
            )
            .await?;
        assert!(matches!(
            conn.recv_on_request_stream(&mut upload).await?,
            ControlMessage::RequestError(_)
        ));
        let other = TrackNamespace(vec![
            b"livekit".to_vec(),
            publisher.local_participant().sid().to_string().into_bytes(),
        ]);
        let mut foreign_catalog = conn.subscribe(other, b"catalog".to_vec(), vec![]).await?;
        assert!(matches!(
            conn.recv_on_request_stream(&mut foreign_catalog).await?,
            ControlMessage::RequestError(_)
        ));
        let mut catalog_request =
            conn.subscribe(namespace.clone(), b"catalog".to_vec(), vec![]).await?;
        let ControlMessage::SubscribeOk(catalog_ack) =
            conn.recv_on_request_stream(&mut catalog_request).await?
        else {
            return Err(anyhow!("catalog refused"));
        };
        let track = loop {
            let (AnySubgroupHeader::Draft23(h), mut stream) = conn.accept_subgroup_stream().await?;
            assert_eq!(h.track_alias, catalog_ack.track_alias);
            let catalog: Catalog =
                serde_json::from_slice(&stream.read_subgroup_object().await?.payload)?;
            assert_eq!(catalog.version, "draft-02");
            if let Some(t) =
                catalog.tracks.into_iter().find(|t| t.role == "video" && t.codec == "vp8")
            {
                break t;
            }
        };
        assert_eq!(track.packaging, "loc");
        assert_eq!(track.packaging_metadata, "streamMapping=object");
        let mut request = conn.subscribe(namespace, track.name.into_bytes(), vec![]).await?;
        let ControlMessage::SubscribeOk(ack) = conn.recv_on_request_stream(&mut request).await?
        else {
            return Err(anyhow!("media refused"));
        };
        loop {
            let (AnySubgroupHeader::Draft23(h), mut stream) = conn.accept_subgroup_stream().await?;
            if h.track_alias != ack.track_alias {
                continue;
            }
            let frame = stream.read_subgroup_object().await?;
            if frame.object_id.into_inner() != 0 {
                continue;
            }
            // RFC 6386 uncompressed VP8 keyframe header and coded dimensions.
            assert_eq!(frame.payload[0] & 1, 0);
            assert_eq!(&frame.payload[3..6], &[0x9d, 0x01, 0x2a]);
            assert_eq!(u16::from_le_bytes(frame.payload[6..8].try_into()?) & 0x3fff, 320);
            assert_eq!(u16::from_le_bytes(frame.payload[8..10].try_into()?) & 0x3fff, 180);
            break;
        }
        conn.close(0, b"test complete");
        ws.close().await;
        video.unpublish().await?;
        publisher.close().await?;
        Ok(())
    })
    .await?
}

/// A publisher can wait for a viewer before capturing its first video frame.
/// With limit.subscription_limit_video=1, also exercise refusal and recovery.
#[test_log::test(tokio::test)]
#[ignore = "requires a live MoQ SFU"]
async fn first_subscriber_and_video_subscription_limit() -> Result<()> {
    use libwebrtc::{
        prelude::{RtcVideoSource, VideoResolution},
        video_source::native::NativeVideoSource,
    };
    use livekit::options::TrackPublishOptions;

    tokio::time::timeout(Duration::from_secs(30), async {
        let mut options = TestRoomOptions::default();
        options.room.transport = RoomTransport::Moq;
        let mut rooms = test_rooms_with_options([options.clone(), options]).await?;
        let (subscriber, mut events) = rooms.pop().unwrap();
        let (publisher, mut publisher_events) = rooms.pop().unwrap();
        let make_video = |name| {
            let source = NativeVideoSource::new(VideoResolution { width: 320, height: 180 }, false);
            LocalTrack::Video(LocalVideoTrack::create_video_track(
                name,
                RtcVideoSource::Native(source),
            ))
        };
        let first = publisher
            .local_participant()
            .publish_track(make_video("first"), TrackPublishOptions::default())
            .await?;
        loop {
            match publisher_events.recv().await.ok_or(anyhow!("publisher closed"))? {
                RoomEvent::LocalTrackSubscribed { track } if track.sid() == first.sid() => break,
                _ => {}
            }
        }
        let mut attached = false;
        let mut quality = false;
        while !attached || !quality {
            match events.recv().await.ok_or(anyhow!("subscriber closed"))? {
                RoomEvent::TrackSubscribed { publication, .. }
                    if publication.sid() == first.sid() =>
                {
                    attached = true
                }
                RoomEvent::ConnectionQualityChanged { participant, .. }
                    if participant.sid() == publisher.local_participant().sid() =>
                {
                    quality = true
                }
                _ => {}
            }
        }

        let second = publisher
            .local_participant()
            .publish_track(make_video("second"), TrackPublishOptions::default())
            .await?;
        let expect_limit = std::env::var_os("LIVEKIT_MOQ_TEST_SINGLE_VIDEO").is_some();
        let mut refused = false;
        loop {
            match events.recv().await.ok_or(anyhow!("subscriber closed"))? {
                RoomEvent::TrackSubscriptionFailed { track_sid, participant, error }
                    if track_sid == second.sid() =>
                {
                    assert!(expect_limit && !refused, "unexpected subscription failure: {error}");
                    assert_eq!(participant.sid(), publisher.local_participant().sid());
                    assert!(matches!(error, livekit::track::TrackError::MoqSubscription(_)));
                    refused = true;
                    publisher.local_participant().unpublish_track(&first.sid()).await?;
                }
                RoomEvent::TrackSubscribed { publication, .. }
                    if publication.sid() == second.sid() =>
                {
                    break
                }
                RoomEvent::Reconnecting => {
                    return Err(anyhow!("a single rendition restarted the room"))
                }
                _ => {}
            }
        }
        assert_eq!(refused, expect_limit);
        subscriber.close().await?;
        publisher.close().await?;
        Ok(())
    })
    .await?
}
