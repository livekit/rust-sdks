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

//! End-to-end tests of the public API against a running
//! `livekit-server --dev`, with a second room standing in for the agent.
//! The tests that assert on the crate's internals are in `src/e2e_tests.rs`.
//!
//! Environment variables (the same as `livekit`'s e2e suites):
//! - LIVEKIT_URL (default ws://localhost:7880)
//! - LIVEKIT_API_KEY (default "devkey")
//! - LIVEKIT_API_SECRET (default "secret")
//!
//! Run:
//!   cargo test -p livekit-session --features __lk-e2e-test

#![cfg(feature = "__lk-e2e-test")]

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::prelude::{AudioSourceOptions, RtcVideoSource, VideoResolution};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::{SendTextOptions, StreamReader, StreamTextOptions, StreamWriter};
use livekit_session::*;
use tokio::time::timeout;

use common::*;

mod common;

// The topics and attributes of the agent's side of the protocol, spelled out
// so that the tests pin the wire names independently of the crate's own
// constants.
const TOPIC_CHAT: &str = "lk.chat";
const TOPIC_TRANSCRIPTION: &str = "lk.transcription";
const ATTRIBUTE_SEGMENT_ID: &str = "lk.segment_id";
const ATTRIBUTE_TRANSCRIPTION_FINAL: &str = "lk.transcription_final";

#[tokio::test(flavor = "multi_thread")]
async fn start_publishes_the_microphone_and_end_releases_the_audio_devices() -> Result<()> {
    // Shares the platform audio handle with the session, to observe the
    // session's reference to it.
    let Some(probe) = microphone_probe() else {
        return Ok(());
    };
    let env = TestEnv::new();
    let session = Session::new(SessionOptions::new_fixed_source(env.user_source(false)));

    session.start().await?;
    assert_eq!(probe.ref_count(), 2);
    let room = session.room().ok_or_else(|| anyhow!("no room"))?;
    let publications = room.local_participant().track_publications();
    assert!(publications.values().any(|p| p.source() == TrackSource::Microphone));
    drop(room);

    session.end().await;
    assert_eq!(probe.ref_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_leaving_is_reported_as_left() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(text_options(env.user_source(false)));
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;

    agent.close().await?;
    wait_for_agent(&session, Expect::Left).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_fires_without_an_agent_and_a_late_agent_recovers() -> Result<()> {
    let env = TestEnv::new();
    let options =
        text_options(env.user_source(true)).with_agent_connect_timeout(Duration::from_millis(500));
    let session = Session::new(options);
    session.start().await?;
    wait_for_agent(&session, Expect::TimedOut).await?;

    let (_agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_is_armed_without_dispatch() -> Result<()> {
    let env = TestEnv::new();
    let options =
        text_options(env.user_source(false)).with_agent_connect_timeout(Duration::from_millis(500));
    // An agent registered without a name is dispatched automatically, so a
    // token naming none still waits for one.
    let session = Session::new(options);
    session.start().await?;
    wait_for_agent(&session, Expect::TimedOut).await?;
    session.end().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_fires_for_an_agent_stuck_initializing() -> Result<()> {
    let env = TestEnv::new();
    let options =
        text_options(env.user_source(true)).with_agent_connect_timeout(Duration::from_millis(500));
    // Joined first, so the agent is present before the timeout can fire.
    let (agent, _agent_events) = env.join_agent(AGENT, "initializing").await?;
    let session = Session::new(options);
    session.start().await?;
    wait_for_agent(&session, Expect::TimedOut).await?;

    set_agent_state(&agent, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;
    session.end().await;
    Ok(())
}

async fn next_message(
    events: &mut SessionEventStream,
    mut predicate: impl FnMut(&Message) -> bool,
) -> Result<Message> {
    timeout(WAIT, async {
        while let Some(event) = events.next().await {
            if let SessionEvent::MessageUpserted(message) = event {
                if predicate(&message) {
                    return Ok(message);
                }
            }
        }
        bail!("session events ended")
    })
    .await?
}

#[tokio::test(flavor = "multi_thread")]
async fn messages_merge_chat_and_transcripts() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(text_options(env.user_source(false)));
    let mut events = session.subscribe();
    session.start().await?;
    let (agent, mut agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;

    // User input reaches the agent on lk.chat, under the id the feed uses.
    let sent = session.send_text("hello agent").await?;
    assert_eq!((sent.origin, sent.text.as_str()), (MessageOrigin::UserInput, "hello agent"));
    let received = timeout(WAIT, async {
        while let Some(event) = agent_events.recv().await {
            if let RoomEvent::TextStreamOpened { reader, topic, .. } = event {
                if topic == TOPIC_CHAT {
                    let reader = reader.take().ok_or_else(|| anyhow!("reader taken"))?;
                    let id = reader.info().id.clone();
                    return Ok((id, reader.read_all().await?));
                }
            }
        }
        bail!("agent events ended")
    })
    .await??;
    assert_eq!(received, (sent.id.clone(), "hello agent".to_owned()));

    // An agent transcript appends on one stream, finalized by the trailer.
    let mut options = StreamTextOptions::new_with_topic(TOPIC_TRANSCRIPTION);
    options.attributes = HashMap::from([
        (ATTRIBUTE_SEGMENT_ID.to_owned(), "seg-1".to_owned()),
        (ATTRIBUTE_TRANSCRIPTION_FINAL.to_owned(), "false".to_owned()),
    ]);
    let writer = agent.local_participant().stream_text(options).await?;
    writer.write("Hel").await?;
    writer.write("lo").await?;
    let partial = next_message(&mut events, |m| m.id == "seg-1" && m.text == "Hello").await?;
    assert!(!partial.is_final);
    writer
        .close_with_options(
            None,
            Some(HashMap::from([(ATTRIBUTE_TRANSCRIPTION_FINAL.to_owned(), "true".to_owned())])),
        )
        .await?;
    let done = next_message(&mut events, |m| m.id == "seg-1" && m.is_final).await?;
    assert_eq!(done.origin, MessageOrigin::AgentTranscript);
    assert_eq!(done.text, "Hello");
    assert_eq!(done.seq, partial.seq);
    assert_eq!(done.participant_identity.as_str(), AGENT);

    // Chat from the agent reaches the feed too.
    agent
        .local_participant()
        .send_text("hello user", SendTextOptions::new_with_topic(TOPIC_CHAT))
        .await?;
    let chat = next_message(&mut events, |m| m.origin == MessageOrigin::Chat).await?;
    assert_eq!(chat.text, "hello user");

    let feed: Vec<_> = session.messages().into_iter().map(|m| (m.origin, m.text)).collect();
    assert_eq!(
        feed,
        [
            (MessageOrigin::UserInput, "hello agent".to_owned()),
            (MessageOrigin::AgentTranscript, "Hello".to_owned()),
            (MessageOrigin::Chat, "hello user".to_owned()),
        ]
    );

    // A failed send adds nothing.
    session.end().await;
    assert!(matches!(session.send_text("too late").await, Err(MessageSendError::NotConnected)));
    assert_eq!(session.messages().len(), 3);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn streams_on_other_topics_are_left_to_the_app() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(text_options(env.user_source(false)));
    session.start().await?;
    let mut app_events = session.room().ok_or_else(|| anyhow!("no room"))?.subscribe();
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;

    agent
        .local_participant()
        .send_text("for the app", SendTextOptions::new_with_topic("app.custom"))
        .await?;
    let text = timeout(WAIT, async {
        while let Some(event) = app_events.recv().await {
            if let RoomEvent::TextStreamOpened { reader, topic, .. } = event {
                if topic == "app.custom" {
                    let reader = reader.take().ok_or_else(|| anyhow!("session took the reader"))?;
                    return Ok(reader.read_all().await?);
                }
            }
        }
        bail!("app events ended")
    })
    .await??;
    assert_eq!(text, "for the app");
    session.end().await;
    Ok(())
}

// Agent media

/// An avatar worker whose identity sorts before [`AGENT`], so that only its
/// publish-on-behalf attribute tells it apart from the agent.
const AVATAR: &str = "a-avatar";

/// Publishes a silent audio track from `room` as `source`.
async fn publish_audio(room: &Room, source: TrackSource) -> Result<LocalTrackPublication> {
    let rtc_source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 100);
    let track = LocalAudioTrack::create_audio_track("audio", RtcAudioSource::Native(rtc_source));
    let options = TrackPublishOptions { source, ..Default::default() };
    Ok(room.local_participant().publish_track(LocalTrack::Audio(track), options).await?)
}

/// Publishes a blank video track from `room` as `source`.
async fn publish_video(room: &Room, source: TrackSource) -> Result<LocalTrackPublication> {
    let rtc_source = NativeVideoSource::new(VideoResolution { width: 16, height: 16 }, false);
    let track = LocalVideoTrack::create_video_track("video", RtcVideoSource::Native(rtc_source));
    let options = TrackPublishOptions { source, ..Default::default() };
    Ok(room.local_participant().publish_track(LocalTrack::Video(track), options).await?)
}

/// Whether the session's room has subscribed to the remote track `sid`.
fn is_subscribed(session: &Session, sid: &TrackSid) -> bool {
    let Some(room) = session.room() else {
        return false;
    };
    room.remote_participants()
        .values()
        .any(|p| p.track_publications().get(sid).is_some_and(|p| p.is_subscribed()))
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_audio_track_is_the_agents_microphone_once_subscribed() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(text_options(env.user_source(false)));
    assert!(current(&session).agent.audio_track().is_none());
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;
    assert!(current(&session).agent.audio_track().is_none());

    // Only the microphone is the agent's voice.
    let other = publish_audio(&agent, TrackSource::ScreenshareAudio).await?;
    eventually("the screen share audio to be subscribed", || is_subscribed(&session, &other.sid()))
        .await?;
    assert!(current(&session).agent.audio_track().is_none());

    let microphone = publish_audio(&agent, TrackSource::Microphone).await?;
    let state =
        wait_for(&session, "the agent's audio track", |s| s.agent.audio_track().is_some()).await?;
    assert_eq!(state.agent.audio_track().map(RemoteAudioTrack::sid), Some(microphone.sid()));
    // The track comes with the agent it belongs to, and what that agent does.
    assert_eq!(state.agent.identity().map(ParticipantIdentity::as_str), Some(AGENT));
    assert_eq!(state.agent.activity(), Some(AgentActivity::Listening));
    drop(state);

    agent.local_participant().unpublish_track(&microphone.sid()).await?;
    wait_for(&session, "the agent's audio track to go", |s| s.agent.audio_track().is_none())
        .await?;

    session.end().await;
    assert_eq!(current(&session), SessionState::default());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_avatar_video_track_is_the_avatar_workers_camera_once_subscribed() -> Result<()> {
    let env = TestEnv::new();
    let session = Session::new(text_options(env.user_source(false)));
    session.start().await?;
    let (agent, _agent_events) = env.join_agent(AGENT, "listening").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Listening)).await?;
    assert!(current(&session).agent.avatar_video_track().is_none());

    // The agent's own camera is not an avatar.
    let agent_camera = publish_video(&agent, TrackSource::Camera).await?;
    eventually("the agent's camera to be subscribed", || {
        is_subscribed(&session, &agent_camera.sid())
    })
    .await?;
    assert!(current(&session).agent.avatar_video_track().is_none());

    let (avatar, _avatar_events) = env.join_avatar_worker(AVATAR, AGENT).await?;
    let camera = publish_video(&avatar, TrackSource::Camera).await?;
    let state =
        wait_for(&session, "the avatar's video track", |s| s.agent.avatar_video_track().is_some())
            .await?;
    assert_eq!(state.agent.avatar_video_track().map(RemoteVideoTrack::sid), Some(camera.sid()));
    assert!(state.agent.audio_track().is_none());
    drop(state);

    // The worker is of kind agent too, and reports no agent state. The state
    // still follows the agent, so the worker was not taken for it.
    set_agent_state(&agent, "speaking").await?;
    wait_for_agent(&session, Expect::Joined(AgentActivity::Speaking)).await?;

    avatar.local_participant().unpublish_track(&camera.sid()).await?;
    wait_for(&session, "the avatar's video track to go", |s| {
        s.agent.avatar_video_track().is_none()
    })
    .await?;

    session.end().await;
    assert_eq!(current(&session), SessionState::default());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn timed_out_agent_keeps_its_audio_track() -> Result<()> {
    let env = TestEnv::new();
    let options =
        text_options(env.user_source(true)).with_agent_connect_timeout(Duration::from_millis(500));
    // An agent that never reports a state is idle, so it times out while it
    // is in the room and publishing.
    let (agent, _agent_events) = env.join_agent(AGENT, "idle").await?;
    let session = Session::new(options);
    session.start().await?;
    wait_for_agent(&session, Expect::TimedOut).await?;

    let microphone = publish_audio(&agent, TrackSource::Microphone).await?;
    let state =
        wait_for(&session, "the agent's audio track", |s| s.agent.audio_track().is_some()).await?;
    assert!(Expect::TimedOut.matches(&state.agent));
    assert_eq!(state.agent.audio_track().map(RemoteAudioTrack::sid), Some(microphone.sid()));
    drop(state);

    session.end().await;
    Ok(())
}
