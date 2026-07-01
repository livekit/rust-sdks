// Copyright 2025 LiveKit, Inc.
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

#[cfg(feature = "__lk-e2e-test")]
use {
    crate::common::test_rooms,
    anyhow::{anyhow, Ok, Result},
    chrono::{TimeDelta, Utc},
    libwebrtc::native::create_random_uuid,
    livekit::{RoomEvent, StreamByteOptions, StreamReader, StreamTextOptions},
    std::time::Duration,
    tokio::{time::timeout, try_join},
};

mod common;

#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_send_bytes() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (_, mut receiving_event_rx) = rooms.pop().unwrap();
    let sender_identity = sending_room.local_participant().identity();

    const BYTES_TO_SEND: &[u8] = &[0xFA; 16];

    let send_text = async move {
        let options = StreamByteOptions { topic: "some-topic".into(), ..Default::default() };
        let stream_info =
            sending_room.local_participant().send_bytes(BYTES_TO_SEND, options).await?;

        assert!(!stream_info.id.is_empty());
        assert!(
            stream_info.timestamp.signed_duration_since(Utc::now()).abs() <= TimeDelta::seconds(1)
        );
        assert!(stream_info.total_length.is_some());
        assert_eq!(stream_info.mime_type, "application/octet-stream");
        assert_eq!(stream_info.topic, "some-topic");
        assert_eq!(stream_info.is_compressed, false);
        assert_eq!(stream_info.is_inline, true);

        Ok(())
    };
    let receive_text = async move {
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::ByteStreamOpened { reader, topic, participant_identity } = event else {
                continue;
            };
            assert_eq!(topic, "some-topic");
            assert_eq!(participant_identity, sender_identity);

            let Some(reader) = reader.take() else {
                return Err(anyhow!("Failed to take reader"));
            };
            assert_eq!(reader.read_all().await?, BYTES_TO_SEND);
            break;
        }
        Ok(())
    };

    timeout(Duration::from_secs(5), async { try_join!(send_text, receive_text) }).await??;
    Ok(())
}

/// End-to-end round-trip of a large, somewhat-compressible text. Both peers are Rust SDK
/// clients advertising data streams v2 + deflate-raw, so the sender compresses across multiple
/// chunks and the receiver decompresses — validating the v2 compression path on the real wire.
#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_send_large_compressible_text() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (_, mut receiving_event_rx) = rooms.pop().unwrap();

    // ~50 KB of deterministic pseudo-random lowercase: too big to inline, compresses well
    // under its raw size, exercising the chunked-compressed path.
    let mut text = String::new();
    let mut state: u64 = 0x1234_5678_9abc_def0;
    for _ in 0..50_000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        text.push((b'a' + ((state >> 33) % 26) as u8) as char);
    }
    let expected = text.clone();

    let send = async move {
        let options = StreamTextOptions { topic: "some-topic".into(), ..Default::default() };
        let stream_info = sending_room.local_participant().send_text(&text, options).await?;
        assert_eq!(stream_info.is_compressed, true);
        assert_eq!(stream_info.is_inline, false);
        Ok(())
    };
    let receive = async move {
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::TextStreamOpened { reader, topic, .. } = event else {
                continue;
            };
            assert_eq!(topic, "some-topic");
            let reader = reader.take().ok_or_else(|| anyhow!("Failed to take reader"))?;
            assert_eq!(reader.read_all().await?, expected);
            break;
        }
        Ok(())
    };

    timeout(Duration::from_secs(10), async { try_join!(send, receive) }).await??;
    Ok(())
}

/// End-to-end round-trip of a large in-memory byte payload, validating the v2 byte-stream path.
#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_send_large_incompressible_random_bytes() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (_, mut receiving_event_rx) = rooms.pop().unwrap();

    let payload: Vec<u8> = (0..50_000).flat_map(|_| create_random_uuid().into_bytes()).collect();
    let expected = payload.clone();

    let send = async move {
        let options = StreamByteOptions { topic: "some-topic".into(), ..Default::default() };
        let stream_info = sending_room.local_participant().send_bytes(&payload, options).await?;
        assert_eq!(stream_info.is_compressed, false);
        assert_eq!(stream_info.is_inline, false);
        Ok(())
    };
    let receive = async move {
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::ByteStreamOpened { reader, topic, .. } = event else {
                continue;
            };
            assert_eq!(topic, "some-topic");
            let reader = reader.take().ok_or_else(|| anyhow!("Failed to take reader"))?;
            assert_eq!(reader.read_all().await?, expected);
            break;
        }
        Ok(())
    };

    timeout(Duration::from_secs(10), async { try_join!(send, receive) }).await??;
    Ok(())
}

/// End-to-end round-trip of a large in-memory byte payload, validating the v2 byte-stream path.
#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_send_large_bytes() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (_, mut receiving_event_rx) = rooms.pop().unwrap();

    let payload: Vec<u8> = (0..50_000u32).map(|i| (i % 251) as u8).collect();
    let expected = payload.clone();

    let send = async move {
        let options = StreamByteOptions { topic: "some-topic".into(), ..Default::default() };
        let stream_info = sending_room.local_participant().send_bytes(&payload, options).await?;
        assert_eq!(stream_info.is_compressed, true);
        assert_eq!(stream_info.is_inline, false);
        Ok(())
    };
    let receive = async move {
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::ByteStreamOpened { reader, topic, .. } = event else {
                continue;
            };
            assert_eq!(topic, "some-topic");
            let reader = reader.take().ok_or_else(|| anyhow!("Failed to take reader"))?;
            assert_eq!(reader.read_all().await?, expected);
            break;
        }
        Ok(())
    };

    timeout(Duration::from_secs(10), async { try_join!(send, receive) }).await??;
    Ok(())
}

#[cfg(feature = "__lk-e2e-test")]
#[tokio::test]
async fn test_send_text() -> Result<()> {
    let mut rooms = test_rooms(2).await?;
    let (sending_room, _) = rooms.pop().unwrap();
    let (_, mut receiving_event_rx) = rooms.pop().unwrap();
    let sender_identity = sending_room.local_participant().identity();

    const TEXT_TO_SEND: &str = "some-text";

    let send_text = async move {
        let options = StreamTextOptions { topic: "some-topic".into(), ..Default::default() };
        let stream_info = sending_room.local_participant().send_text(TEXT_TO_SEND, options).await?;

        assert!(!stream_info.id.is_empty());
        assert!(
            stream_info.timestamp.signed_duration_since(Utc::now()).abs() <= TimeDelta::seconds(1)
        );
        assert!(stream_info.total_length.is_some());
        assert_eq!(stream_info.mime_type, "text/plain");
        assert_eq!(stream_info.topic, "some-topic");

        Ok(())
    };
    let receive_text = async move {
        while let Some(event) = receiving_event_rx.recv().await {
            let RoomEvent::TextStreamOpened { reader, topic, participant_identity } = event else {
                continue;
            };
            assert_eq!(topic, "some-topic");
            assert_eq!(participant_identity, sender_identity);

            let Some(reader) = reader.take() else {
                return Err(anyhow!("Failed to take reader"));
            };
            assert_eq!(reader.read_all().await?, TEXT_TO_SEND);
            break;
        }
        Ok(())
    };

    timeout(Duration::from_secs(5), async { try_join!(send_text, receive_text) }).await??;
    Ok(())
}
