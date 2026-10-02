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

//! Everything holding [`crate::server::audio_stream::AudioStream`] onto the protobuf path:
//! the handle bridge, the request handlers, and the event senders that forward what
//! `next()` yields.
//!
//! This whole file goes when the protobuf path does. What it leaves behind over there is
//! the `handle_id` field on the struct, and the `pub(crate)` these reach through.

use livekit::{
    registered_audio_filter_plugin, track as lk, webrtc::prelude::*, AudioFilterStreamInfo,
};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::server::audio_stream::{
    AudioFilterInfo, AudioFilterSetup, AudioFrameBuffer, AudioStream, AudioStreamError,
    AudioStreamInner,
};
use crate::server::room::Track;
use crate::server::utils;
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

crate::migrate_from_ffi!(AudioStream);

impl AudioFrameBuffer {
    /// Publishes the samples to the FFI handle map, and describes them the way the
    /// protobuf path expects: a pointer into samples the returned handle keeps alive.
    pub(super) fn into_ffi(
        self,
        server: &'static server::FfiServer,
    ) -> (FfiHandleId, proto::AudioFrameBufferInfo) {
        let frame = AudioFrame::from(self);
        let info = proto::AudioFrameBufferInfo::from(&frame);
        let handle_id = server.next_id();
        server.store_handle(handle_id, frame);
        (handle_id, info)
    }
}

impl From<AudioStreamError> for FfiError {
    fn from(err: AudioStreamError) -> Self {
        FfiError::InvalidRequest(err.to_string().into())
    }
}

impl AudioStream {
    /// Forwards frames to the FFI event bus until either handle is dropped or the track ends.
    ///
    /// This is the protobuf path as an adapter over [`AudioStream::next`]: one puller, one
    /// re-framing, and the samples reach the handle map only on their way out.
    async fn pump(
        server: &'static server::FfiServer,
        stream: Arc<Self>,
        stream_handle: FfiHandleId,
        mut stream_dropped_rx: oneshot::Receiver<()>,
        mut track_dropped_rx: oneshot::Receiver<()>,
        send_eos: bool,
    ) {
        loop {
            tokio::select! {
                _ = &mut stream_dropped_rx => break,
                _ = &mut track_dropped_rx => break,
                frame = stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };
                    send_frame(server, stream_handle, frame);
                }
            }
        }

        if send_eos {
            send_eos_event(server, stream_handle);
        }
    }

    /// Setup a new AudioStream and forward the audio data to the client/the foreign
    /// language.
    ///
    /// The forwarding task stops when either the stream handle or the track handle is
    /// dropped. It holds an `Arc` of the stream, so the handle going away is what ends
    /// it — the stream itself only drops once the task has let go.
    ///
    /// It is possible that the client receives an AudioFrame after the task is closed. The
    /// client musts ignore it.
    pub fn from_track_ffi(
        server: &'static server::FfiServer,
        new_stream: proto::NewAudioStreamRequest,
    ) -> FfiResult<proto::OwnedAudioStream> {
        if new_stream.r#type() != proto::AudioStreamType::AudioStreamNative {
            return Err(FfiError::InvalidRequest("unsupported audio stream type".into()));
        }

        let track_handle = new_stream.track_handle;
        let stream = Self::from_track(
            Track::of_handle(server, track_handle)?,
            new_stream.sample_rate,
            new_stream.num_channels,
            new_stream.frame_size_ms,
            new_stream.queue_size_frames,
            new_stream.audio_filter_module_id,
            new_stream.audio_filter_options,
        )?;

        let handle_id = stream.clone().ffi_handle_id();
        let task = server.async_runtime.spawn(Self::pump(
            server,
            stream,
            handle_id,
            server.watch_handle_dropped(handle_id),
            server.watch_handle_dropped(track_handle),
            true,
        ));
        server.watch_panic(task);

        Ok(proto::OwnedAudioStream {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: audio_stream_info(),
        })
    }

    /// Follows a participant's track source, restarting the stream each time the track is
    /// republished.
    ///
    /// Still protobuf-only: the object holds no sink of its own, so [`AudioStream::next`]
    /// yields nothing on it. Migrating it means teaching `next` to wait out a track change
    /// without reporting end-of-stream, which is its own layer.
    pub fn from_participant(
        server: &'static server::FfiServer,
        request: proto::AudioStreamFromParticipantRequest,
    ) -> FfiResult<proto::OwnedAudioStream> {
        if request.r#type() != proto::AudioStreamType::AudioStreamNative {
            return Err(FfiError::InvalidRequest("unsupported audio stream type".into()));
        }

        let stream = Arc::new(Self::over(
            None,
            request.sample_rate.unwrap_or(48000),
            request.num_channels.unwrap_or(1),
            request.frame_size_ms,
        ));

        let handle_id = stream.ffi_handle_id();
        let task = server.async_runtime.spawn(Self::participant_audio_stream_task(
            server,
            request,
            handle_id,
            server.watch_handle_dropped(handle_id),
        ));
        server.watch_panic(task);

        Ok(proto::OwnedAudioStream {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info: audio_stream_info(),
        })
    }

    async fn participant_audio_stream_task(
        server: &'static server::FfiServer,
        request: proto::AudioStreamFromParticipantRequest,
        stream_handle: FfiHandleId,
        mut close_rx: oneshot::Receiver<()>,
    ) {
        let ffi_participant =
            utils::ffi_participant_from_handle(server, request.participant_handle);
        let ffi_participant = match ffi_participant {
            Ok(ffi_participant) => ffi_participant,
            Err(err) => {
                log::error!("failed to get participant: {}", err);
                return;
            }
        };

        let track_source = request.track_source();
        let (track_tx, mut track_rx) = mpsc::channel::<lk::Track>(1);
        let (track_finished_tx, _track_finished_rx) = broadcast::channel::<lk::Track>(1);
        server.async_runtime.spawn(utils::track_changed_trigger(
            ffi_participant.clone(),
            track_source.into(),
            track_tx,
            track_finished_tx.clone(),
        ));
        // track_tx is no longer held, so the track_rx will be closed when track_changed_trigger is done

        let room = &ffi_participant.room;
        let url = room.url();
        let room_sid = room.room.maybe_sid().map(|sid| sid.to_string()).unwrap_or_default();
        let room_name = room.room.name();
        let participant_identity = ffi_participant.participant.identity();
        let participant_id = ffi_participant.participant.sid();
        let plugin =
            request.audio_filter_module_id.as_deref().and_then(registered_audio_filter_plugin);

        let sample_rate = request.sample_rate.unwrap_or(48000);
        let num_channels = request.num_channels.unwrap_or(1);

        loop {
            let track = track_rx.recv().await;
            if let Some(track) = track {
                let rtc_track = track.rtc_track();
                let MediaStreamTrack::Audio(rtc_track) = rtc_track else {
                    continue;
                };

                let (c_tx, c_rx) = oneshot::channel::<()>();
                let (handle_dropped_tx, handle_dropped_rx) = oneshot::channel::<()>();
                let (done_tx, mut done_rx) = oneshot::channel::<()>();

                let mut track_finished_rx = track_finished_tx.subscribe();
                let track_sid = track.sid();
                server.async_runtime.spawn(async move {
                    tokio::select! {
                            t = track_finished_rx.recv() => {
                            let Ok(t) = t else {
                                return
                            };
                            if t.sid() == track_sid {
                                handle_dropped_tx.send(()).ok();
                                return
                            }
                        }
                    }
                });

                // Unlike the track-sourced path, a filter here needs options as well as a
                // module id: there is no default to fall back on.
                let filter = plugin.clone().zip(request.audio_filter_options.clone()).map(
                    |(plugin, options)| {
                        let stream_info = AudioFilterStreamInfo {
                            url: url.clone(),
                            room_id: room_sid.clone(),
                            room_name: room_name.clone(),
                            participant_identity: participant_identity.clone().into(),
                            participant_id: participant_id.clone().into(),
                            track_id: track.sid().into(),
                        };
                        AudioFilterSetup {
                            plugin,
                            options,
                            info: AudioFilterInfo { stream_info, room_handle: room.handle_id },
                        }
                    },
                );

                // One stream per track, pumped by the same loop as a track-sourced stream.
                // It is never published as a handle: the outer stream owns the id.
                let per_track = Arc::new(Self::over(
                    Some(AudioStreamInner::over(
                        rtc_track,
                        track.codec_clock_rate(),
                        sample_rate,
                        num_channels,
                        request.queue_size_frames,
                        filter,
                    )),
                    sample_rate,
                    num_channels,
                    request.frame_size_ms,
                ));
                server.async_runtime.spawn(async move {
                    Self::pump(server, per_track, stream_handle, c_rx, handle_dropped_rx, false)
                        .await;
                    let _ = done_tx.send(());
                });
                tokio::select! {
                    _ = &mut close_rx => {
                        let _ = c_tx.send(());
                        return
                    }
                    _ = &mut done_rx => {
                        continue
                    }
                }
            } else {
                // when tracks are done (i.e. the participant leaves the room), we are done
                break;
            }
        }

        send_eos_event(server, stream_handle);
    }
}

/// Every stream this crate builds is native: `from_track` and `from_participant` reject
/// the browser-side types.
fn audio_stream_info() -> proto::AudioStreamInfo {
    proto::AudioStreamInfo { r#type: proto::AudioStreamType::AudioStreamNative as i32 }
}

fn send_frame(
    server: &'static server::FfiServer,
    stream_handle: FfiHandleId,
    frame: AudioFrameBuffer,
) {
    let (handle_id, info) = frame.into_ffi(server);

    if let Err(err) = server.send_event(
        proto::AudioStreamEvent {
            stream_handle,
            message: Some(
                proto::AudioFrameReceived {
                    frame: proto::OwnedAudioFrameBuffer {
                        handle: proto::FfiOwnedHandle { id: handle_id },
                        info,
                    },
                }
                .into(),
            ),
        }
        .into(),
    ) {
        server.drop_handle(handle_id);
        log::warn!("failed to send audio frame: {}", err);
    }
}

fn send_eos_event(server: &'static server::FfiServer, stream_handle: FfiHandleId) {
    if let Err(err) = server.send_event(
        proto::AudioStreamEvent { stream_handle, message: Some(proto::AudioStreamEos {}.into()) }
            .into(),
    ) {
        log::warn!("failed to send audio eos: {}", err);
    }
}

/// Direct-call tests for [`AudioStream`] crossing the FFI seam, paired with the
/// protobuf-driven tests in `crate::migration_tests`.
///
/// The probe for "one object, two surfaces" is the stream's own sink: a frame pulled
/// through one `Arc` is gone from the other, the way the resampler's filter state is the
/// probe over there. It needs real audio, so these drive a local audio track end to end.
#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::FFI_SERVER;
    use livekit::prelude::LocalAudioTrack;
    use livekit::webrtc::audio_source::{
        native::NativeAudioSource, AudioSourceOptions, RtcAudioSource,
    };
    use std::borrow::Cow;
    use std::time::Duration;

    const SAMPLE_RATE: u32 = 48000;
    /// 10ms of mono, which is the frame size the WebRTC sink works in.
    const FRAME_SAMPLES: usize = SAMPLE_RATE as usize / 100;

    /// A track fed by a source the test drives directly, the way `create_audio_track`
    /// builds one.
    fn audio_track() -> (NativeAudioSource, Arc<Track>) {
        let source = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, 1000);
        let track =
            LocalAudioTrack::create_audio_track("probe", RtcAudioSource::Native(source.clone()));
        (source, Arc::new(Track::over(lk::Track::LocalAudio(track), None)))
    }

    /// Captures one 10ms frame at a steady level, tagged so it can be told apart from the
    /// silence the sink delivers when nothing has been captured.
    async fn capture(source: &NativeAudioSource, level: i16) {
        assert_ne!(level, 0, "silence is indistinguishable from an idle sink");
        let frame = AudioFrame {
            data: Cow::Owned(vec![level; FRAME_SAMPLES]),
            sample_rate: SAMPLE_RATE,
            num_channels: 1,
            samples_per_channel: FRAME_SAMPLES as u32,
        };
        source.capture_frame(&frame).await.expect("the source took the frame");
    }

    /// The next frame carrying something other than silence.
    async fn next_tagged(stream: &AudioStream) -> AudioFrameBuffer {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let frame = stream.next().await.expect("the stream ended");
                if frame.data.iter().any(|sample| *sample != 0) {
                    return frame;
                }
            }
        })
        .await
        .expect("timed out waiting for a tagged frame")
    }

    fn stream_over(track: &Arc<Track>, frame_size_ms: Option<u32>) -> Arc<AudioStream> {
        AudioStream::from_track(
            track.clone(),
            Some(SAMPLE_RATE),
            Some(1),
            frame_size_ms,
            None,
            None,
            None,
        )
        .expect("the track is an audio track")
    }

    /// A stream created over uniffi publishes a handle, the handle resolves back to the
    /// very same object, and the two `Arc`s share one sink: pulling through either
    /// advances it for both. `take_ffi_handle_id` hands sole ownership back, and
    /// publishing again restores the same id with the stream still running.
    #[serial_test::parallel]
    #[test]
    fn an_audio_stream_hands_back_and_forth_across_the_seam() {
        FFI_SERVER.async_runtime.block_on(async {
            let (source, track) = audio_track();
            let stream = stream_over(&track, None);
            assert_eq!(Arc::strong_count(&stream), 1, "an unpublished stream has no other owner");

            let handle = stream.clone().ffi_handle_id();
            let same = AudioStream::from_ffi_handle_id(handle).expect("the published id resolves");
            assert!(Arc::ptr_eq(&same, &stream), "the id resolves to the object that published it");

            // A second stream over the same track, to show the check below discriminates.
            let rival = stream_over(&track, None);

            // One frame, and the two Arcs go looking for it. Being one object they share
            // the one sink, so the pull through `stream` takes the frame out from under
            // `same`.
            capture(&source, 10).await;
            assert_eq!(next_tagged(&stream).await.data[0], 10);
            // `rival` is what the assertion below would look like if they were two
            // objects: a separate stream is its own sink, and still holds that frame. It
            // is drained first because the sink's queue is bounded, and the silence the
            // source keeps producing during that wait would push the frame out of it.
            assert_eq!(next_tagged(&rival).await.data[0], 10);
            assert!(
                tokio::time::timeout(Duration::from_millis(500), next_tagged(&same)).await.is_err(),
                "the frame `stream` consumed is gone from `same`"
            );

            // The FFI side lets go, and the uniffi side carries on alone.
            stream.take_ffi_handle_id().expect("the FFI side co-owned the stream");
            assert!(
                AudioStream::from_ffi_handle_id(handle).is_err(),
                "a released handle is absent from the map until it is published again"
            );
            capture(&source, 30).await;
            assert_eq!(next_tagged(&stream).await.data[0], 30);

            // Publishing again restores the handle, id and all, over the same live sink.
            assert_eq!(stream.clone().ffi_handle_id(), handle, "republishing keeps the id");
            let republished =
                AudioStream::from_ffi_handle_id(handle).expect("the republished handle resolves");
            assert!(Arc::ptr_eq(&republished, &stream));
            capture(&source, 40).await;
            assert_eq!(next_tagged(&republished).await.data[0], 40);

            FFI_SERVER.drop_handle(handle);
        });
    }

    /// `frame_size_ms` re-cuts the sink's 10ms frames on the way out; without it they
    /// arrive as the sink produced them.
    #[serial_test::parallel]
    #[test]
    fn frames_are_recut_to_the_size_asked_for() {
        FFI_SERVER.async_runtime.block_on(async {
            let (source, track) = audio_track();
            let as_is = stream_over(&track, None);
            let in_thirties = stream_over(&track, Some(30));

            for level in 1..=6 {
                capture(&source, level).await;
            }

            let frame = next_tagged(&as_is).await;
            assert_eq!(frame.data.len(), FRAME_SAMPLES, "the sink's own frame, untouched");
            assert_eq!(frame.samples_per_channel, FRAME_SAMPLES as u32);

            let frame = next_tagged(&in_thirties).await;
            assert_eq!(frame.data.len(), FRAME_SAMPLES * 3, "three of the sink's frames");
            assert_eq!(frame.samples_per_channel, FRAME_SAMPLES as u32 * 3);
            assert_eq!(frame.sample_rate, SAMPLE_RATE);
        });
    }

    /// An [`AudioFrameBuffer`] has no handle of its own — it is a record, so the samples
    /// are what crosses rather than a reference to an object both sides share. What the
    /// FFI side gets is a handle owning those very samples, with the info addressing them
    /// in place.
    #[serial_test::parallel]
    #[test]
    fn a_buffer_publishes_the_very_samples_it_carried() {
        let buffer = AudioFrameBuffer {
            sample_rate: 48000,
            num_channels: 2,
            samples_per_channel: 4,
            data: vec![1, -1, 2, -2, 3, -3, 4, -4],
        };
        let carried = buffer.data.clone();

        let (handle, info) = buffer.into_ffi(&FFI_SERVER);

        let owned = FFI_SERVER
            .retrieve_handle::<AudioFrame<'static>>(handle)
            .expect("the samples are published");
        assert_eq!(&*owned.data, &carried[..], "the handle owns the samples the record carried");
        assert_eq!(info.data_ptr, owned.data.as_ptr() as u64, "the info addresses them in place");
        assert_eq!(info.num_channels, 2);
        assert_eq!(info.samples_per_channel, 4);

        // ...and the foreign side reads them back through the pointer it was handed.
        let samples =
            unsafe { std::slice::from_raw_parts(info.data_ptr as *const i16, carried.len()) };
        assert_eq!(samples, &carried[..]);
        drop(owned);

        FFI_SERVER.drop_handle(handle);
    }
}
