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

use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
    time::Duration,
};

use futures_util::StreamExt;
use livekit::webrtc::{
    audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
    prelude::*,
};
use livekit::{
    registered_audio_filter_plugin, AudioFilterAudioStream, AudioFilterPlugin,
    AudioFilterStreamInfo,
};
use tokio::sync::Mutex;

use super::audio_plugin::AudioStreamKind;
use super::room::{FfiRoom, Track};
use crate::FfiHandleId;

/// Interleaved PCM samples, and the format describing them.
///
/// A record, so uniffi copies every frame's samples across the boundary — about a
/// kilobyte per 10ms frame of 48kHz mono, a hundred times a second, where the protobuf
/// path passes a pointer. Three orders of magnitude under the video buffer's copy.
///
/// TODO: make this an object holding the samples, the way [`crate::server::video_stream::VideoBuffer`]
/// would be, if a profile of a real call says the copy matters.
#[derive(uniffi::Record)]
pub struct AudioFrameBuffer {
    pub sample_rate: u32,
    pub num_channels: u32,
    pub samples_per_channel: u32,
    /// Channels interleaved, `samples_per_channel * num_channels` long.
    pub data: Vec<i16>,
}

impl From<AudioFrame<'_>> for AudioFrameBuffer {
    fn from(frame: AudioFrame<'_>) -> Self {
        Self {
            sample_rate: frame.sample_rate,
            num_channels: frame.num_channels,
            samples_per_channel: frame.samples_per_channel,
            data: frame.data.into_owned(),
        }
    }
}

impl From<AudioFrameBuffer> for AudioFrame<'static> {
    fn from(buffer: AudioFrameBuffer) -> Self {
        Self {
            data: Cow::Owned(buffer.data),
            sample_rate: buffer.sample_rate,
            num_channels: buffer.num_channels,
            samples_per_channel: buffer.samples_per_channel,
        }
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AudioStreamError {
    #[error("{0}")]
    InvalidTrack(String),
    #[error("{0}")]
    AudioFilter(String),
}

/// FFI wrapper around the WebRTC audio sink, with the filter and the re-framing the
/// caller asked for in front of it.
#[derive(uniffi::Object)]
pub struct AudioStream {
    /// Absent on a participant-sourced stream, whose track — and so whose sink — is
    /// replaced as the participant republishes. Those still run on the protobuf path;
    /// see [`AudioStream::from_participant`].
    inner: Mutex<Option<AudioStreamInner>>,
    sample_rate: u32,
    num_channels: u32,
    /// Samples per frame [`AudioStream::next`] hands out, all channels counted. `None`
    /// passes the sink's own frames through.
    target_samples: Option<usize>,
    /// Reached by [`crate::migration::audio_stream`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}

#[uniffi::export(async_runtime = "tokio")]
impl AudioStream {
    /// Opens a stream over an audio track.
    ///
    /// `frame_size_ms` re-cuts the sink's frames to that length; without it they arrive
    /// as the sink produced them. `audio_filter_module_id` names a plugin loaded over the
    /// protobuf path, which is still the only way to load one.
    #[uniffi::constructor]
    pub fn from_track(
        track: Arc<Track>,
        sample_rate: Option<u32>,
        num_channels: Option<u32>,
        frame_size_ms: Option<u32>,
        queue_size_frames: Option<u32>,
        audio_filter_module_id: Option<String>,
        audio_filter_options: Option<String>,
    ) -> Result<Arc<Self>, AudioStreamError> {
        let MediaStreamTrack::Audio(rtc_track) = track.inner.rtc_track() else {
            return Err(AudioStreamError::InvalidTrack("not an audio track".into()));
        };

        let filter = match audio_filter_module_id {
            Some(module_id) => Some(AudioFilterSetup::over_room(
                &module_id,
                audio_filter_options.unwrap_or_default(),
                track.room_handle,
                rtc_track.id(),
            )?),
            None => None,
        };

        let sample_rate = sample_rate.unwrap_or(48000);
        let num_channels = num_channels.unwrap_or(1);
        let inner = AudioStreamInner::over(
            rtc_track,
            track.inner.codec_clock_rate(),
            sample_rate,
            num_channels,
            queue_size_frames,
            filter,
        );
        Ok(Arc::new(Self::over(Some(inner), sample_rate, num_channels, frame_size_ms)))
    }

    /// The next frame, or `None` once the track has ended.
    ///
    /// Participant-sourced streams always yield `None`: they have not migrated.
    ///
    /// One puller at a time, by the mutex. A stream the protobuf path is pumping will
    /// interleave its frames with a foreign caller's: the handle round-trip is for passing
    /// the object across the seam, not for switching how it is consumed mid-flight.
    pub async fn next(&self) -> Option<AudioFrameBuffer> {
        let mut guard = self.inner.lock().await;
        let inner = guard.as_mut()?;
        loop {
            if let Some(frame) = self.take_reframed(inner) {
                return Some(frame);
            }
            let frame = inner.stream.next().await?;
            inner.refresh_filter_info();
            match self.target_samples {
                None => return Some(frame.into()),
                Some(_) => inner.pending.extend_from_slice(&frame.data),
            }
        }
    }
}

impl AudioStream {
    pub(crate) fn over(
        inner: Option<AudioStreamInner>,
        sample_rate: u32,
        num_channels: u32,
        frame_size_ms: Option<u32>,
    ) -> Self {
        Self {
            inner: Mutex::new(inner),
            sample_rate,
            num_channels,
            target_samples: target_samples(sample_rate, num_channels, frame_size_ms),
            handle_id: OnceLock::new(),
        }
    }

    /// A frame's worth of re-cut samples, once enough of them have arrived.
    fn take_reframed(&self, inner: &mut AudioStreamInner) -> Option<AudioFrameBuffer> {
        let target = self.target_samples?;
        if inner.pending.len() < target {
            return None;
        }
        Some(AudioFrameBuffer {
            sample_rate: self.sample_rate,
            num_channels: self.num_channels,
            samples_per_channel: target as u32 / self.num_channels,
            data: inner.pending.drain(..target).collect(),
        })
    }
}

/// Samples per output frame, all channels counted, for a `frame_size_ms` re-cutting.
///
/// `None` passes the sink's own frames through — including for a frame size that works
/// out to no samples at all, since an empty buffer always meets a zero-length target and
/// would hand out empty frames forever without ever pulling the sink.
fn target_samples(
    sample_rate: u32,
    num_channels: u32,
    frame_size_ms: Option<u32>,
) -> Option<usize> {
    frame_size_ms
        .map(|ms| sample_rate as usize * ms as usize / 1000 * num_channels as usize)
        .filter(|target| *target > 0)
}

/// The sink, and what it takes to keep reading from it.
pub(crate) struct AudioStreamInner {
    stream: AudioStreamKind,
    /// Samples pulled from the sink but not yet long enough to make up a frame of the
    /// size the caller asked for. Empty when they did not ask for one.
    pending: Vec<i16>,
    filter_info: Option<AudioFilterInfo>,
}

impl AudioStreamInner {
    /// Builds the WebRTC sink, with a filter session in front of it when one was asked
    /// for and could be created.
    ///
    /// A filter supporting separate rates converts from the codec's rate to the requested
    /// one itself, so the sink runs at the codec rate and the filter does the rest.
    /// Without a live session — none asked for, or creation failed — the sink must run at
    /// the requested rate directly, or codec-rate audio would be forwarded mislabeled as
    /// the output rate, dilating it downstream.
    pub(crate) fn over(
        rtc_track: RtcAudioTrack,
        codec_clock_rate: Option<u32>,
        output_sample_rate: u32,
        num_channels: u32,
        queue_size_frames: Option<u32>,
        filter: Option<AudioFilterSetup>,
    ) -> Self {
        let input_sample_rate = match &filter {
            Some(filter) if filter.plugin.supports_separate_rates() => {
                codec_clock_rate.unwrap_or(48000)
            }
            _ => output_sample_rate,
        };

        let session = filter.as_ref().and_then(|filter| {
            let session = filter.plugin.clone().new_session(
                input_sample_rate,
                output_sample_rate,
                &filter.options,
                filter.info.stream_info.clone(),
            );
            if session.is_none() {
                log::error!(
                    "failed to initialize the audio filter. it will not be enabled for this session."
                );
            }
            session
        });

        let sink_rate = if session.is_some() { input_sample_rate } else { output_sample_rate };
        let native_stream = NativeAudioStream::with_options(
            rtc_track,
            sink_rate as i32,
            num_channels as i32,
            NativeAudioStreamOptions {
                queue_size_frames: queue_size_frames.map(|capacity| capacity as usize),
            },
        );

        let stream = match session {
            Some(session) => AudioStreamKind::Filtered(AudioFilterAudioStream::new(
                native_stream,
                session,
                Duration::from_millis(10),
                input_sample_rate,
                output_sample_rate,
                num_channels,
            )),
            None => AudioStreamKind::Native(native_stream),
        };

        Self {
            stream,
            pending: Vec::new(),
            // Only worth carrying while the room's sid is still unknown.
            filter_info: filter
                .map(|filter| filter.info)
                .filter(|info| info.stream_info.room_id.is_empty()),
        }
    }

    /// Tells the filter session the room's sid, the first frame after it arrives.
    fn refresh_filter_info(&mut self) {
        let Some(info) = self.filter_info.as_mut() else {
            return;
        };
        if !info.room_id_arrived() {
            return;
        }
        if let AudioStreamKind::Filtered(filter) = &mut self.stream {
            filter.update_stream_info(info.stream_info.clone());
        }
        self.filter_info = None;
    }
}

/// What it takes to put a filter plugin in front of a sink: which plugin, how it is
/// configured, and the stream it is told it is filtering.
pub(crate) struct AudioFilterSetup {
    pub(crate) plugin: Arc<AudioFilterPlugin>,
    pub(crate) options: String,
    pub(crate) info: AudioFilterInfo,
}

impl AudioFilterSetup {
    /// Resolves the plugin and describes the stream from the room the track belongs to.
    fn over_room(
        module_id: &str,
        options: String,
        room_handle: Option<FfiHandleId>,
        track_id: String,
    ) -> Result<Self, AudioStreamError> {
        let Some(room_handle) = room_handle else {
            return Err(AudioStreamError::AudioFilter("this track has no room information".into()));
        };
        let room = crate::FFI_SERVER
            .retrieve_handle::<FfiRoom>(room_handle)
            .map_err(|err| AudioStreamError::AudioFilter(err.to_string()))?
            .clone();
        let Some(plugin) = registered_audio_filter_plugin(module_id) else {
            return Err(AudioStreamError::AudioFilter("the audio filter is not found".into()));
        };

        let stream_info = AudioFilterStreamInfo {
            url: room.inner.url(),
            room_id: room.inner.room.maybe_sid().map(|sid| sid.to_string()).unwrap_or_default(),
            room_name: room.inner.room.name(),
            participant_identity: room.inner.room.local_participant().identity().into(),
            participant_id: room.inner.room.local_participant().name(),
            track_id,
        };
        Ok(Self { plugin, options, info: AudioFilterInfo { stream_info, room_handle } })
    }
}

/// A filter session's description of its stream, and the room it is still waiting on.
///
/// A session created before the room connected was handed an empty room id, which is the
/// one thing it is told again after the fact.
pub(crate) struct AudioFilterInfo {
    pub(crate) stream_info: AudioFilterStreamInfo,
    pub(crate) room_handle: FfiHandleId,
}

impl AudioFilterInfo {
    /// Re-reads the room, reporting whether its sid has arrived.
    fn room_id_arrived(&mut self) -> bool {
        let Ok(room) = crate::FFI_SERVER.retrieve_handle::<FfiRoom>(self.room_handle) else {
            return false;
        };
        let Some(sid) = room.inner.room.maybe_sid() else {
            return false;
        };
        self.stream_info.room_id = sid.to_string();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::target_samples;

    const SAMPLE_RATE: u32 = 48000;

    /// 30ms of 48kHz stereo is 1440 samples per channel, 2880 counting both.
    #[test]
    fn a_frame_size_is_a_whole_number_of_sink_frames() {
        assert_eq!(target_samples(SAMPLE_RATE, 2, Some(30)), Some(2880));
        assert_eq!(target_samples(SAMPLE_RATE, 1, Some(10)), Some(480));
        assert_eq!(target_samples(SAMPLE_RATE, 1, None), None);
    }

    /// A frame size that works out to no samples is no frame size at all, or `next` would
    /// hand out empty frames forever without pulling the sink.
    #[test]
    fn a_frame_size_of_nothing_is_not_a_frame_size() {
        assert_eq!(target_samples(SAMPLE_RATE, 1, Some(0)), None);
        assert_eq!(target_samples(SAMPLE_RATE, 0, Some(30)), None);
    }
}
