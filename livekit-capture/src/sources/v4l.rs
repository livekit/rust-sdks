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

//! Linux V4L2 capture using Nokhwa's V4L backend.

use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use livekit::webrtc::video_frame::VideoRotation;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame};
#[cfg(target_os = "linux")]
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{
        ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
        Resolution,
    },
    Camera,
};
use thiserror::Error;

#[cfg(target_os = "linux")]
use crate::device::CaptureBackend;
use crate::device::{
    CaptureDeviceInfo, CaptureDeviceSelector, CaptureFormat, CaptureFormatRequest,
    CaptureFrameFormat, CapturePath, CaptureResolution,
};

#[cfg(any(target_os = "linux", test))]
const MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US: u64 = 5_000_000;

/// Options used to open a Linux V4L2 capture session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4lCaptureOptions {
    /// Device to open.
    pub device: CaptureDeviceSelector,
    /// Requested format policy.
    pub format: CaptureFormatRequest,
    /// Ordered source frame formats to try.
    pub frame_formats: Vec<CaptureFrameFormat>,
}

impl V4lCaptureOptions {
    /// Creates options that try YUYV, MJPEG, grayscale, RGB24, and NV12 at the requested format.
    pub fn new(
        device: CaptureDeviceSelector,
        resolution: CaptureResolution,
        frame_rate: u32,
    ) -> Self {
        Self {
            device,
            format: CaptureFormatRequest::Exact(CaptureFormat::new(
                resolution,
                frame_rate,
                CaptureFrameFormat::Yuyv,
            )),
            frame_formats: default_frame_formats(),
        }
    }
}

impl Default for V4lCaptureOptions {
    fn default() -> Self {
        Self::new(CaptureDeviceSelector::Default, CaptureResolution::new(1280, 720), 30)
    }
}

/// Error returned by the V4L capture backend.
#[derive(Debug, Error)]
pub enum V4lError {
    /// V4L capture is only available on Linux.
    #[error("V4L capture is not supported on this platform")]
    UnsupportedPlatform,
    /// The requested frame format is not supported by this backend.
    #[error("V4L capture does not support frame format {0:?}")]
    UnsupportedFrameFormat(CaptureFrameFormat),
    /// The requested option is invalid.
    #[error("invalid V4L capture option: {0}")]
    InvalidOption(&'static str),
    /// A numeric option could not be represented by Nokhwa.
    #[error("V4L capture option is out of range: {0}")]
    OptionOutOfRange(&'static str),
    /// The camera backend returned an error.
    #[error("V4L camera error: {0}")]
    Camera(String),
    /// Captured frame bytes did not match the negotiated format.
    #[error("invalid V4L frame buffer: {0}")]
    InvalidFrame(&'static str),
    /// Pixel conversion failed.
    #[error("failed to convert V4L frame to I420: {0}")]
    Convert(&'static str),
    /// MJPEG fallback decoding failed.
    #[error("failed to decode MJPEG frame: {0}")]
    Decode(String),
}

/// One V4L frame converted to I420.
#[derive(Debug)]
pub struct V4lFrame {
    /// Decoded I420 frame suitable for [`crate::VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<I420Buffer>,
    /// Source frame format delivered by the camera backend.
    pub source_format: CaptureFrameFormat,
    /// Backend-provided capture timestamp, when available.
    pub backend_capture_timestamp: Option<Duration>,
    /// Wall-clock timestamp selected for metadata and timing correlation.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read from the camera backend.
    pub read_wall_time_us: u64,
    /// Sensor timestamp translated to UNIX-epoch microseconds, when available.
    pub sensor_timestamp_us: Option<u64>,
    /// Whether conversion from the source format to I420 was needed.
    pub used_conversion: bool,
    /// Whether compressed image decoding was needed before conversion.
    pub used_decode_path: bool,
}

impl V4lFrame {
    /// Returns the decoded video frame.
    pub fn video_frame(&self) -> &VideoFrame<I420Buffer> {
        &self.frame
    }
}

/// Linux V4L2 capture session that emits decoded I420 frames.
pub struct V4lCaptureSession {
    #[cfg(target_os = "linux")]
    camera: Camera,
    format: CaptureFormat,
    options: V4lCaptureOptions,
    #[cfg(target_os = "linux")]
    started_at: Instant,
}

impl std::fmt::Debug for V4lCaptureSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("V4lCaptureSession");
        debug.field("format", &self.format);
        debug.field("options", &self.options);
        debug.finish()
    }
}

impl V4lCaptureSession {
    /// Opens a Linux V4L2 capture session.
    pub fn new(options: V4lCaptureOptions) -> Result<Self, V4lError> {
        validate_options(&options)?;
        Self::open(options)
    }

    /// Captures the next frame and converts it to I420.
    pub fn capture_frame(&mut self) -> Result<V4lFrame, V4lError> {
        self.capture_frame_inner()
    }

    /// Returns the negotiated capture format.
    pub fn format(&self) -> CaptureFormat {
        self.format
    }

    /// Returns the configured capture options.
    pub fn options(&self) -> &V4lCaptureOptions {
        &self.options
    }

    /// Returns the capture path produced by this session.
    pub fn capture_path(&self) -> CapturePath {
        CapturePath::Raw
    }

    #[cfg(target_os = "linux")]
    fn open(options: V4lCaptureOptions) -> Result<Self, V4lError> {
        let frame_formats = frame_formats_for_request(&options)?;
        let requested = RequestedFormat::with_formats(RequestedFormatType::None, &frame_formats);
        let mut camera = Camera::with_backend(
            camera_index(&options.device)?,
            requested,
            ApiBackend::Video4Linux,
        )
        .map_err(nokhwa_error)?;

        apply_format_request(&mut camera, &options, &frame_formats)?;

        camera.open_stream().map_err(nokhwa_error)?;
        let format = capture_format_from_nokhwa(camera.camera_format())?;
        Ok(Self { camera, format, options, started_at: Instant::now() })
    }

    #[cfg(not(target_os = "linux"))]
    fn open(_options: V4lCaptureOptions) -> Result<Self, V4lError> {
        Err(V4lError::UnsupportedPlatform)
    }

    #[cfg(target_os = "linux")]
    fn capture_frame_inner(&mut self) -> Result<V4lFrame, V4lError> {
        let fallback_wall_time_us = unix_time_us_now().unwrap_or_default();
        let buffer = self.camera.frame().map_err(nokhwa_error)?;
        let read_wall_time_us = unix_time_us_now().unwrap_or(fallback_wall_time_us);
        let backend_capture_timestamp = buffer.capture_timestamp();
        let capture_wall_time_us = select_capture_wall_time_us(
            backend_capture_timestamp,
            fallback_wall_time_us,
            read_wall_time_us,
        );

        let format = self.camera.camera_format();
        let width = format.width();
        let height = format.height();
        let mut frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: elapsed_us(self.started_at.elapsed()),
            frame_metadata: None,
            buffer: I420Buffer::new(width, height),
        };
        let used_decode_path = convert_to_i420(
            buffer.source_frame_format(),
            buffer.buffer(),
            width,
            height,
            &mut frame.buffer,
        )?;
        let source_format = capture_frame_format_from_nokhwa(buffer.source_frame_format())?;

        Ok(V4lFrame {
            frame,
            source_format,
            backend_capture_timestamp,
            capture_wall_time_us,
            read_wall_time_us,
            sensor_timestamp_us: None,
            used_conversion: source_format != CaptureFrameFormat::I420,
            used_decode_path,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn capture_frame_inner(&mut self) -> Result<V4lFrame, V4lError> {
        Err(V4lError::UnsupportedPlatform)
    }
}

/// Returns Linux V4L2 capture devices.
#[cfg(target_os = "linux")]
pub fn devices() -> Result<Vec<CaptureDeviceInfo>, V4lError> {
    nokhwa::query(ApiBackend::Video4Linux)
        .map_err(nokhwa_error)?
        .into_iter()
        .map(|info| {
            let formats = enumerate_formats(&info);
            let (formats, formats_complete) = match formats {
                Ok(formats) => (formats, true),
                Err(_) => (Vec::new(), false),
            };
            let id = info.index().as_string();
            Ok(CaptureDeviceInfo {
                backend: CaptureBackend::V4l2,
                id: id.clone(),
                selector: CaptureDeviceSelector::Id(id),
                name: info.human_name(),
                model_id: Some(info.description().to_string()).filter(|value| !value.is_empty()),
                manufacturer: None,
                paths: vec![CapturePath::Raw],
                formats,
                formats_complete,
            })
        })
        .collect()
}

/// Returns Linux V4L2 capture devices.
#[cfg(not(target_os = "linux"))]
pub fn devices() -> Result<Vec<CaptureDeviceInfo>, V4lError> {
    Err(V4lError::UnsupportedPlatform)
}

/// Returns the default ordered V4L source frame formats.
pub fn default_frame_formats() -> Vec<CaptureFrameFormat> {
    vec![
        CaptureFrameFormat::Yuyv,
        CaptureFrameFormat::Mjpeg,
        CaptureFrameFormat::Gray,
        CaptureFrameFormat::Rgb24,
        CaptureFrameFormat::Nv12,
    ]
}

/// Returns default V4L source frame formats with `first` preferred.
pub fn ordered_frame_formats_with_first(first: CaptureFrameFormat) -> Vec<CaptureFrameFormat> {
    ordered_formats_with_first(&default_frame_formats(), first)
}

fn validate_options(options: &V4lCaptureOptions) -> Result<(), V4lError> {
    match &options.device {
        CaptureDeviceSelector::Default => {}
        CaptureDeviceSelector::Index(index) => {
            u32::try_from(*index).map_err(|_| V4lError::OptionOutOfRange("device index"))?;
        }
        CaptureDeviceSelector::Id(id) => {
            if id.is_empty() {
                return Err(V4lError::InvalidOption("device id must be non-empty"));
            }
        }
    }

    if options.frame_formats.is_empty() {
        return Err(V4lError::InvalidOption("frame_formats must include at least one format"));
    }
    for frame_format in &options.frame_formats {
        if nokhwa_frame_format(*frame_format).is_none() {
            return Err(V4lError::UnsupportedFrameFormat(*frame_format));
        }
    }

    validate_format_request(&options.format)
}

fn validate_format_request(format: &CaptureFormatRequest) -> Result<(), V4lError> {
    let validate_format = |format: &CaptureFormat| {
        if format.resolution.width == 0 {
            return Err(V4lError::InvalidOption("width must be non-zero"));
        }
        if format.resolution.height == 0 {
            return Err(V4lError::InvalidOption("height must be non-zero"));
        }
        if format.frame_rate == 0 {
            return Err(V4lError::InvalidOption("frame_rate must be non-zero"));
        }
        if nokhwa_frame_format(format.frame_format).is_none() {
            return Err(V4lError::UnsupportedFrameFormat(format.frame_format));
        }
        Ok(())
    };

    match format {
        CaptureFormatRequest::Default => Ok(()),
        CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
            validate_format(format)
        }
        CaptureFormatRequest::HighestFrameRate { resolution, frame_format } => {
            if let Some(resolution) = resolution {
                validate_resolution(*resolution)?;
            }
            if let Some(frame_format) = frame_format {
                if nokhwa_frame_format(*frame_format).is_none() {
                    return Err(V4lError::UnsupportedFrameFormat(*frame_format));
                }
            }
            Ok(())
        }
        CaptureFormatRequest::HighestResolution { frame_rate, frame_format } => {
            if matches!(frame_rate, Some(0)) {
                return Err(V4lError::InvalidOption("frame_rate must be non-zero"));
            }
            if let Some(frame_format) = frame_format {
                if nokhwa_frame_format(*frame_format).is_none() {
                    return Err(V4lError::UnsupportedFrameFormat(*frame_format));
                }
            }
            Ok(())
        }
    }
}

fn validate_resolution(resolution: CaptureResolution) -> Result<(), V4lError> {
    if resolution.width == 0 {
        return Err(V4lError::InvalidOption("width must be non-zero"));
    }
    if resolution.height == 0 {
        return Err(V4lError::InvalidOption("height must be non-zero"));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn camera_index(selector: &CaptureDeviceSelector) -> Result<CameraIndex, V4lError> {
    match selector {
        CaptureDeviceSelector::Default => Ok(CameraIndex::Index(0)),
        CaptureDeviceSelector::Index(index) => Ok(CameraIndex::Index(
            u32::try_from(*index).map_err(|_| V4lError::OptionOutOfRange("device index"))?,
        )),
        CaptureDeviceSelector::Id(id) => Ok(CameraIndex::String(id.clone())),
    }
}

#[cfg(target_os = "linux")]
fn frame_formats_for_request(options: &V4lCaptureOptions) -> Result<Vec<FrameFormat>, V4lError> {
    let mut formats = match &options.format {
        CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
            ordered_formats_with_first(&options.frame_formats, format.frame_format)
        }
        CaptureFormatRequest::HighestFrameRate { frame_format: Some(frame_format), .. }
        | CaptureFormatRequest::HighestResolution { frame_format: Some(frame_format), .. } => {
            vec![*frame_format]
        }
        CaptureFormatRequest::Default
        | CaptureFormatRequest::HighestFrameRate { frame_format: None, .. }
        | CaptureFormatRequest::HighestResolution { frame_format: None, .. } => {
            options.frame_formats.clone()
        }
    };
    formats.dedup();
    formats
        .into_iter()
        .map(|format| nokhwa_frame_format(format).ok_or(V4lError::UnsupportedFrameFormat(format)))
        .collect()
}

fn ordered_formats_with_first(
    frame_formats: &[CaptureFrameFormat],
    first: CaptureFrameFormat,
) -> Vec<CaptureFrameFormat> {
    std::iter::once(first)
        .chain(frame_formats.iter().copied().filter(|format| *format != first))
        .collect()
}

#[cfg(target_os = "linux")]
fn requested_format<'a>(
    request: &CaptureFormatRequest,
    frame_formats: &'a [FrameFormat],
    override_format: Option<FrameFormat>,
) -> Result<RequestedFormat<'a>, V4lError> {
    let request_type = match request {
        CaptureFormatRequest::Default => RequestedFormatType::None,
        CaptureFormatRequest::Exact(format) => {
            RequestedFormatType::Exact(nokhwa_camera_format(*format, override_format)?)
        }
        CaptureFormatRequest::Closest(format) => {
            RequestedFormatType::Closest(nokhwa_camera_format(*format, override_format)?)
        }
        CaptureFormatRequest::HighestFrameRate { resolution: Some(resolution), .. } => {
            RequestedFormatType::HighestResolution(nokhwa_resolution(*resolution))
        }
        CaptureFormatRequest::HighestFrameRate { resolution: None, .. } => {
            RequestedFormatType::AbsoluteHighestFrameRate
        }
        CaptureFormatRequest::HighestResolution { frame_rate: Some(frame_rate), .. } => {
            RequestedFormatType::HighestFrameRate(*frame_rate)
        }
        CaptureFormatRequest::HighestResolution { frame_rate: None, .. } => {
            RequestedFormatType::AbsoluteHighestResolution
        }
    };
    Ok(RequestedFormat::with_formats(request_type, frame_formats))
}

#[cfg(target_os = "linux")]
fn apply_format_request(
    camera: &mut Camera,
    options: &V4lCaptureOptions,
    frame_formats: &[FrameFormat],
) -> Result<(), V4lError> {
    match options.format {
        CaptureFormatRequest::Default => Ok(()),
        CaptureFormatRequest::Exact(_) | CaptureFormatRequest::Closest(_) => {
            apply_ordered_format_request(camera, options, frame_formats)
        }
        CaptureFormatRequest::HighestFrameRate { .. }
        | CaptureFormatRequest::HighestResolution { .. } => {
            let selected = select_highest_format(
                &options.format,
                frame_formats,
                &camera.compatible_camera_formats().map_err(nokhwa_error)?,
            )?;
            camera
                .set_camera_requset(RequestedFormat::with_formats(
                    RequestedFormatType::Exact(selected),
                    &[selected.format()],
                ))
                .map(|_| ())
                .map_err(nokhwa_error)
        }
    }
}

#[cfg(target_os = "linux")]
fn apply_ordered_format_request(
    camera: &mut Camera,
    options: &V4lCaptureOptions,
    frame_formats: &[FrameFormat],
) -> Result<(), V4lError> {
    let mut last_error = None;
    for frame_format in frame_formats {
        let requested = requested_format(&options.format, frame_formats, Some(*frame_format))?;
        match camera.set_camera_requset(requested) {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error
        .map(nokhwa_error)
        .unwrap_or(V4lError::InvalidOption("no V4L frame formats were requested")))
}

#[cfg(target_os = "linux")]
fn select_highest_format(
    request: &CaptureFormatRequest,
    frame_formats: &[FrameFormat],
    all_formats: &[CameraFormat],
) -> Result<CameraFormat, V4lError> {
    let candidates = all_formats
        .iter()
        .copied()
        .filter(|format| frame_formats.contains(&format.format()))
        .filter(|format| match request {
            CaptureFormatRequest::HighestFrameRate { resolution, .. } => resolution
                .map(|resolution| format.resolution() == nokhwa_resolution(resolution))
                .unwrap_or(true),
            CaptureFormatRequest::HighestResolution { frame_rate, .. } => {
                frame_rate.map(|frame_rate| format.frame_rate() == frame_rate).unwrap_or(true)
            }
            CaptureFormatRequest::Default
            | CaptureFormatRequest::Exact(_)
            | CaptureFormatRequest::Closest(_) => false,
        });

    let selected = match request {
        CaptureFormatRequest::HighestFrameRate { .. } => candidates.max_by(|a, b| {
            a.frame_rate()
                .cmp(&b.frame_rate())
                .then_with(|| a.resolution().cmp(&b.resolution()))
                .then_with(|| compare_format_preference(a.format(), b.format(), frame_formats))
        }),
        CaptureFormatRequest::HighestResolution { .. } => candidates.max_by(|a, b| {
            a.resolution()
                .cmp(&b.resolution())
                .then_with(|| a.frame_rate().cmp(&b.frame_rate()))
                .then_with(|| compare_format_preference(a.format(), b.format(), frame_formats))
        }),
        CaptureFormatRequest::Default
        | CaptureFormatRequest::Exact(_)
        | CaptureFormatRequest::Closest(_) => None,
    };

    selected.ok_or_else(|| V4lError::Camera("CameraFormat: Failed to Fufill".to_string()))
}

#[cfg(target_os = "linux")]
fn compare_format_preference(
    left: FrameFormat,
    right: FrameFormat,
    frame_formats: &[FrameFormat],
) -> std::cmp::Ordering {
    let left_index = frame_formats.iter().position(|format| *format == left).unwrap_or(usize::MAX);
    let right_index =
        frame_formats.iter().position(|format| *format == right).unwrap_or(usize::MAX);
    right_index.cmp(&left_index)
}

#[cfg(target_os = "linux")]
fn nokhwa_camera_format(
    format: CaptureFormat,
    override_format: Option<FrameFormat>,
) -> Result<CameraFormat, V4lError> {
    let frame_format = match override_format {
        Some(format) => format,
        None => nokhwa_frame_format(format.frame_format)
            .ok_or(V4lError::UnsupportedFrameFormat(format.frame_format))?,
    };
    Ok(CameraFormat::new(nokhwa_resolution(format.resolution), frame_format, format.frame_rate))
}

#[cfg(target_os = "linux")]
fn nokhwa_resolution(resolution: CaptureResolution) -> Resolution {
    Resolution::new(resolution.width, resolution.height)
}

#[cfg(target_os = "linux")]
fn nokhwa_frame_format(pixel_format: CaptureFrameFormat) -> Option<FrameFormat> {
    match pixel_format {
        CaptureFrameFormat::Nv12 => Some(FrameFormat::NV12),
        CaptureFrameFormat::Rgb24 => Some(FrameFormat::RAWRGB),
        CaptureFrameFormat::Bgr24 => Some(FrameFormat::RAWBGR),
        CaptureFrameFormat::Yuyv => Some(FrameFormat::YUYV),
        CaptureFrameFormat::Gray => Some(FrameFormat::GRAY),
        CaptureFrameFormat::Mjpeg => Some(FrameFormat::MJPEG),
        CaptureFrameFormat::I420 | CaptureFrameFormat::Bgra | CaptureFrameFormat::Uyvy => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn nokhwa_frame_format(pixel_format: CaptureFrameFormat) -> Option<()> {
    match pixel_format {
        CaptureFrameFormat::Nv12
        | CaptureFrameFormat::Rgb24
        | CaptureFrameFormat::Bgr24
        | CaptureFrameFormat::Yuyv
        | CaptureFrameFormat::Gray
        | CaptureFrameFormat::Mjpeg => Some(()),
        CaptureFrameFormat::I420 | CaptureFrameFormat::Bgra | CaptureFrameFormat::Uyvy => None,
    }
}

#[cfg(target_os = "linux")]
fn capture_format_from_nokhwa(format: CameraFormat) -> Result<CaptureFormat, V4lError> {
    Ok(CaptureFormat::new(
        CaptureResolution::new(format.width(), format.height()),
        format.frame_rate(),
        capture_frame_format_from_nokhwa(format.format())?,
    ))
}

#[cfg(target_os = "linux")]
fn capture_frame_format_from_nokhwa(format: FrameFormat) -> Result<CaptureFrameFormat, V4lError> {
    match format {
        FrameFormat::MJPEG => Ok(CaptureFrameFormat::Mjpeg),
        FrameFormat::YUYV => Ok(CaptureFrameFormat::Yuyv),
        FrameFormat::NV12 => Ok(CaptureFrameFormat::Nv12),
        FrameFormat::GRAY => Ok(CaptureFrameFormat::Gray),
        FrameFormat::RAWRGB => Ok(CaptureFrameFormat::Rgb24),
        FrameFormat::RAWBGR => Ok(CaptureFrameFormat::Bgr24),
    }
}

#[cfg(target_os = "linux")]
fn enumerate_formats(info: &nokhwa::utils::CameraInfo) -> Result<Vec<CaptureFormat>, V4lError> {
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let mut camera = Camera::with_backend(info.index().clone(), requested, ApiBackend::Video4Linux)
        .map_err(nokhwa_error)?;

    Ok(camera
        .compatible_camera_formats()
        .map_err(nokhwa_error)?
        .into_iter()
        .filter_map(|format| capture_format_from_nokhwa(format).ok())
        .collect())
}

#[cfg(target_os = "linux")]
fn convert_to_i420(
    source_format: FrameFormat,
    source: &[u8],
    width: u32,
    height: u32,
    destination: &mut I420Buffer,
) -> Result<bool, V4lError> {
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (dst_y, dst_u, dst_v) = destination.data_mut();
    let width_i32 = i32_from_u32(width, "width")?;
    let height_i32 = i32_from_u32(height, "height")?;

    let ret = match source_format {
        FrameFormat::YUYV => {
            validate_len(source, width as usize * height as usize * 2, "YUYV frame")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_YUY2ToI420(
                    source.as_ptr(),
                    width_i32 * 2,
                    dst_y.as_mut_ptr(),
                    stride_y as i32,
                    dst_u.as_mut_ptr(),
                    stride_u as i32,
                    dst_v.as_mut_ptr(),
                    stride_v as i32,
                    width_i32,
                    height_i32,
                )
            }
        }
        FrameFormat::RAWRGB => {
            validate_len(source, width as usize * height as usize * 3, "RGB24 frame")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_RGB24ToI420(
                    source.as_ptr(),
                    width_i32 * 3,
                    dst_y.as_mut_ptr(),
                    stride_y as i32,
                    dst_u.as_mut_ptr(),
                    stride_u as i32,
                    dst_v.as_mut_ptr(),
                    stride_v as i32,
                    width_i32,
                    height_i32,
                )
            }
        }
        FrameFormat::RAWBGR => {
            validate_len(source, width as usize * height as usize * 3, "BGR24 frame")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_RAWToI420(
                    source.as_ptr(),
                    width_i32 * 3,
                    dst_y.as_mut_ptr(),
                    stride_y as i32,
                    dst_u.as_mut_ptr(),
                    stride_u as i32,
                    dst_v.as_mut_ptr(),
                    stride_v as i32,
                    width_i32,
                    height_i32,
                )
            }
        }
        FrameFormat::GRAY => {
            validate_len(source, width as usize * height as usize, "GRAY frame")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_I400ToI420(
                    source.as_ptr(),
                    width_i32,
                    dst_y.as_mut_ptr(),
                    stride_y as i32,
                    dst_u.as_mut_ptr(),
                    stride_u as i32,
                    dst_v.as_mut_ptr(),
                    stride_v as i32,
                    width_i32,
                    height_i32,
                )
            }
        }
        FrameFormat::NV12 => {
            let y_size = width as usize * height as usize;
            validate_len(source, y_size + y_size / 2, "NV12 frame")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_NV12ToI420(
                    source.as_ptr(),
                    width_i32,
                    source[y_size..].as_ptr(),
                    width_i32,
                    dst_y.as_mut_ptr(),
                    stride_y as i32,
                    dst_u.as_mut_ptr(),
                    stride_u as i32,
                    dst_v.as_mut_ptr(),
                    stride_v as i32,
                    width_i32,
                    height_i32,
                )
            }
        }
        FrameFormat::MJPEG => {
            return convert_mjpeg_to_i420(source, width, height, destination).map(|()| true);
        }
    };

    if ret == 0 {
        Ok(false)
    } else {
        Err(V4lError::Convert("libyuv conversion failed"))
    }
}

#[cfg(target_os = "linux")]
fn convert_mjpeg_to_i420(
    source: &[u8],
    width: u32,
    height: u32,
    destination: &mut I420Buffer,
) -> Result<(), V4lError> {
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (dst_y, dst_u, dst_v) = destination.data_mut();
    let width_i32 = i32_from_u32(width, "width")?;
    let height_i32 = i32_from_u32(height, "height")?;

    let ret = unsafe {
        // SAFETY: Source and destination slices are valid for the dimensions and strides.
        yuv_sys::rs_MJPGToI420(
            source.as_ptr(),
            source.len(),
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width_i32,
            height_i32,
            width_i32,
            height_i32,
        )
    };
    if ret == 0 {
        return Ok(());
    }

    let rgb = image::load_from_memory(source)
        .map_err(|error| V4lError::Decode(error.to_string()))?
        .to_rgb8();
    if rgb.width() != width || rgb.height() != height {
        return Err(V4lError::InvalidFrame("decoded MJPEG dimensions changed"));
    }
    let ret = unsafe {
        // SAFETY: Source and destination slices are valid for the dimensions and strides.
        yuv_sys::rs_RGB24ToI420(
            rgb.as_raw().as_ptr(),
            width_i32 * 3,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width_i32,
            height_i32,
        )
    };
    if ret == 0 {
        Ok(())
    } else {
        Err(V4lError::Convert("RGB24 fallback conversion failed"))
    }
}

#[cfg(target_os = "linux")]
fn validate_len(source: &[u8], expected: usize, label: &'static str) -> Result<(), V4lError> {
    if source.len() < expected {
        return Err(V4lError::InvalidFrame(label));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn select_capture_wall_time_us(
    backend_capture_timestamp: Option<Duration>,
    fallback_wall_time_us: u64,
    read_wall_time_us: u64,
) -> u64 {
    backend_capture_timestamp
        .and_then(|timestamp| validate_backend_capture_timestamp_us(timestamp, read_wall_time_us))
        .unwrap_or(fallback_wall_time_us)
}

#[cfg(any(target_os = "linux", test))]
fn validate_backend_capture_timestamp_us(
    capture_timestamp: Duration,
    read_wall_time_us: u64,
) -> Option<u64> {
    let capture_timestamp_us = u64::try_from(capture_timestamp.as_micros()).ok()?;
    if capture_timestamp_us == 0 || capture_timestamp_us > read_wall_time_us {
        return None;
    }
    if read_wall_time_us - capture_timestamp_us > MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US {
        return None;
    }
    Some(capture_timestamp_us)
}

#[cfg(target_os = "linux")]
fn unix_time_us_now() -> Option<u64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_micros()).ok()
}

#[cfg(target_os = "linux")]
fn elapsed_us(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

#[cfg(target_os = "linux")]
fn i32_from_u32(value: u32, field: &'static str) -> Result<i32, V4lError> {
    i32::try_from(value).map_err(|_| V4lError::OptionOutOfRange(field))
}

#[cfg(target_os = "linux")]
fn nokhwa_error(error: nokhwa::NokhwaError) -> V4lError {
    V4lError::Camera(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_frame_format_preferences() {
        let mut options = V4lCaptureOptions::default();
        options.frame_formats.clear();
        let err = V4lCaptureSession::new(options).expect_err("empty formats must be rejected");
        assert!(matches!(err, V4lError::InvalidOption(_)));
    }

    #[test]
    fn rejects_unsupported_i420_source_format() {
        let mut options = V4lCaptureOptions::default();
        options.frame_formats = vec![CaptureFrameFormat::I420];
        let err = V4lCaptureSession::new(options).expect_err("I420 source must be rejected");
        assert!(matches!(err, V4lError::UnsupportedFrameFormat(CaptureFrameFormat::I420)));
    }

    #[test]
    fn rejects_zero_frame_rate() {
        let options = V4lCaptureOptions::new(
            CaptureDeviceSelector::Default,
            CaptureResolution::new(640, 480),
            0,
        );
        let err = V4lCaptureSession::new(options).expect_err("zero fps must be rejected");
        assert!(matches!(err, V4lError::InvalidOption(_)));
    }

    #[test]
    fn ignores_stream_relative_capture_timestamp() {
        let selected =
            select_capture_wall_time_us(Some(Duration::from_micros(10)), 10_000_000, 10_000_000);
        assert_eq!(selected, 10_000_000);
    }
}
