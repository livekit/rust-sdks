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

use futures_util::StreamExt;
use livekit::webrtc::{
    prelude::*,
    video_frame::{self as rtc, FrameMetadata, VideoRotation},
    video_stream::native::{NativeVideoStream, NativeVideoStreamOptions},
};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use super::{colorcvt, room::Track};
use crate::{proto, FfiHandleId};

/// Capture-side metadata travelling with a frame.
#[uniffi::remote(Record)]
pub struct FrameMetadata {
    pub user_timestamp: Option<u64>,
    pub frame_id: Option<u32>,
    pub user_data: Option<Vec<u8>>,
}

/// Clockwise rotation to apply to a frame before display.
#[uniffi::remote(Enum)]
pub enum VideoRotation {
    VideoRotation0 = 0,
    VideoRotation90 = 90,
    VideoRotation180 = 180,
    VideoRotation270 = 270,
}

/// Pixel layout of a [`VideoBuffer`].
///
/// Mirrors [`proto::VideoBufferType`], not the libwebrtc enum of the same name: these are
/// the formats `colorcvt` can deliver, so `Native` is absent.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum VideoBufferType {
    Rgba,
    Abgr,
    Argb,
    Bgra,
    Rgb24,
    I420,
    I420a,
    I422,
    I444,
    I010,
    Nv12,
}

/// One plane of a [`VideoBuffer`], as a range of the bytes the buffer owns.
///
/// This is the shape difference the migration forces: the protobuf path describes a plane
/// with a raw pointer, because the pixels stay in the handle map and only their address
/// crosses. A uniffi buffer carries the pixels, so a plane is an offset into them.
#[derive(uniffi::Record)]
pub struct VideoBufferComponent {
    pub offset: u64,
    pub stride: u32,
    pub size: u32,
}

/// Pixel data, and the layout describing it.
///
/// A record, so uniffi copies every frame's pixels across the boundary — ~3MB per 1080p
/// I420 frame, where the protobuf path passes a pointer. Nothing is copied on the Rust
/// side (`colorcvt` already allocated the box), so this is the boundary's own memcpy.
///
/// TODO: make this an object holding the bytes, with plane accessors, if a profile of a
/// real video call says the copy matters.
#[derive(uniffi::Record)]
pub struct VideoBuffer {
    pub r#type: VideoBufferType,
    pub width: u32,
    pub height: u32,
    /// Row length in bytes. Packed formats only; planar formats carry a stride per plane.
    pub stride: Option<u32>,
    /// Empty for packed formats.
    pub components: Vec<VideoBufferComponent>,
    pub data: Vec<u8>,
}

impl VideoBuffer {
    /// Adopts a buffer `colorcvt` produced, rebasing the pointers it wrote into `data` as
    /// offsets into it.
    pub(crate) fn from_ffi(data: Box<[u8]>, info: proto::VideoBufferInfo) -> Self {
        let base = data.as_ptr() as u64;
        Self {
            r#type: info.r#type().into(),
            width: info.width,
            height: info.height,
            stride: info.stride,
            components: info
                .components
                .iter()
                .map(|component| VideoBufferComponent {
                    offset: component.data_ptr - base,
                    stride: component.stride,
                    size: component.size,
                })
                .collect(),
            data: data.into_vec(),
        }
    }
}

/// A decoded frame, and the buffer holding its pixels.
#[derive(uniffi::Record)]
pub struct VideoFrame {
    /// When the frame was captured, in microseconds.
    pub timestamp_us: i64,
    pub rotation: VideoRotation,
    pub metadata: Option<FrameMetadata>,
    pub buffer: VideoBuffer,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum VideoStreamError {
    #[error("{0}")]
    InvalidTrack(String),
}

/// FFI wrapper around [`NativeVideoStream`].
#[derive(uniffi::Object)]
pub struct VideoStream {
    /// Absent on a participant-sourced stream, whose track — and so whose underlying
    /// stream — is replaced as the participant republishes. Those still run on the
    /// protobuf path; see [`VideoStream::from_participant`].
    inner: Mutex<Option<NativeVideoStream>>,
    dst_type: Option<VideoBufferType>,
    normalize_stride: bool,
    /// Reached by [`crate::migration::video_stream`], which is where the rest of the
    /// protobuf path lives. Goes when that file does.
    pub(crate) handle_id: OnceLock<FfiHandleId>,
}

#[uniffi::export(async_runtime = "tokio")]
impl VideoStream {
    /// Opens a stream over a video track.
    #[uniffi::constructor]
    pub fn from_track(
        track: Arc<Track>,
        format: Option<VideoBufferType>,
        normalize_stride: bool,
        queue_size_frames: Option<u32>,
    ) -> Result<Arc<Self>, VideoStreamError> {
        let MediaStreamTrack::Video(rtc_track) = track.inner.rtc_track() else {
            return Err(VideoStreamError::InvalidTrack("not a video track".into()));
        };

        let options = NativeVideoStreamOptions {
            queue_size_frames: queue_size_frames.map(|capacity| capacity as usize),
        };
        Ok(Arc::new(Self::over(
            Some(NativeVideoStream::with_options(rtc_track, options)),
            format.map(Into::into),
            normalize_stride,
        )))
    }

    /// The next frame, or `None` once the track has ended.
    ///
    /// Frames that fail colour conversion are skipped rather than ending the stream.
    /// Participant-sourced streams always yield `None`: they have not migrated.
    ///
    /// One puller at a time, by the mutex. A stream the protobuf path is pumping will
    /// interleave its frames with a foreign caller's: the handle round-trip is for passing
    /// the object across the seam, not for switching how it is consumed mid-flight.
    pub async fn next(&self) -> Option<VideoFrame> {
        let mut inner = self.inner.lock().await;
        let stream = inner.as_mut()?;
        loop {
            let frame = stream.next().await?;
            if let Some(frame) = self.convert(frame) {
                return Some(frame);
            }
        }
    }
}

impl VideoStream {
    pub(crate) fn over(
        stream: Option<NativeVideoStream>,
        dst_type: Option<VideoBufferType>,
        normalize_stride: bool,
    ) -> Self {
        Self { inner: Mutex::new(stream), dst_type, normalize_stride, handle_id: OnceLock::new() }
    }

    fn convert(&self, frame: rtc::VideoFrame<rtc::BoxVideoBuffer>) -> Option<VideoFrame> {
        let dst_type = self.dst_type.map(Into::into);
        match colorcvt::to_video_buffer_info(frame.buffer, dst_type, self.normalize_stride) {
            Ok((data, info)) => Some(VideoFrame {
                timestamp_us: frame.timestamp_us,
                rotation: frame.rotation,
                metadata: frame.frame_metadata,
                buffer: VideoBuffer::from_ffi(data, info),
            }),
            Err(_) => {
                log::error!("video stream failed to convert video frame to {:?}", self.dst_type);
                None
            }
        }
    }
}
