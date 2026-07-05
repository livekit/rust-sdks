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

use std::{error::Error, fmt};

use livekit::webrtc::video_frame::{native::NativeBuffer, I420Buffer, VideoFrame};
use thiserror::Error;

use crate::{
    device::{
        CaptureBackend, CaptureDeviceInfo, CaptureDeviceQueryError, CaptureDeviceSelector,
        CaptureFormat, CaptureFormatRequest, CaptureFrameFormat, CapturePath,
    },
    dmabuf::DmaBufFrame,
    encoded::{ingress::EncodedAccessUnitSource, EncodedRateControl, OwnedEncodedAccessUnit},
    error::CaptureError,
    track::VideoCaptureTrack,
};

/// Options used by [`VideoCaptureSource::open`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureSourceOptions {
    /// Backend to open.
    pub backend: CaptureBackend,
    /// Device to open.
    pub device: CaptureDeviceSelector,
    /// Format requested from the backend.
    pub format: CaptureFormatRequest,
    /// Whether the resulting track should be marked as a screencast.
    pub is_screencast: bool,
    /// Prefer CPU-accessible frames over zero-copy native buffers, for
    /// callers that modify pixels before publishing.
    pub prefer_raw_frames: bool,
    /// Endpoint for the encoded ingest backends (RTSP/TCP/GStreamer).
    pub encoded: Option<EncodedEndpoint>,
}

impl Default for CaptureSourceOptions {
    fn default() -> Self {
        Self {
            backend: CaptureBackend::Auto,
            device: CaptureDeviceSelector::Default,
            format: CaptureFormatRequest::Default,
            is_screencast: false,
            prefer_raw_frames: false,
            encoded: None,
        }
    }
}

/// Endpoint configuration for the encoded ingest backends.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodedEndpoint {
    /// RTSP camera URL ingested over TCP-interleaved RTP.
    #[cfg(feature = "rtsp")]
    Rtsp {
        /// RTSP URL, e.g. `rtsp://user:pass@camera/stream`.
        url: String,
        /// RTSP source options (codec expectation, dimensions, timeouts).
        options: crate::sources::rtsp::RtspSourceOptions,
    },
    /// TCP byte-stream endpoint to connect to.
    #[cfg(feature = "tcpsink")]
    TcpConnect {
        /// `host:port` to connect to.
        address: String,
        /// Byte-stream configuration (wire format, dimensions, timing).
        config: crate::sources::tcp::ByteStreamSourceConfig,
    },
    /// GStreamer launch description that contains or feeds an encoded appsink.
    ///
    /// The pipeline must either contain `appsink name=lk_appsink` or leave
    /// one encoded video source pad unlinked (a parser, capsfilter, and
    /// appsink are attached automatically).
    #[cfg(feature = "gstreamer")]
    GstreamerLaunch {
        /// `gst-launch`-style pipeline description.
        launch: String,
        /// Expected codec; inferred from the pipeline caps when `None`.
        codec: Option<crate::encoded::EncodedVideoCodec>,
        /// Appsink source configuration (dimensions and timestamp fallbacks).
        ///
        /// The `sample_format` field is overridden by what the pipeline caps
        /// advertise.
        config: crate::sources::gstreamer::GStreamerAppSinkConfig,
    },
}

/// Uncompressed CPU-accessible video frame buffer produced by a capture source.
#[derive(Debug)]
pub struct RawVideoFrame {
    /// I420 video frame suitable for [`VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<I420Buffer>,
    /// Source format delivered by the capture backend before conversion to I420.
    pub source_format: CaptureFrameFormat,
    /// Wall-clock capture timestamp in microseconds.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read, in microseconds.
    pub read_wall_time_us: u64,
    /// Sensor timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
    /// Whether the backend converted the source buffer before publishing.
    pub used_conversion: bool,
}

impl RawVideoFrame {
    /// Returns the decoded I420 video frame.
    pub fn video_frame(&self) -> &VideoFrame<I420Buffer> {
        &self.frame
    }
}

/// Platform-native uncompressed video frame buffer produced by a capture source.
#[derive(Debug)]
pub struct NativeVideoFrame {
    /// Native video frame suitable for [`VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<NativeBuffer>,
    /// Source format delivered by the capture backend.
    pub source_format: CaptureFrameFormat,
    /// Wall-clock capture timestamp in microseconds.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read, in microseconds.
    pub read_wall_time_us: u64,
    /// Sensor timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
}

impl NativeVideoFrame {
    /// Returns the native video frame.
    pub fn video_frame(&self) -> &VideoFrame<NativeBuffer> {
        &self.frame
    }
}

/// Frame produced by a capture source.
#[derive(Debug)]
#[non_exhaustive]
pub enum CaptureFrame {
    /// Platform-native uncompressed frame.
    Native(NativeVideoFrame),
    /// Uncompressed CPU-accessible frame.
    Raw(RawVideoFrame),
    /// Linux DMA-BUF backed frame.
    DmaBuf(DmaBufFrame),
    /// Encoded video access unit.
    Encoded(OwnedEncodedAccessUnit),
}

impl CaptureFrame {
    /// Returns the capture path used by this frame.
    pub fn capture_path(&self) -> CapturePath {
        match self {
            Self::Native(_) => CapturePath::Native,
            Self::Raw(_) => CapturePath::Raw,
            Self::DmaBuf(_) => CapturePath::DmaBuf,
            Self::Encoded(_) => CapturePath::Encoded,
        }
    }

    /// Publishes this frame into a LiveKit capture track.
    pub fn publish_to(&self, track: &VideoCaptureTrack) -> Result<(), CaptureError> {
        match self {
            Self::Native(frame) => {
                track.capture_frame(&frame.frame);
                Ok(())
            }
            Self::Raw(frame) => {
                track.capture_frame(&frame.frame);
                Ok(())
            }
            #[cfg(target_os = "linux")]
            Self::DmaBuf(frame) => track.capture_dmabuf(frame),
            #[cfg(not(target_os = "linux"))]
            Self::DmaBuf(_) => Err(CaptureError::UnsupportedPlatform("DMA-BUF capture")),
            Self::Encoded(access_unit) => track.capture_encoded(&access_unit.as_access_unit()),
        }
    }
}

/// Source that produces one of the common capture frame paths.
pub trait CaptureFrameSource {
    /// Error returned by the source.
    type Error: Error + Send + Sync + 'static;

    /// Returns the capture path produced by this source.
    fn capture_path(&self) -> CapturePath;

    /// Returns the negotiated capture format when the source has one.
    fn format(&self) -> Option<CaptureFormat>;

    /// Captures the next frame.
    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error>;
}

/// Adapts an [`EncodedAccessUnitSource`] into the common frame-source model.
#[derive(Debug)]
pub struct EncodedCaptureFrameSource<S> {
    source: S,
}

impl<S> EncodedCaptureFrameSource<S> {
    /// Creates a frame-source adapter for an encoded access-unit source.
    pub fn new(source: S) -> Self {
        Self { source }
    }

    /// Returns the underlying encoded source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Returns the underlying encoded source mutably.
    pub fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    /// Consumes this adapter and returns the underlying encoded source.
    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S> CaptureFrameSource for EncodedCaptureFrameSource<S>
where
    S: EncodedAccessUnitSource,
{
    type Error = EncodedFrameSourceError<S::Error>;

    fn capture_path(&self) -> CapturePath {
        CapturePath::Encoded
    }

    fn format(&self) -> Option<CaptureFormat> {
        None
    }

    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error> {
        let Some(access_unit) =
            self.source.next_access_unit().map_err(EncodedFrameSourceError::Source)?
        else {
            return Err(EncodedFrameSourceError::EndOfStream);
        };
        Ok(CaptureFrame::Encoded(access_unit))
    }
}

/// Error returned by [`EncodedCaptureFrameSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodedFrameSourceError<E> {
    /// The encoded source reached EOF.
    EndOfStream,
    /// The encoded source failed.
    Source(E),
}

impl<E: fmt::Display> fmt::Display for EncodedFrameSourceError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfStream => f.write_str("encoded source reached end of stream"),
            Self::Source(err) => write!(f, "encoded source failed: {err}"),
        }
    }
}

impl<E> Error for EncodedFrameSourceError<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EndOfStream => None,
            Self::Source(err) => Some(err),
        }
    }
}

/// Error returned by the high-level capture source façade.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CaptureSourceError {
    /// The requested backend cannot be used by this façade on this target or build.
    #[error("capture backend {0} is not supported by VideoCaptureSource on this target or build")]
    UnsupportedBackend(CaptureBackend),
    /// The backend requires an [`EncodedEndpoint`] in [`CaptureSourceOptions::encoded`].
    #[error("capture backend {0} requires a matching CaptureSourceOptions::encoded endpoint")]
    MissingEncodedEndpoint(CaptureBackend),
    /// The encoded source reached end of stream.
    #[error("capture source reached end of stream")]
    EndOfStream,
    /// The backend source failed.
    #[error("capture backend {backend} failed: {message}")]
    Backend {
        /// Backend that failed.
        backend: CaptureBackend,
        /// Backend error message.
        message: String,
    },
    /// The capture track rejected the frame.
    #[error(transparent)]
    Capture(#[from] CaptureError),
}

/// GStreamer pipeline plus the encoded appsink source reading from it.
///
/// Stops the pipeline when dropped.
#[cfg(feature = "gstreamer")]
#[derive(Debug)]
pub struct GStreamerCaptureSource {
    pipeline: ::gstreamer::Pipeline,
    source: EncodedCaptureFrameSource<crate::sources::gstreamer::GStreamerAppSinkEncodedSource>,
}

#[cfg(feature = "gstreamer")]
impl GStreamerCaptureSource {
    /// Returns the running pipeline.
    pub fn pipeline(&self) -> &::gstreamer::Pipeline {
        &self.pipeline
    }

    /// Returns the encoded appsink source.
    pub fn source_mut(
        &mut self,
    ) -> &mut EncodedCaptureFrameSource<crate::sources::gstreamer::GStreamerAppSinkEncodedSource>
    {
        &mut self.source
    }
}

#[cfg(feature = "gstreamer")]
impl Drop for GStreamerCaptureSource {
    fn drop(&mut self) {
        use ::gstreamer::prelude::ElementExt;
        let _ = self.pipeline.set_state(::gstreamer::State::Null);
    }
}

/// High-level capture source façade for the crate's capture backends.
#[derive(Debug)]
#[non_exhaustive]
pub enum VideoCaptureSource {
    /// AVFoundation decoded-frame source.
    #[cfg(feature = "avfoundation")]
    AvFoundation {
        /// Underlying capture session.
        session: crate::sources::avfoundation::AvFoundationCaptureSession,
        /// Prefer CPU-accessible frames over zero-copy native buffers.
        prefer_raw_frames: bool,
    },
    /// Linux V4L2 decoded-frame source.
    #[cfg(feature = "v4l")]
    V4l(crate::sources::v4l::V4lCaptureSession),
    /// Jetson libargus DMA-BUF source.
    #[cfg(feature = "libargus")]
    LibArgus(crate::sources::argus::ArgusCaptureSession),
    /// RTSP encoded ingest source.
    #[cfg(feature = "rtsp")]
    Rtsp(EncodedCaptureFrameSource<crate::sources::rtsp::RtspEncodedSource>),
    /// TCP byte-stream encoded ingest source.
    #[cfg(feature = "tcpsink")]
    Tcp(EncodedCaptureFrameSource<crate::sources::tcp::TcpEncodedSource>),
    /// GStreamer pipeline encoded ingest source.
    #[cfg(feature = "gstreamer")]
    Gstreamer(GStreamerCaptureSource),
}

impl VideoCaptureSource {
    /// Lists capture devices for a backend.
    ///
    /// The encoded ingest backends (RTSP/TCP/GStreamer) address network
    /// endpoints rather than enumerable devices, so they report
    /// [`CaptureDeviceQueryError::UnsupportedBackend`].
    pub fn list_devices(
        backend: CaptureBackend,
    ) -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
        match backend {
            CaptureBackend::Auto => list_auto_devices(),
            CaptureBackend::AvFoundation => list_avfoundation_devices(),
            CaptureBackend::V4l2 => list_v4l_devices(),
            CaptureBackend::LibArgus => list_argus_devices(),
            CaptureBackend::Rtsp | CaptureBackend::Tcp | CaptureBackend::Gstreamer => {
                Err(CaptureDeviceQueryError::UnsupportedBackend(backend))
            }
        }
    }

    /// Opens a capture source.
    pub fn open(options: CaptureSourceOptions) -> Result<Self, CaptureSourceError> {
        match options.backend {
            CaptureBackend::Auto => open_auto_source(options),
            CaptureBackend::AvFoundation => open_avfoundation_source(options),
            CaptureBackend::V4l2 => open_v4l_source(options),
            CaptureBackend::LibArgus => open_argus_source(options),
            CaptureBackend::Rtsp => open_rtsp_source(options),
            CaptureBackend::Tcp => open_tcp_source(options),
            CaptureBackend::Gstreamer => open_gstreamer_source(options),
        }
    }

    /// Returns the capture path produced by this source.
    pub fn capture_path(&self) -> CapturePath {
        match self {
            #[cfg(feature = "avfoundation")]
            Self::AvFoundation { session, prefer_raw_frames } => {
                if session.native_capture_supported() && !prefer_raw_frames {
                    CapturePath::Native
                } else {
                    CapturePath::Raw
                }
            }
            #[cfg(feature = "v4l")]
            Self::V4l(source) => source.capture_path(),
            #[cfg(feature = "libargus")]
            Self::LibArgus(source) => source.capture_path(),
            #[cfg(feature = "rtsp")]
            Self::Rtsp(_) => CapturePath::Encoded,
            #[cfg(feature = "tcpsink")]
            Self::Tcp(_) => CapturePath::Encoded,
            #[cfg(feature = "gstreamer")]
            Self::Gstreamer(_) => CapturePath::Encoded,
            #[allow(unreachable_patterns)]
            _ => unreachable!("VideoCaptureSource has no enabled backend variants"),
        }
    }

    /// Returns the negotiated capture format when the source has one.
    pub fn format(&self) -> Option<CaptureFormat> {
        match self {
            #[cfg(feature = "avfoundation")]
            Self::AvFoundation { session, .. } => Some(session.format()),
            #[cfg(feature = "v4l")]
            Self::V4l(source) => Some(source.format()),
            #[cfg(feature = "libargus")]
            Self::LibArgus(source) => Some(source.format()),
            #[cfg(feature = "rtsp")]
            Self::Rtsp(_) => None,
            #[cfg(feature = "tcpsink")]
            Self::Tcp(_) => None,
            #[cfg(feature = "gstreamer")]
            Self::Gstreamer(_) => None,
            #[allow(unreachable_patterns)]
            _ => unreachable!("VideoCaptureSource has no enabled backend variants"),
        }
    }

    /// Captures the next frame.
    ///
    /// The encoded ingest backends return
    /// [`CaptureSourceError::EndOfStream`] when the stream terminates
    /// normally.
    pub fn next_frame(&mut self) -> Result<CaptureFrame, CaptureSourceError> {
        match self {
            #[cfg(feature = "avfoundation")]
            Self::AvFoundation { session, prefer_raw_frames } => {
                let frame = if session.native_capture_supported() && !*prefer_raw_frames {
                    session.capture_native_frame().map(|frame| CaptureFrame::Native(frame.into()))
                } else {
                    session.capture_frame().map(|frame| CaptureFrame::Raw(frame.into()))
                };
                frame.map_err(|err| backend_source_error(CaptureBackend::AvFoundation, err))
            }
            #[cfg(feature = "v4l")]
            Self::V4l(source) => {
                source.next_frame().map_err(|err| backend_source_error(CaptureBackend::V4l2, err))
            }
            #[cfg(feature = "libargus")]
            Self::LibArgus(source) => source
                .next_frame()
                .map_err(|err| backend_source_error(CaptureBackend::LibArgus, err)),
            #[cfg(feature = "rtsp")]
            Self::Rtsp(source) => {
                source.next_frame().map_err(|err| encoded_source_error(CaptureBackend::Rtsp, err))
            }
            #[cfg(feature = "tcpsink")]
            Self::Tcp(source) => {
                source.next_frame().map_err(|err| encoded_source_error(CaptureBackend::Tcp, err))
            }
            #[cfg(feature = "gstreamer")]
            Self::Gstreamer(source) => source
                .source
                .next_frame()
                .map_err(|err| encoded_source_error(CaptureBackend::Gstreamer, err)),
            #[allow(unreachable_patterns)]
            _ => unreachable!("VideoCaptureSource has no enabled backend variants"),
        }
    }

    /// Signals the source to stop, interrupting a blocked
    /// [`VideoCaptureSource::next_frame`] where the backend supports it
    /// (AVFoundation today); other backends return at the next frame
    /// boundary.
    pub fn stop(&self) {
        match self {
            #[cfg(feature = "avfoundation")]
            Self::AvFoundation { session, .. } => session.stop(),
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// Forwards a downstream keyframe request to the source's producer.
    ///
    /// No-op for the decoded camera backends, which have no upstream
    /// encoder.
    pub fn request_keyframe(&mut self) {
        match self {
            #[cfg(feature = "rtsp")]
            Self::Rtsp(source) => source.source_mut().request_keyframe(),
            #[cfg(feature = "tcpsink")]
            Self::Tcp(source) => source.source_mut().request_keyframe(),
            #[cfg(feature = "gstreamer")]
            Self::Gstreamer(source) => source.source.source_mut().request_keyframe(),
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// Forwards a downstream rate-control target to the source's producer.
    ///
    /// No-op for decoded camera backends and encoded transports that cannot
    /// influence an upstream encoder.
    pub fn update_rate_control(&mut self, rate_control: EncodedRateControl) {
        match self {
            #[cfg(feature = "rtsp")]
            Self::Rtsp(source) => source.source_mut().update_rate_control(rate_control),
            #[cfg(feature = "tcpsink")]
            Self::Tcp(source) => source.source_mut().update_rate_control(rate_control),
            #[cfg(feature = "gstreamer")]
            Self::Gstreamer(source) => source.source.source_mut().update_rate_control(rate_control),
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    /// Captures and publishes the next frame, returning `false` once an
    /// encoded source reaches end of stream.
    ///
    /// Rate-control and keyframe requests raised by the passthrough encoder
    /// are polled from the track and forwarded to the source before each
    /// capture.
    pub fn publish_next(&mut self, track: &VideoCaptureTrack) -> Result<bool, CaptureSourceError> {
        if let Some(rate_control) = track.take_rate_control_request() {
            self.update_rate_control(rate_control);
        }
        if track.take_keyframe_request() {
            self.request_keyframe();
        }
        let frame = match self.next_frame() {
            Ok(frame) => frame,
            Err(CaptureSourceError::EndOfStream) => return Ok(false),
            Err(err) => return Err(err),
        };
        frame.publish_to(track)?;
        Ok(true)
    }
}

#[cfg(feature = "avfoundation")]
impl CaptureFrameSource for crate::sources::avfoundation::AvFoundationCaptureSession {
    type Error = crate::sources::avfoundation::AvFoundationError;

    fn capture_path(&self) -> CapturePath {
        self.capture_path()
    }

    fn format(&self) -> Option<CaptureFormat> {
        Some(self.format())
    }

    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error> {
        if self.native_capture_supported() {
            self.capture_native_frame().map(|frame| CaptureFrame::Native(frame.into()))
        } else {
            self.capture_frame().map(|frame| CaptureFrame::Raw(frame.into()))
        }
    }
}

#[cfg(feature = "avfoundation")]
impl From<crate::sources::avfoundation::AvFoundationNativeFrame> for NativeVideoFrame {
    fn from(frame: crate::sources::avfoundation::AvFoundationNativeFrame) -> Self {
        Self {
            frame: frame.frame,
            source_format: frame.source_format,
            capture_wall_time_us: frame.capture_wall_time_us,
            read_wall_time_us: frame.read_wall_time_us,
            sensor_timestamp_us: frame.sensor_timestamp_us,
        }
    }
}

#[cfg(feature = "avfoundation")]
impl From<crate::sources::avfoundation::AvFoundationFrame> for RawVideoFrame {
    fn from(frame: crate::sources::avfoundation::AvFoundationFrame) -> Self {
        Self {
            frame: frame.frame,
            source_format: frame.source_format,
            capture_wall_time_us: frame.capture_wall_time_us,
            read_wall_time_us: frame.read_wall_time_us,
            sensor_timestamp_us: frame.sensor_timestamp_us,
            used_conversion: frame.used_conversion,
        }
    }
}

#[cfg(feature = "v4l")]
impl CaptureFrameSource for crate::sources::v4l::V4lCaptureSession {
    type Error = crate::sources::v4l::V4lError;

    fn capture_path(&self) -> CapturePath {
        self.capture_path()
    }

    fn format(&self) -> Option<CaptureFormat> {
        Some(self.format())
    }

    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error> {
        self.capture_frame().map(|frame| CaptureFrame::Raw(frame.into()))
    }
}

#[cfg(feature = "v4l")]
impl From<crate::sources::v4l::V4lFrame> for RawVideoFrame {
    fn from(frame: crate::sources::v4l::V4lFrame) -> Self {
        Self {
            used_conversion: frame.used_conversion,
            frame: frame.frame,
            source_format: frame.source_format,
            capture_wall_time_us: frame.capture_wall_time_us,
            read_wall_time_us: frame.read_wall_time_us,
            sensor_timestamp_us: frame.sensor_timestamp_us,
        }
    }
}

#[cfg(feature = "libargus")]
impl CaptureFrameSource for crate::sources::argus::ArgusCaptureSession {
    type Error = crate::sources::argus::ArgusError;

    fn capture_path(&self) -> CapturePath {
        self.capture_path()
    }

    fn format(&self) -> Option<CaptureFormat> {
        Some(self.format())
    }

    fn next_frame(&mut self) -> Result<CaptureFrame, Self::Error> {
        self.capture_frame().map(|frame| CaptureFrame::DmaBuf(frame.dmabuf))
    }
}

#[allow(dead_code)]
fn backend_source_error(
    backend: CaptureBackend,
    error: impl Error + Send + Sync + 'static,
) -> CaptureSourceError {
    CaptureSourceError::Backend { backend, message: error.to_string() }
}

#[allow(dead_code)]
fn backend_query_error(
    backend: CaptureBackend,
    error: impl Error + Send + Sync + 'static,
) -> CaptureDeviceQueryError {
    CaptureDeviceQueryError::Backend { backend, message: error.to_string() }
}

#[allow(dead_code)]
fn encoded_source_error<E: Error + Send + Sync + 'static>(
    backend: CaptureBackend,
    error: EncodedFrameSourceError<E>,
) -> CaptureSourceError {
    match error {
        EncodedFrameSourceError::EndOfStream => CaptureSourceError::EndOfStream,
        EncodedFrameSourceError::Source(err) => backend_source_error(backend, err),
    }
}

#[cfg(feature = "rtsp")]
fn open_rtsp_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let Some(EncodedEndpoint::Rtsp { url, options: rtsp_options }) = options.encoded else {
        return Err(CaptureSourceError::MissingEncodedEndpoint(CaptureBackend::Rtsp));
    };
    let source = crate::sources::rtsp::RtspEncodedSource::connect(&url, rtsp_options)
        .map_err(|err| backend_source_error(CaptureBackend::Rtsp, err))?;
    Ok(VideoCaptureSource::Rtsp(EncodedCaptureFrameSource::new(source)))
}

#[cfg(not(feature = "rtsp"))]
fn open_rtsp_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::Rtsp))
}

#[cfg(feature = "tcpsink")]
fn open_tcp_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let Some(EncodedEndpoint::TcpConnect { address, config }) = options.encoded else {
        return Err(CaptureSourceError::MissingEncodedEndpoint(CaptureBackend::Tcp));
    };
    let source = crate::sources::tcp::TcpEncodedSource::connect(address.as_str(), config)
        .map_err(|err| backend_source_error(CaptureBackend::Tcp, err))?;
    Ok(VideoCaptureSource::Tcp(EncodedCaptureFrameSource::new(source)))
}

#[cfg(not(feature = "tcpsink"))]
fn open_tcp_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::Tcp))
}

#[cfg(feature = "gstreamer")]
fn open_gstreamer_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    use ::gstreamer::prelude::*;

    let Some(EncodedEndpoint::GstreamerLaunch { launch, codec, mut config }) = options.encoded
    else {
        return Err(CaptureSourceError::MissingEncodedEndpoint(CaptureBackend::Gstreamer));
    };

    let gst_error = |err: &dyn std::fmt::Display| CaptureSourceError::Backend {
        backend: CaptureBackend::Gstreamer,
        message: err.to_string(),
    };

    ::gstreamer::init().map_err(|err| gst_error(&err))?;
    let pipeline = ::gstreamer::parse::launch(&launch)
        .map_err(|err| gst_error(&err))?
        .downcast::<::gstreamer::Pipeline>()
        .map_err(|element| CaptureSourceError::Backend {
            backend: CaptureBackend::Gstreamer,
            message: format!(
                "launch description did not produce a pipeline (got {})",
                element.name()
            ),
        })?;
    let (appsink, sample_format) =
        crate::sources::gstreamer::ensure_encoded_appsink(&pipeline, codec)
            .map_err(|err| gst_error(&err))?;
    config.sample_format = sample_format;
    pipeline.set_state(::gstreamer::State::Playing).map_err(|err| gst_error(&err))?;

    let source = crate::sources::gstreamer::GStreamerAppSinkEncodedSource::new(appsink, config);
    Ok(VideoCaptureSource::Gstreamer(GStreamerCaptureSource {
        pipeline,
        source: EncodedCaptureFrameSource::new(source),
    }))
}

#[cfg(not(feature = "gstreamer"))]
fn open_gstreamer_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::Gstreamer))
}

fn list_auto_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    #[cfg(all(target_os = "macos", feature = "avfoundation"))]
    {
        return list_avfoundation_devices();
    }
    #[cfg(all(target_os = "linux", feature = "v4l"))]
    {
        return list_v4l_devices();
    }
    #[allow(unreachable_code)]
    Err(CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::Auto))
}

fn open_auto_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let _ = &options;
    #[cfg(all(target_os = "macos", feature = "avfoundation"))]
    {
        let mut options = options;
        options.backend = CaptureBackend::AvFoundation;
        return open_avfoundation_source(options);
    }
    #[cfg(all(target_os = "linux", feature = "v4l"))]
    {
        let mut options = options;
        options.backend = CaptureBackend::V4l2;
        return open_v4l_source(options);
    }
    #[allow(unreachable_code)]
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::Auto))
}

#[cfg(feature = "avfoundation")]
fn list_avfoundation_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    crate::sources::avfoundation::devices().map_err(|err| match err {
        crate::sources::avfoundation::AvFoundationError::UnsupportedPlatform => {
            CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::AvFoundation)
        }
        other => backend_query_error(CaptureBackend::AvFoundation, other),
    })
}

#[cfg(not(feature = "avfoundation"))]
fn list_avfoundation_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    Err(CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::AvFoundation))
}

#[cfg(feature = "avfoundation")]
fn open_avfoundation_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let prefer_raw_frames = options.prefer_raw_frames;
    let source = crate::sources::avfoundation::AvFoundationCaptureSession::new(options.into())
        .map_err(|err| match err {
            crate::sources::avfoundation::AvFoundationError::UnsupportedPlatform => {
                CaptureSourceError::UnsupportedBackend(CaptureBackend::AvFoundation)
            }
            other => backend_source_error(CaptureBackend::AvFoundation, other),
        })?;
    Ok(VideoCaptureSource::AvFoundation { session: source, prefer_raw_frames })
}

#[cfg(not(feature = "avfoundation"))]
fn open_avfoundation_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::AvFoundation))
}

#[cfg(feature = "avfoundation")]
impl From<CaptureSourceOptions> for crate::sources::avfoundation::AvFoundationCaptureOptions {
    fn from(options: CaptureSourceOptions) -> Self {
        Self {
            device: options.device,
            format: options.format,
            is_screencast: options.is_screencast,
        }
    }
}

#[cfg(feature = "v4l")]
fn list_v4l_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    crate::sources::v4l::devices().map_err(|err| match err {
        crate::sources::v4l::V4lError::UnsupportedPlatform => {
            CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::V4l2)
        }
        other => backend_query_error(CaptureBackend::V4l2, other),
    })
}

#[cfg(not(feature = "v4l"))]
fn list_v4l_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    Err(CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::V4l2))
}

#[cfg(feature = "v4l")]
fn open_v4l_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let source =
        crate::sources::v4l::V4lCaptureSession::new(options.into()).map_err(|err| match err {
            crate::sources::v4l::V4lError::UnsupportedPlatform => {
                CaptureSourceError::UnsupportedBackend(CaptureBackend::V4l2)
            }
            other => backend_source_error(CaptureBackend::V4l2, other),
        })?;
    Ok(VideoCaptureSource::V4l(source))
}

#[cfg(not(feature = "v4l"))]
fn open_v4l_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::V4l2))
}

#[cfg(feature = "v4l")]
impl From<CaptureSourceOptions> for crate::sources::v4l::V4lCaptureOptions {
    fn from(options: CaptureSourceOptions) -> Self {
        let mut source_options = Self {
            device: options.device,
            format: options.format,
            frame_formats: crate::sources::v4l::default_frame_formats(),
        };
        if let CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) =
            source_options.format
        {
            source_options.frame_formats =
                crate::sources::v4l::ordered_frame_formats_with_first(format.frame_format);
        }
        source_options
    }
}

#[cfg(feature = "libargus")]
fn list_argus_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    crate::sources::argus::devices().map_err(|err| match err {
        crate::sources::argus::ArgusError::Unsupported => {
            CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::LibArgus)
        }
        other => backend_query_error(CaptureBackend::LibArgus, other),
    })
}

#[cfg(not(feature = "libargus"))]
fn list_argus_devices() -> Result<Vec<CaptureDeviceInfo>, CaptureDeviceQueryError> {
    Err(CaptureDeviceQueryError::UnsupportedBackend(CaptureBackend::LibArgus))
}

#[cfg(feature = "libargus")]
fn open_argus_source(
    options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    let source =
        crate::sources::argus::ArgusCaptureSession::new(options.try_into()?).map_err(|err| {
            match err {
                crate::sources::argus::ArgusError::Unsupported => {
                    CaptureSourceError::UnsupportedBackend(CaptureBackend::LibArgus)
                }
                other => backend_source_error(CaptureBackend::LibArgus, other),
            }
        })?;
    Ok(VideoCaptureSource::LibArgus(source))
}

#[cfg(not(feature = "libargus"))]
fn open_argus_source(
    _options: CaptureSourceOptions,
) -> Result<VideoCaptureSource, CaptureSourceError> {
    Err(CaptureSourceError::UnsupportedBackend(CaptureBackend::LibArgus))
}

#[cfg(feature = "libargus")]
impl TryFrom<CaptureSourceOptions> for crate::sources::argus::ArgusCaptureOptions {
    type Error = CaptureSourceError;

    fn try_from(options: CaptureSourceOptions) -> Result<Self, Self::Error> {
        let sensor_index = match options.device {
            CaptureDeviceSelector::Default => 0,
            CaptureDeviceSelector::Index(index) => {
                u32::try_from(index).map_err(|_| CaptureSourceError::Backend {
                    backend: CaptureBackend::LibArgus,
                    message: "device index is out of range".to_string(),
                })?
            }
            CaptureDeviceSelector::Id(_) => {
                return Err(CaptureSourceError::Backend {
                    backend: CaptureBackend::LibArgus,
                    message: "libargus does not support string device selectors".to_string(),
                });
            }
        };
        let format = match options.format {
            CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => format,
            CaptureFormatRequest::Default => {
                crate::sources::argus::ArgusCaptureOptions::default().format
            }
            CaptureFormatRequest::HighestFrameRate { .. }
            | CaptureFormatRequest::HighestResolution { .. } => {
                return Err(CaptureSourceError::Backend {
                    backend: CaptureBackend::LibArgus,
                    message: "libargus requires an exact or closest format".to_string(),
                });
            }
        };
        Ok(Self { sensor_index, format })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmabuf::{DmaBufPixelFormat, DmaBufPlane};
    use crate::encoded::{EncodedFrameType, EncodedVideoCodec};
    use livekit::webrtc::video_frame::VideoRotation;

    #[derive(Debug, Error)]
    #[error("fake source failed")]
    struct FakeSourceError;

    #[derive(Debug)]
    struct FakeEncodedSource {
        next: Option<OwnedEncodedAccessUnit>,
    }

    impl EncodedAccessUnitSource for FakeEncodedSource {
        type Error = FakeSourceError;

        fn next_access_unit(&mut self) -> Result<Option<OwnedEncodedAccessUnit>, Self::Error> {
            Ok(self.next.take())
        }
    }

    #[test]
    fn encoded_source_adapts_to_capture_frame_source() {
        let access_unit = OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::H264,
            vec![0, 0, 0, 1, 0x65],
            10,
            EncodedFrameType::Key,
            640,
            480,
        );
        let mut source =
            EncodedCaptureFrameSource::new(FakeEncodedSource { next: Some(access_unit.clone()) });

        assert_eq!(source.capture_path(), CapturePath::Encoded);
        let frame = source.next_frame().expect("encoded frame should be returned");
        assert_eq!(frame.capture_path(), CapturePath::Encoded);
        let CaptureFrame::Encoded(returned) = frame else {
            panic!("expected encoded frame");
        };
        assert_eq!(returned, access_unit);
    }

    #[test]
    fn encoded_source_reports_end_of_stream() {
        let mut source = EncodedCaptureFrameSource::new(FakeEncodedSource { next: None });
        let err = source.next_frame().expect_err("EOF should be reported");
        assert!(matches!(err, EncodedFrameSourceError::EndOfStream));
    }

    #[test]
    fn capture_frame_reports_common_paths() {
        let raw = CaptureFrame::Raw(RawVideoFrame {
            frame: VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                frame_metadata: None,
                buffer: I420Buffer::new(2, 2),
            },
            source_format: CaptureFrameFormat::I420,
            capture_wall_time_us: 1,
            read_wall_time_us: 2,
            sensor_timestamp_us: None,
            used_conversion: false,
        });
        assert_eq!(raw.capture_path(), CapturePath::Raw);

        let dmabuf = CaptureFrame::DmaBuf(DmaBufFrame {
            width: 2,
            height: 2,
            pixel_format: DmaBufPixelFormat::Nv12,
            planes: vec![DmaBufPlane { fd: -1, offset: 0, stride: 2 }],
            modifier: None,
            timestamp_us: 0,
            sensor_timestamp_us: None,
        });
        assert_eq!(dmabuf.capture_path(), CapturePath::DmaBuf);

        let encoded = CaptureFrame::Encoded(OwnedEncodedAccessUnit::new(
            EncodedVideoCodec::H264,
            vec![0, 0, 0, 1, 0x65],
            0,
            EncodedFrameType::Key,
            2,
            2,
        ));
        assert_eq!(encoded.capture_path(), CapturePath::Encoded);
    }

    #[cfg(feature = "avfoundation")]
    #[test]
    fn avfoundation_canonical_import_compiles() {
        let _ = std::any::TypeId::of::<crate::sources::avfoundation::AvFoundationCaptureOptions>();
    }

    #[cfg(feature = "v4l")]
    #[test]
    fn v4l_canonical_import_compiles() {
        let _ = std::any::TypeId::of::<crate::sources::v4l::V4lCaptureOptions>();
    }
}
