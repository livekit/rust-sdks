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

//! Protobuf-driven tests for the types being migrated to uniffi.
//!
//! The FFI request surface is the contract that must not move while the types
//! behind it grow uniffi annotations, so these tests drive it the way the
//! bindings do — encoded requests in, handles and raw pointers out — and check
//! the result against the same work done through the Rust API directly. The
//! direct-call half of each pair lives next to the code it exercises, e.g.
//! `server::resampler::migration_tests`.
//!
//! They are written to compile and pass either side of the migration, so they
//! can be replayed on the commit before it.

use std::{mem::size_of, slice};

use crate::{proto, server::requests::handle_request, FfiHandleId, FFI_SERVER};

fn request(message: impl Into<proto::ffi_request::Message>) -> proto::ffi_response::Message {
    let response = handle_request(&FFI_SERVER, proto::FfiRequest { message: Some(message.into()) })
        .expect("request failed");
    response.message.expect("response carried no message")
}

fn new_sox_resampler(
    input_rate: f64,
    output_rate: f64,
    num_channels: u32,
    quality: proto::SoxQualityRecipe,
) -> FfiHandleId {
    let response = match request(proto::NewSoxResamplerRequest {
        input_rate,
        output_rate,
        num_channels,
        input_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
        output_data_type: proto::SoxResamplerDataType::SoxrDatatypeInt16i as i32,
        quality_recipe: quality as i32,
        flags: None,
    }) {
        proto::ffi_response::Message::NewSoxResampler(response) => response,
        _ => panic!("expected a NewSoxResampler response"),
    };

    match response.message.expect("response carried no message") {
        proto::new_sox_resampler_response::Message::Resampler(resampler) => resampler.handle.id,
        proto::new_sox_resampler_response::Message::Error(error) => panic!("{error}"),
    }
}

fn push_sox_resampler(handle: FfiHandleId, input: &[i16]) -> Vec<i16> {
    let response = match request(proto::PushSoxResamplerRequest {
        resampler_handle: handle,
        data_ptr: input.as_ptr() as u64,
        size: (input.len() * size_of::<i16>()) as u32,
    }) {
        proto::ffi_response::Message::PushSoxResampler(response) => response,
        _ => panic!("expected a PushSoxResampler response"),
    };

    assert_eq!(response.error, None, "push failed");
    read_output(response.output_ptr, response.size)
}

fn flush_sox_resampler(handle: FfiHandleId) -> Vec<i16> {
    let response = match request(proto::FlushSoxResamplerRequest { resampler_handle: handle }) {
        proto::ffi_response::Message::FlushSoxResampler(response) => response,
        _ => panic!("expected a FlushSoxResampler response"),
    };

    assert_eq!(response.error, None, "flush failed");
    read_output(response.output_ptr, response.size)
}

/// The pointer/size pair the bindings dereference: `size` counts bytes, the
/// samples are `i16`, a run that produced nothing is a null pointer, and the
/// samples stay readable until the next call on the same resampler.
fn read_output(output_ptr: u64, size: u32) -> Vec<i16> {
    assert_eq!(size as usize % size_of::<i16>(), 0, "size {size} is not whole i16 samples");
    if output_ptr == 0 || size == 0 {
        return Vec::new();
    }
    unsafe { slice::from_raw_parts(output_ptr as *const i16, size as usize / size_of::<i16>()) }
        .to_vec()
}

/// soxr dithers its int16 output, so two instances fed identical input agree to
/// within a LSB or so rather than bit for bit.
fn assert_samples_match(actual: &[i16], expected: &[i16], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: output length");
    for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (*actual as i32 - *expected as i32).abs() <= 2,
            "{context}: sample {i} is {actual}, expected ~{expected}"
        );
    }
}

/// Every knob of `NewSoxResamplerRequest` reaches the resampler: the same input
/// pushed through the FFI request surface and through the Rust API must come
/// back as the same audio, for each rate, channel count and quality recipe.
#[serial_test::serial]
#[test]
fn proto_push_matches_direct_push() {
    let cases = [
        (48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick),
        (48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityVeryhigh),
        (16000.0, 48000.0, 1, proto::SoxQualityRecipe::SoxrQualityMedium),
        (48000.0, 24000.0, 2, proto::SoxQualityRecipe::SoxrQualityHigh),
    ];

    for (input_rate, output_rate, num_channels, quality) in cases {
        let input: Vec<i16> = (0..960 * num_channels as i16)
            .map(|i| (i % 97) * 300 - 14000) // a repeating sawtooth, loud enough to compare
            .collect();

        let handle = new_sox_resampler(input_rate, output_rate, num_channels, quality);
        #[allow(unused_mut)]
        let mut direct = crate::sox_resampler!(input_rate, output_rate, num_channels, quality);

        let context = format!("{input_rate} -> {output_rate}, {num_channels}ch, {quality:?}");
        for _ in 0..3 {
            assert_samples_match(
                &push_sox_resampler(handle, &input),
                &direct.push(&input).unwrap(),
                &format!("push at {context}"),
            );
        }
        assert_samples_match(
            &flush_sox_resampler(handle),
            &direct.flush().unwrap(),
            &format!("flush at {context}"),
        );

        FFI_SERVER.drop_handle(handle);
    }
}

/// 30ms in at 48kHz is 30ms out at 16kHz, counted in bytes off the wire.
#[serial_test::serial]
#[test]
fn proto_push_then_flush_conserves_duration() {
    let handle = new_sox_resampler(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick);
    let frame = vec![1000i16; 480]; // 10ms @ 48kHz

    let mut frames = 0;
    for _ in 0..3 {
        frames += push_sox_resampler(handle, &frame).len();
    }
    frames += flush_sox_resampler(handle).len();

    assert!((frames as i64 - 480).abs() <= 8, "got {frames} frames, expected ~480");

    FFI_SERVER.drop_handle(handle);
}

/// The FFI handle map is a *second* owner of a migrated object, and it only
/// takes that ownership when `ffi_handle_id()` publishes an id. A resampler
/// that never crosses into the FFI path must die with its last `Arc`.
#[serial_test::serial]
#[test]
fn the_handle_map_only_owns_published_resamplers() {
    use crate::{server::resampler::SoxResampler, sox_resampler};
    use std::sync::Arc;

    let resampler = sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityQuick);
    assert_eq!(Arc::strong_count(&resampler), 1, "an unpublished resampler has no other owner");

    let handle = resampler.clone().ffi_handle_id();
    assert_eq!(
        Arc::strong_count(&resampler),
        2,
        "publishing an id makes the FFI server a co-owner"
    );

    drop(resampler);
    assert!(
        FFI_SERVER.retrieve_handle::<Arc<SoxResampler>>(handle).is_ok(),
        "the FFI server holds the resampler until its handle is dropped"
    );

    FFI_SERVER.drop_handle(handle);
    assert!(FFI_SERVER.retrieve_handle::<Arc<SoxResampler>>(handle).is_err());
}

/// A resampler that has just been fed a loud block is still ringing: pushing
/// silence into it comes back non-silent until the filter drains. A *different*
/// resampler, fed the same silence, answers with silence — so this is the probe
/// for "the two surfaces are driving one object", and it needs a quality recipe
/// with a filter long enough to ring.
fn assert_still_ringing(output: &[i16], context: &str) {
    let peak = output.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    assert!(peak > 1000, "{context}: peak is {peak}, expected the tail of the loud block");
}

/// A resampler created over the FFI can be picked up by the uniffi side: the id
/// round-trips, and the uniffi side sees the filter state the FFI side left.
#[serial_test::serial]
#[test]
fn a_proto_resampler_is_the_same_object_over_uniffi() {
    use crate::server::resampler::SoxResampler;

    let quality = proto::SoxQualityRecipe::SoxrQualityVeryhigh;
    let handle = new_sox_resampler(48000.0, 16000.0, 1, quality);
    let resampler = SoxResampler::from_ffi_handle_id(handle).expect("handle resolves to an object");
    assert_eq!(resampler.clone().ffi_handle_id(), handle, "the object keeps the published id");

    push_sox_resampler(handle, &vec![8000i16; 4800]); // 100ms of loud, over the FFI
    assert_still_ringing(
        &resampler.push(&vec![0i16; 4800]).unwrap(),
        "silence pushed over uniffi after a loud block over the FFI",
    );

    FFI_SERVER.drop_handle(handle);
}

/// The same trip the other way, and back again: a resampler created over uniffi
/// publishes a handle the FFI request surface drives, `take_ffi_handle_id` hands
/// sole ownership back to the uniffi side, and publishing a second time restores
/// the same id — so a migrating object can cross the seam as often as it needs to.
#[serial_test::serial]
#[test]
fn a_resampler_hands_back_and_forth_across_the_seam() {
    use crate::{server::resampler::SoxResampler, sox_resampler};
    use std::sync::Arc;

    let loud = vec![8000i16; 4800]; // 100ms @ 48kHz
    let silence = vec![0i16; 4800];
    let resampler =
        sox_resampler!(48000.0, 16000.0, 1, proto::SoxQualityRecipe::SoxrQualityVeryhigh);

    // uniffi drives it first
    assert_still_ringing(&resampler.push(&loud).unwrap(), "push over uniffi");

    // published, the FFI side picks up the filter uniffi left behind
    let handle = resampler.clone().ffi_handle_id();
    assert_still_ringing(
        &push_sox_resampler(handle, &silence),
        "silence pushed over the FFI after a loud block over uniffi",
    );

    // the FFI side lets go, and the uniffi side carries on alone
    resampler.take_ffi_handle_id().expect("the FFI side co-owned the resampler");
    assert!(
        SoxResampler::from_ffi_handle_id(handle).is_err(),
        "a released handle is absent from the map until it is published again"
    );
    assert_still_ringing(&resampler.push(&loud).unwrap(), "push over uniffi after the take");

    // publishing again restores the handle, id and all
    assert_eq!(resampler.clone().ffi_handle_id(), handle, "republishing keeps the id");
    assert_still_ringing(
        &push_sox_resampler(handle, &silence),
        "silence pushed over the FFI after republishing",
    );

    // and the id resolves back to the very same object
    let same = SoxResampler::from_ffi_handle_id(handle).expect("the republished handle resolves");
    assert!(Arc::ptr_eq(&same, &resampler), "the handle resolves to the object that published it");
    same.push(&loud).expect("uniffi still drives it at the end of the round trip");

    FFI_SERVER.drop_handle(handle);
}

/// Protobuf-driven tests for data tracks, the next types to be migrated.
///
/// Unlike the resampler, a data track only exists inside a connected room, so
/// these run against a LiveKit server (`livekit-server --dev` with
/// `enable_data_tracks: true`), the same as the `livekit` crate's e2e tests.
#[cfg(feature = "__lk-e2e-test")]
pub(crate) mod data_track {
    use super::request;
    use crate::{
        proto,
        server::{requests::handle_request, FfiConfig},
        FfiHandleId, FFI_SERVER,
    };
    use livekit_token::{AccessToken, VideoGrants};
    use std::{
        env,
        sync::{Arc, OnceLock},
        time::Duration,
    };
    use tokio::sync::broadcast;

    const EVENT_TIMEOUT: Duration = Duration::from_secs(15);

    /// Routes every event the FFI server emits into a broadcast channel, so a test
    /// can subscribe before it sends the request that triggers the event.
    pub(crate) fn events() -> broadcast::Receiver<proto::ffi_event::Message> {
        static EVENTS: OnceLock<broadcast::Sender<proto::ffi_event::Message>> = OnceLock::new();
        EVENTS
            .get_or_init(|| {
                let (tx, _) = broadcast::channel(4096);
                let callback_tx = tx.clone();
                FFI_SERVER.setup(FfiConfig {
                    callback_fn: Arc::new(move |event: proto::FfiEvent| {
                        if let Some(message) = event.message {
                            let _ = callback_tx.send(message);
                        }
                    }),
                    capture_logs: false,
                    sdk: "livekit-ffi-tests".into(),
                    sdk_version: env!("CARGO_PKG_VERSION").into(),
                });
                tx
            })
            .subscribe()
    }

    /// Waits for the first event `filter` maps to `Some`, skipping the rest.
    pub(crate) async fn next_event<T>(
        events: &mut broadcast::Receiver<proto::ffi_event::Message>,
        mut filter: impl FnMut(proto::ffi_event::Message) -> Option<T>,
    ) -> T {
        tokio::time::timeout(EVENT_TIMEOUT, async {
            loop {
                let event = events.recv().await.expect("event channel closed or lagged");
                if let Some(value) = filter(event) {
                    return value;
                }
            }
        })
        .await
        .expect("timed out waiting for an FFI event")
    }

    /// A room connection made over the FFI request surface.
    pub(crate) struct Participant {
        pub room_handle: FfiHandleId,
        pub local_participant_handle: FfiHandleId,
        pub identity: String,
    }

    /// Connects `count` participants to a fresh room through `ConnectRequest`s.
    pub(crate) async fn connect(count: usize) -> Vec<Participant> {
        let api_key = env::var("LIVEKIT_API_KEY").unwrap_or("devkey".into());
        let api_secret = env::var("LIVEKIT_API_SECRET").unwrap_or("secret".into());
        let url = env::var("LIVEKIT_URL").unwrap_or("http://localhost:7880".into());
        let room_name = format!("ffi_test_room_{}", FFI_SERVER.next_id());

        let mut events = events();
        let mut participants = Vec::with_capacity(count);
        for i in 0..count {
            let identity = format!("p{i}");
            let token = AccessToken::with_api_key(&api_key, &api_secret)
                .with_identity(&identity)
                .with_grants(VideoGrants {
                    room_join: true,
                    room: room_name.clone(),
                    ..Default::default()
                })
                .to_jwt()
                .unwrap();

            let async_id = match request(proto::ConnectRequest {
                url: url.clone(),
                token,
                options: Default::default(),
                request_async_id: None,
            }) {
                proto::ffi_response::Message::Connect(res) => res.async_id,
                _ => panic!("expected a Connect response"),
            };
            let result = next_event(&mut events, |event| match event {
                proto::ffi_event::Message::Connect(cb) if cb.async_id == async_id => cb.message,
                _ => None,
            })
            .await;
            let result = match result {
                proto::connect_callback::Message::Result(result) => result,
                proto::connect_callback::Message::Error(error) => panic!("connect failed: {error}"),
            };
            // The server holds room events back until the client says it has
            // registered the room, as the bindings do on receiving the callback.
            request(proto::ReadyForRoomEventRequest { room_handle: result.room.handle.id });
            participants.push(Participant {
                room_handle: result.room.handle.id,
                local_participant_handle: result.local_participant.handle.id,
                identity,
            });
        }
        participants
    }

    /// Disconnects every participant and waits for the callbacks.
    pub(crate) async fn disconnect(participants: Vec<Participant>) {
        let mut events = events();
        for participant in participants {
            let async_id = match request(proto::DisconnectRequest {
                room_handle: participant.room_handle,
                request_async_id: None,
                reason: None,
            }) {
                proto::ffi_response::Message::Disconnect(res) => res.async_id,
                _ => panic!("expected a Disconnect response"),
            };
            next_event(&mut events, |event| match event {
                proto::ffi_event::Message::Disconnect(cb) if cb.async_id == async_id => Some(()),
                _ => None,
            })
            .await;
            FFI_SERVER.drop_handle(participant.room_handle);
            FFI_SERVER.drop_handle(participant.local_participant_handle);
        }
    }

    pub(crate) async fn publish(
        participant: &Participant,
        name: &str,
    ) -> Result<proto::OwnedLocalDataTrack, proto::PublishDataTrackError> {
        let mut events = events();
        let async_id = match request(proto::PublishDataTrackRequest {
            local_participant_handle: participant.local_participant_handle,
            options: proto::DataTrackOptions { name: name.into(), ..Default::default() },
            request_async_id: None,
        }) {
            proto::ffi_response::Message::PublishDataTrack(res) => res.async_id,
            _ => panic!("expected a PublishDataTrack response"),
        };
        let result = next_event(&mut events, |event| match event {
            proto::ffi_event::Message::PublishDataTrack(cb) if cb.async_id == async_id => cb.result,
            _ => None,
        })
        .await;
        match result {
            proto::publish_data_track_callback::Result::Track(track) => Ok(track),
            proto::publish_data_track_callback::Result::Error(error) => Err(error),
        }
    }

    /// Waits for `room_handle` to see a remote data track named `name`.
    pub(crate) async fn remote_track_published(
        events: &mut broadcast::Receiver<proto::ffi_event::Message>,
        room_handle: FfiHandleId,
        name: &str,
    ) -> proto::OwnedRemoteDataTrack {
        next_event(events, |event| match event {
            proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
                room_handle: handle,
                message: Some(proto::room_event::Message::DataTrackPublished(event)),
            }) if handle == room_handle && event.track.info.name == name => Some(event.track),
            _ => None,
        })
        .await
    }

    /// Waits for `room_handle` to see the remote data track `sid` unpublished.
    pub(crate) async fn remote_track_unpublished(
        events: &mut broadcast::Receiver<proto::ffi_event::Message>,
        room_handle: FfiHandleId,
        sid: &str,
    ) {
        next_event(events, |event| match event {
            proto::ffi_event::Message::RoomEvent(proto::RoomEvent {
                room_handle: handle,
                message: Some(proto::room_event::Message::DataTrackUnpublished(event)),
            }) if handle == room_handle && event.sid == sid => Some(()),
            _ => None,
        })
        .await
    }

    pub(crate) fn local_is_published(track_handle: FfiHandleId) -> bool {
        match request(proto::LocalDataTrackIsPublishedRequest { track_handle }) {
            proto::ffi_response::Message::LocalDataTrackIsPublished(res) => res.is_published,
            _ => panic!("expected a LocalDataTrackIsPublished response"),
        }
    }

    pub(crate) fn remote_is_published(track_handle: FfiHandleId) -> bool {
        match request(proto::RemoteDataTrackIsPublishedRequest { track_handle }) {
            proto::ffi_response::Message::RemoteDataTrackIsPublished(res) => res.is_published,
            _ => panic!("expected a RemoteDataTrackIsPublished response"),
        }
    }

    pub(crate) fn try_push(
        track_handle: FfiHandleId,
        payload: &[u8],
    ) -> Option<proto::LocalDataTrackTryPushError> {
        match request(proto::LocalDataTrackTryPushRequest {
            track_handle,
            frame: proto::DataTrackFrame { payload: payload.to_vec(), user_timestamp: None },
        }) {
            proto::ffi_response::Message::LocalDataTrackTryPush(res) => res.error,
            _ => panic!("expected a LocalDataTrackTryPush response"),
        }
    }

    pub(crate) fn subscribe(track_handle: FfiHandleId) -> FfiHandleId {
        match request(proto::SubscribeDataTrackRequest {
            track_handle,
            options: Default::default(),
        }) {
            proto::ffi_response::Message::SubscribeDataTrack(res) => res.stream.handle.id,
            _ => panic!("expected a SubscribeDataTrack response"),
        }
    }

    /// One read on a stream: the next frame, or the end-of-stream event.
    pub(crate) async fn read(
        events: &mut broadcast::Receiver<proto::ffi_event::Message>,
        stream_handle: FfiHandleId,
    ) -> proto::data_track_stream_event::Detail {
        match request(proto::DataTrackStreamReadRequest { stream_handle }) {
            proto::ffi_response::Message::DataTrackStreamRead(res) => {
                if let Some(eos) = res.eos_event {
                    return eos.into();
                }
            }
            _ => panic!("expected a DataTrackStreamRead response"),
        }
        next_event(events, |event| match event {
            proto::ffi_event::Message::DataTrackStreamEvent(event)
                if event.stream_handle == stream_handle =>
            {
                event.detail
            }
            _ => None,
        })
        .await
    }

    /// Reads until a frame arrives. The subscription is set up asynchronously, so
    /// frames pushed before it lands are dropped; keep pushing until one gets through.
    pub(crate) async fn push_until_received(
        events: &mut broadcast::Receiver<proto::ffi_event::Message>,
        push: impl Fn(),
        stream_handle: FfiHandleId,
    ) -> proto::DataTrackFrame {
        match request(proto::DataTrackStreamReadRequest { stream_handle }) {
            proto::ffi_response::Message::DataTrackStreamRead(res) => {
                assert!(res.eos_event.is_none(), "stream ended before the first frame")
            }
            _ => panic!("expected a DataTrackStreamRead response"),
        }
        tokio::time::timeout(EVENT_TIMEOUT, async {
            loop {
                push();
                let received = tokio::time::timeout(Duration::from_millis(200), async {
                    loop {
                        match events.recv().await.expect("event channel closed or lagged") {
                            proto::ffi_event::Message::DataTrackStreamEvent(event)
                                if event.stream_handle == stream_handle =>
                            {
                                return event.detail
                            }
                            _ => continue,
                        }
                    }
                })
                .await;
                match received {
                    Ok(Some(proto::data_track_stream_event::Detail::FrameReceived(frame))) => {
                        return frame.frame
                    }
                    Ok(Some(proto::data_track_stream_event::Detail::Eos(eos))) => {
                        panic!("stream ended before the first frame: {eos:?}")
                    }
                    Ok(None) | Err(_) => continue,
                }
            }
        })
        .await
        .expect("timed out waiting for a frame")
    }

    /// A track published over the FFI reaches the other participant with the
    /// same info, and frames pushed over the FFI arrive on an FFI subscription,
    /// one per read request.
    #[serial_test::serial]
    #[tokio::test]
    async fn proto_publish_subscribe_and_push() {
        let participants = connect(2).await;
        let (publisher, subscriber) = (&participants[0], &participants[1]);
        let mut events = events();

        let local = publish(publisher, "proto-track").await.expect("publish succeeds");
        assert_eq!(local.info.name, "proto-track");
        assert!(!local.info.sid.is_empty(), "the SFU assigns a sid");
        assert!(local_is_published(local.handle.id));

        let remote =
            remote_track_published(&mut events, subscriber.room_handle, "proto-track").await;
        assert_eq!(remote.info.sid, local.info.sid);
        assert_eq!(remote.publisher_identity, publisher.identity);
        assert!(remote_is_published(remote.handle.id));

        let pipeline = request(proto::RemoteDataTrackSetPipelineOptionsRequest {
            track_handle: remote.handle.id,
            options: proto::RemoteDataTrackPipelineOptions { max_partial_frames: Some(4) },
        });
        assert!(matches!(
            pipeline,
            proto::ffi_response::Message::RemoteDataTrackSetPipelineOptions(_)
        ));

        let stream = subscribe(remote.handle.id);
        let first = push_until_received(
            &mut events,
            || assert_eq!(try_push(local.handle.id, b"first"), None),
            stream,
        )
        .await;
        assert_eq!(first.payload, b"first");

        // Once flowing, a frame pushed per read arrives per read, in order.
        for i in 0..5u8 {
            assert_eq!(try_push(local.handle.id, &[i; 64]), None);
            match read(&mut events, stream).await {
                proto::data_track_stream_event::Detail::FrameReceived(frame) => {
                    assert_eq!(frame.frame.payload, vec![i; 64])
                }
                proto::data_track_stream_event::Detail::Eos(eos) => panic!("early eos: {eos:?}"),
            }
        }

        FFI_SERVER.drop_handle(stream);
        FFI_SERVER.drop_handle(remote.handle.id);
        FFI_SERVER.drop_handle(local.handle.id);
        disconnect(participants).await;
    }

    /// `LocalDataTrackUnpublishRequest` unpublishes on both sides, ends the
    /// remote stream, and later pushes fail with `TrackUnpublished`.
    #[serial_test::serial]
    #[tokio::test]
    async fn proto_unpublish_ends_the_stream() {
        let participants = connect(2).await;
        let (publisher, subscriber) = (&participants[0], &participants[1]);
        let mut events = events();

        let local = publish(publisher, "unpublish-track").await.expect("publish succeeds");
        let remote =
            remote_track_published(&mut events, subscriber.room_handle, "unpublish-track").await;
        let stream = subscribe(remote.handle.id);
        push_until_received(&mut events, || _ = try_push(local.handle.id, b"x"), stream).await;

        match request(proto::LocalDataTrackUnpublishRequest { track_handle: local.handle.id }) {
            proto::ffi_response::Message::LocalDataTrackUnpublish(_) => {}
            _ => panic!("expected a LocalDataTrackUnpublish response"),
        }
        assert!(!local_is_published(local.handle.id));
        let error = try_push(local.handle.id, b"late").expect("pushing to an unpublished track");
        assert_eq!(error.code, proto::LocalDataTrackTryPushErrorCode::TrackUnpublished as i32);
        assert!(!error.message.is_empty());

        remote_track_unpublished(&mut events, subscriber.room_handle, &local.info.sid).await;
        assert!(!remote_is_published(remote.handle.id));

        // Reads drain to an end-of-stream that carries no error, and keep reporting it.
        loop {
            match read(&mut events, stream).await {
                proto::data_track_stream_event::Detail::FrameReceived(_) => continue,
                proto::data_track_stream_event::Detail::Eos(eos) => {
                    assert_eq!(eos.error, None);
                    break;
                }
            }
        }
        match request(proto::DataTrackStreamReadRequest { stream_handle: stream }) {
            proto::ffi_response::Message::DataTrackStreamRead(res) => {
                assert_eq!(res.eos_event, Some(proto::DataTrackStreamEos { error: None }))
            }
            _ => panic!("expected a DataTrackStreamRead response"),
        }

        FFI_SERVER.drop_handle(stream);
        FFI_SERVER.drop_handle(remote.handle.id);
        FFI_SERVER.drop_handle(local.handle.id);
        disconnect(participants).await;
    }

    /// Dropping the only handle to a local track unpublishes it: the FFI handle
    /// map is that track's sole owner.
    #[serial_test::serial]
    #[tokio::test]
    async fn proto_dropping_the_local_handle_unpublishes() {
        let participants = connect(2).await;
        let (publisher, subscriber) = (&participants[0], &participants[1]);
        let mut events = events();

        let local = publish(publisher, "drop-track").await.expect("publish succeeds");
        let remote =
            remote_track_published(&mut events, subscriber.room_handle, "drop-track").await;

        assert!(FFI_SERVER.drop_handle(local.handle.id));
        remote_track_unpublished(&mut events, subscriber.room_handle, &local.info.sid).await;
        assert!(!remote_is_published(remote.handle.id));

        FFI_SERVER.drop_handle(remote.handle.id);
        disconnect(participants).await;
    }

    /// Publishing a second track under a taken name fails with `DuplicateName`.
    #[serial_test::serial]
    #[tokio::test]
    async fn proto_publish_duplicate_name_fails() {
        let participants = connect(1).await;

        let local = publish(&participants[0], "dup").await.expect("first publish succeeds");
        let error = publish(&participants[0], "dup").await.expect_err("second publish fails");
        assert_eq!(error.code, proto::PublishDataTrackErrorCode::DuplicateName as i32);

        FFI_SERVER.drop_handle(local.handle.id);
        disconnect(participants).await;
    }

    /// Each request resolves its handle as the type it expects: a remote track
    /// handle is not a local track, a local track is not a stream, and a dropped
    /// handle resolves to nothing.
    #[serial_test::serial]
    #[tokio::test]
    async fn proto_handles_resolve_only_as_their_own_type() {
        let participants = connect(2).await;
        let (publisher, subscriber) = (&participants[0], &participants[1]);
        let mut events = events();

        let local = publish(publisher, "typed").await.expect("publish succeeds");
        let remote = remote_track_published(&mut events, subscriber.room_handle, "typed").await;

        let fails = |message: proto::ffi_request::Message| {
            handle_request(&FFI_SERVER, proto::FfiRequest { message: Some(message) }).is_err()
        };
        assert!(fails(
            proto::LocalDataTrackIsPublishedRequest { track_handle: remote.handle.id }.into()
        ));
        assert!(fails(
            proto::RemoteDataTrackIsPublishedRequest { track_handle: local.handle.id }.into()
        ));
        assert!(fails(
            proto::SubscribeDataTrackRequest {
                track_handle: local.handle.id,
                options: Default::default()
            }
            .into()
        ));
        assert!(fails(
            proto::DataTrackStreamReadRequest { stream_handle: remote.handle.id }.into()
        ));

        FFI_SERVER.drop_handle(remote.handle.id);
        assert!(fails(
            proto::RemoteDataTrackIsPublishedRequest { track_handle: remote.handle.id }.into()
        ));

        FFI_SERVER.drop_handle(local.handle.id);
        disconnect(participants).await;
    }
}
