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

//! Linux V4L2 capture using direct V4L2 access.

use std::time::Duration;
#[cfg(target_os = "linux")]
use std::{path::Path, time::Instant};

#[cfg(target_os = "linux")]
use livekit::webrtc::video_frame::VideoRotation;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame};
use thiserror::Error;
#[cfg(target_os = "linux")]
use v4l::{
    buffer::Type as V4lBufferType,
    capability::Flags as V4lCapabilityFlags,
    context,
    format::{Format as V4lFormat, FourCC},
    frameinterval::FrameIntervalEnum,
    framesize::FrameSizeEnum,
    io::{mmap::Stream as MmapStream, traits::CaptureStream},
    video::{capture::Parameters as V4lCaptureParameters, Capture},
    Device,
};

#[cfg(target_os = "linux")]
use crate::device::CaptureBackend;
use crate::device::{
    CaptureDeviceInfo, CaptureDeviceSelector, CaptureFormat, CaptureFormatRequest,
    CaptureFrameFormat, CapturePath, CaptureResolution,
};
#[cfg(any(target_os = "linux", test))]
use crate::time::validate_capture_timestamp_us;
#[cfg(target_os = "linux")]
use crate::time::{elapsed_us, unix_time_us_now};

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
    /// Creates options that try YUYV, MJPEG, greyscale, RGB24, and NV12 at the requested format.
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
    /// A numeric option could not be represented by the V4L backend.
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
    stream: MmapStream<'static>,
    format: CaptureFormat,
    /// Driver-reported row stride in bytes (V4L2 `bytesperline`).
    #[cfg(target_os = "linux")]
    stride: u32,
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
        let device = open_device(&options.device)?;
        let all_formats = enumerate_device_formats(&device)?;
        let (format, stride) =
            apply_format_request(&device, &options, &frame_formats, &all_formats)?;
        let stream =
            MmapStream::with_buffers(&device, V4lBufferType::VideoCapture, 4).map_err(v4l_error)?;
        Ok(Self { stream, format, stride, options, started_at: Instant::now() })
    }

    #[cfg(not(target_os = "linux"))]
    fn open(_options: V4lCaptureOptions) -> Result<Self, V4lError> {
        Err(V4lError::UnsupportedPlatform)
    }

    #[cfg(target_os = "linux")]
    fn capture_frame_inner(&mut self) -> Result<V4lFrame, V4lError> {
        let fallback_wall_time_us = unix_time_us_now().unwrap_or_default();
        let format = self.format;
        let (buffer, metadata) = self.stream.next().map_err(v4l_error)?;
        let read_wall_time_us = unix_time_us_now().unwrap_or(fallback_wall_time_us);
        let backend_capture_timestamp = monotonic_to_wallclock(metadata.timestamp);
        let capture_wall_time_us = select_capture_wall_time_us(
            backend_capture_timestamp,
            fallback_wall_time_us,
            read_wall_time_us,
        );

        let width = format.resolution.width;
        let height = format.resolution.height;
        let mut frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: elapsed_us(self.started_at.elapsed()),
            frame_metadata: None,
            buffer: I420Buffer::new(width, height),
        };
        let source = frame_bytes(buffer, metadata.bytesused);
        let used_decode_path = convert_to_i420(
            format.frame_format,
            source,
            width,
            height,
            self.stride,
            &mut frame.buffer,
        )?;

        Ok(V4lFrame {
            frame,
            source_format: format.frame_format,
            backend_capture_timestamp,
            capture_wall_time_us,
            read_wall_time_us,
            sensor_timestamp_us: None,
            used_conversion: format.frame_format != CaptureFrameFormat::I420,
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
    context::enum_devices()
        .into_iter()
        .filter_map(|node| {
            let id = node.index().to_string();
            let fallback_name =
                node.name().unwrap_or_else(|| node.path().to_string_lossy().into_owned());
            let mut name = fallback_name;
            let mut model_id = None;
            let mut manufacturer = None;
            let mut formats = Vec::new();
            let mut formats_complete = false;

            if let Ok(device) = Device::with_path(node.path()) {
                if let Ok(capabilities) = device.query_caps() {
                    if !capabilities.capabilities.contains(V4lCapabilityFlags::VIDEO_CAPTURE) {
                        return None;
                    }
                    if !capabilities.card.is_empty() {
                        name = capabilities.card;
                    }
                    model_id = Some(capabilities.bus).filter(|value| !value.is_empty());
                    manufacturer = Some(capabilities.driver).filter(|value| !value.is_empty());
                }

                if let Ok(device_formats) = enumerate_device_formats(&device) {
                    formats = device_formats;
                    formats_complete = true;
                }
            };

            Some(Ok(CaptureDeviceInfo {
                backend: CaptureBackend::V4l2,
                id: id.clone(),
                selector: CaptureDeviceSelector::Id(id),
                name,
                model_id,
                manufacturer,
                paths: vec![CapturePath::Raw],
                formats,
                formats_complete,
            }))
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
        CaptureFrameFormat::Grey,
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
        if !is_supported_source_format(*frame_format) {
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
        if !is_supported_source_format(format.frame_format) {
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
                if !is_supported_source_format(*frame_format) {
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
                if !is_supported_source_format(*frame_format) {
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
fn open_device(selector: &CaptureDeviceSelector) -> Result<Device, V4lError> {
    match selector {
        CaptureDeviceSelector::Default => Device::new(0).map_err(v4l_error),
        CaptureDeviceSelector::Index(index) => Device::new(*index).map_err(v4l_error),
        CaptureDeviceSelector::Id(id) => open_device_id(id),
    }
}

#[cfg(target_os = "linux")]
fn open_device_id(id: &str) -> Result<Device, V4lError> {
    if let Ok(index) = id.parse::<usize>() {
        return Device::new(index).map_err(v4l_error);
    }

    Device::with_path(Path::new(id)).map_err(v4l_error)
}

#[cfg(target_os = "linux")]
fn frame_formats_for_request(
    options: &V4lCaptureOptions,
) -> Result<Vec<CaptureFrameFormat>, V4lError> {
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
    for format in &formats {
        if !is_supported_source_format(*format) {
            return Err(V4lError::UnsupportedFrameFormat(*format));
        }
    }
    Ok(formats)
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
fn apply_format_request(
    device: &Device,
    options: &V4lCaptureOptions,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Result<(CaptureFormat, u32), V4lError> {
    match options.format {
        CaptureFormatRequest::Default => {
            let selected = select_format_for_request(&options.format, frame_formats, all_formats)?;
            set_device_format(device, selected)
        }
        CaptureFormatRequest::Exact(_) | CaptureFormatRequest::Closest(_) => {
            apply_ordered_format_request(device, options, frame_formats, all_formats)
        }
        CaptureFormatRequest::HighestFrameRate { .. }
        | CaptureFormatRequest::HighestResolution { .. } => {
            let selected = select_format_for_request(&options.format, frame_formats, all_formats)?;
            set_device_format(device, selected)
        }
    }
}

#[cfg(target_os = "linux")]
fn apply_ordered_format_request(
    device: &Device,
    options: &V4lCaptureOptions,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Result<(CaptureFormat, u32), V4lError> {
    let mut last_error = None;
    for frame_format in frame_formats {
        let request = format_request_with_frame_format(&options.format, *frame_format);
        let selected = match select_format_for_request(&request, &[*frame_format], all_formats) {
            Ok(selected) => selected,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };

        match set_device_format(device, selected) {
            Ok(format) => return Ok(format),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or(V4lError::InvalidOption("no V4L frame formats were requested")))
}

#[cfg(target_os = "linux")]
fn format_request_with_frame_format(
    request: &CaptureFormatRequest,
    frame_format: CaptureFrameFormat,
) -> CaptureFormatRequest {
    match request {
        CaptureFormatRequest::Exact(format) => CaptureFormatRequest::Exact(CaptureFormat::new(
            format.resolution,
            format.frame_rate,
            frame_format,
        )),
        CaptureFormatRequest::Closest(format) => CaptureFormatRequest::Closest(CaptureFormat::new(
            format.resolution,
            format.frame_rate,
            frame_format,
        )),
        CaptureFormatRequest::Default => CaptureFormatRequest::Default,
        CaptureFormatRequest::HighestFrameRate { resolution, .. } => {
            CaptureFormatRequest::HighestFrameRate {
                resolution: *resolution,
                frame_format: Some(frame_format),
            }
        }
        CaptureFormatRequest::HighestResolution { frame_rate, .. } => {
            CaptureFormatRequest::HighestResolution {
                frame_rate: *frame_rate,
                frame_format: Some(frame_format),
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn select_format_for_request(
    request: &CaptureFormatRequest,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Result<CaptureFormat, V4lError> {
    let selected = match request {
        CaptureFormatRequest::Default => {
            all_formats.iter().find(|format| frame_formats.contains(&format.frame_format)).copied()
        }
        CaptureFormatRequest::Exact(format) => {
            if frame_formats.contains(&format.frame_format) {
                Some(*format)
            } else {
                None
            }
        }
        CaptureFormatRequest::Closest(format) => {
            select_closest_format(*format, frame_formats, all_formats)
        }
        CaptureFormatRequest::HighestFrameRate { .. } => {
            select_highest_frame_rate_format(request, frame_formats, all_formats)
        }
        CaptureFormatRequest::HighestResolution { .. } => {
            select_highest_resolution_format(request, frame_formats, all_formats)
        }
    };

    selected.ok_or_else(|| V4lError::Camera("CameraFormat: Failed to Fufill".to_string()))
}

#[cfg(target_os = "linux")]
fn select_closest_format(
    requested: CaptureFormat,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Option<CaptureFormat> {
    if !frame_formats.contains(&requested.frame_format) {
        return None;
    }

    let resolution = all_formats
        .iter()
        .copied()
        .filter(|format| format.frame_format == requested.frame_format)
        .min_by_key(|format| resolution_distance(format.resolution, requested.resolution))?
        .resolution;

    let frame_rate = all_formats
        .iter()
        .copied()
        .filter(|format| {
            format.frame_format == requested.frame_format && format.resolution == resolution
        })
        .min_by_key(|format| format.frame_rate.abs_diff(requested.frame_rate))?
        .frame_rate;

    Some(CaptureFormat::new(resolution, frame_rate, requested.frame_format))
}

#[cfg(target_os = "linux")]
fn select_highest_frame_rate_format(
    request: &CaptureFormatRequest,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Option<CaptureFormat> {
    all_formats
        .iter()
        .copied()
        .filter(|format| frame_formats.contains(&format.frame_format))
        .filter(|format| match request {
            CaptureFormatRequest::HighestFrameRate { resolution, frame_format } => {
                resolution.map(|resolution| format.resolution == resolution).unwrap_or(true)
                    && frame_format
                        .map(|frame_format| format.frame_format == frame_format)
                        .unwrap_or(true)
            }
            _ => false,
        })
        .max_by(|left, right| {
            left.frame_rate
                .cmp(&right.frame_rate)
                .then_with(|| compare_resolution(left.resolution, right.resolution))
                .then_with(|| {
                    compare_format_preference(left.frame_format, right.frame_format, frame_formats)
                })
        })
}

#[cfg(target_os = "linux")]
fn select_highest_resolution_format(
    request: &CaptureFormatRequest,
    frame_formats: &[CaptureFrameFormat],
    all_formats: &[CaptureFormat],
) -> Option<CaptureFormat> {
    all_formats
        .iter()
        .copied()
        .filter(|format| frame_formats.contains(&format.frame_format))
        .filter(|format| match request {
            CaptureFormatRequest::HighestResolution { frame_rate, frame_format } => {
                frame_rate.map(|frame_rate| format.frame_rate == frame_rate).unwrap_or(true)
                    && frame_format
                        .map(|frame_format| format.frame_format == frame_format)
                        .unwrap_or(true)
            }
            _ => false,
        })
        .max_by(|left, right| {
            compare_resolution(left.resolution, right.resolution)
                .then_with(|| left.frame_rate.cmp(&right.frame_rate))
                .then_with(|| {
                    compare_format_preference(left.frame_format, right.frame_format, frame_formats)
                })
        })
}

#[cfg(target_os = "linux")]
fn compare_resolution(left: CaptureResolution, right: CaptureResolution) -> std::cmp::Ordering {
    frame_area(left)
        .cmp(&frame_area(right))
        .then_with(|| left.width.cmp(&right.width))
        .then_with(|| left.height.cmp(&right.height))
}

#[cfg(target_os = "linux")]
fn resolution_distance(left: CaptureResolution, right: CaptureResolution) -> u64 {
    let width = i64::from(left.width) - i64::from(right.width);
    let height = i64::from(left.height) - i64::from(right.height);
    width.unsigned_abs().pow(2) + height.unsigned_abs().pow(2)
}

#[cfg(target_os = "linux")]
fn frame_area(resolution: CaptureResolution) -> u64 {
    u64::from(resolution.width) * u64::from(resolution.height)
}

#[cfg(target_os = "linux")]
fn compare_format_preference(
    left: CaptureFrameFormat,
    right: CaptureFrameFormat,
    frame_formats: &[CaptureFrameFormat],
) -> std::cmp::Ordering {
    let left_index = frame_formats.iter().position(|format| *format == left).unwrap_or(usize::MAX);
    let right_index =
        frame_formats.iter().position(|format| *format == right).unwrap_or(usize::MAX);
    right_index.cmp(&left_index)
}

#[cfg(target_os = "linux")]
fn set_device_format(
    device: &Device,
    selected: CaptureFormat,
) -> Result<(CaptureFormat, u32), V4lError> {
    let (current, _) = device_capture_format(device)?;
    let format_changed =
        current.resolution != selected.resolution || current.frame_format != selected.frame_format;
    if format_changed {
        device
            .set_format(&V4lFormat::new(
                selected.resolution.width,
                selected.resolution.height,
                fourcc_for_frame_format(selected.frame_format)
                    .ok_or(V4lError::UnsupportedFrameFormat(selected.frame_format))?,
            ))
            .map_err(v4l_error)?;
    }
    if format_changed || current.frame_rate != selected.frame_rate {
        device
            .set_params(&V4lCaptureParameters::with_fps(selected.frame_rate))
            .map_err(v4l_error)?;
    }

    let (actual, stride) = device_capture_format(device)?;
    if actual != selected {
        return Err(V4lError::Camera(format!(
            "CameraFormat rejected: requested {:?}, got {:?}",
            selected, actual
        )));
    }
    Ok((actual, stride))
}

/// Returns the device's current capture format and its row stride in bytes
/// (V4L2 `bytesperline`).
#[cfg(target_os = "linux")]
fn device_capture_format(device: &Device) -> Result<(CaptureFormat, u32), V4lError> {
    let format = device.format().map_err(v4l_error)?;
    let params = device.params().map_err(v4l_error)?;
    let frame_rate =
        frame_rate_from_fraction(params.interval.numerator, params.interval.denominator)
            .ok_or(V4lError::InvalidOption("V4L frame interval must be non-zero"))?;
    let capture_format = CaptureFormat::new(
        CaptureResolution::new(format.width, format.height),
        frame_rate,
        capture_frame_format_from_fourcc(format.fourcc)
            .ok_or_else(|| V4lError::Camera(format!("unsupported V4L fourcc {}", format.fourcc)))?,
    );
    Ok((capture_format, format.stride))
}

#[cfg(target_os = "linux")]
fn enumerate_device_formats(device: &Device) -> Result<Vec<CaptureFormat>, V4lError> {
    let mut formats = Vec::new();
    let fourccs = device
        .enum_formats()
        .map_err(v4l_error)?
        .into_iter()
        .filter_map(|format| capture_frame_format_from_fourcc(format.fourcc).map(|_| format.fourcc))
        .collect::<Vec<_>>();

    for fourcc in dedup_fourccs(fourccs) {
        let Some(frame_format) = capture_frame_format_from_fourcc(fourcc) else {
            continue;
        };
        let frame_sizes = device.enum_framesizes(fourcc).map_err(v4l_error)?;
        for resolution in frame_sizes.into_iter().flat_map(resolutions_from_frame_size) {
            let intervals = device
                .enum_frameintervals(fourcc, resolution.width, resolution.height)
                .unwrap_or_default();
            for frame_rate in intervals.into_iter().flat_map(frame_rates_from_interval) {
                formats.push(CaptureFormat::new(resolution, frame_rate, frame_format));
            }
        }
    }

    Ok(formats)
}

fn is_supported_source_format(frame_format: CaptureFrameFormat) -> bool {
    matches!(
        frame_format,
        CaptureFrameFormat::Nv12
            | CaptureFrameFormat::Rgb24
            | CaptureFrameFormat::Bgr24
            | CaptureFrameFormat::Yuyv
            | CaptureFrameFormat::Grey
            | CaptureFrameFormat::Mjpeg
    )
}

#[cfg(target_os = "linux")]
fn fourcc_for_frame_format(frame_format: CaptureFrameFormat) -> Option<FourCC> {
    match frame_format {
        CaptureFrameFormat::Nv12 => Some(FourCC::new(b"NV12")),
        CaptureFrameFormat::Rgb24 => Some(FourCC::new(b"RGB3")),
        CaptureFrameFormat::Bgr24 => Some(FourCC::new(b"BGR3")),
        CaptureFrameFormat::Yuyv => Some(FourCC::new(b"YUYV")),
        CaptureFrameFormat::Grey => Some(FourCC::new(b"GREY")),
        CaptureFrameFormat::Mjpeg => Some(FourCC::new(b"MJPG")),
        CaptureFrameFormat::I420 | CaptureFrameFormat::Bgra | CaptureFrameFormat::Uyvy => None,
    }
}

#[cfg(target_os = "linux")]
fn capture_frame_format_from_fourcc(fourcc: FourCC) -> Option<CaptureFrameFormat> {
    match fourcc.str().ok()? {
        "NV12" => Some(CaptureFrameFormat::Nv12),
        "RGB3" => Some(CaptureFrameFormat::Rgb24),
        "BGR3" => Some(CaptureFrameFormat::Bgr24),
        "YUYV" | "YUY2" => Some(CaptureFrameFormat::Yuyv),
        "GREY" => Some(CaptureFrameFormat::Grey),
        "MJPG" | "JPEG" => Some(CaptureFrameFormat::Mjpeg),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn dedup_fourccs(fourccs: Vec<FourCC>) -> Vec<FourCC> {
    let mut deduped = Vec::new();
    for fourcc in fourccs {
        if !deduped.contains(&fourcc) {
            deduped.push(fourcc);
        }
    }
    deduped
}

#[cfg(target_os = "linux")]
fn resolutions_from_frame_size(size: v4l::FrameSize) -> Vec<CaptureResolution> {
    match size.size {
        FrameSizeEnum::Discrete(discrete) => {
            vec![CaptureResolution::new(discrete.width, discrete.height)]
        }
        FrameSizeEnum::Stepwise(stepwise) => {
            let mut resolutions = Vec::new();
            push_stepwise_resolution(
                &mut resolutions,
                CaptureResolution::new(stepwise.min_width, stepwise.min_height),
            );
            push_stepwise_resolution(
                &mut resolutions,
                CaptureResolution::new(stepwise.max_width, stepwise.max_height),
            );
            resolutions
        }
    }
}

#[cfg(target_os = "linux")]
fn push_stepwise_resolution(
    resolutions: &mut Vec<CaptureResolution>,
    resolution: CaptureResolution,
) {
    if resolution.width != 0 && resolution.height != 0 && !resolutions.contains(&resolution) {
        resolutions.push(resolution);
    }
}

#[cfg(target_os = "linux")]
fn frame_rates_from_interval(interval: v4l::FrameInterval) -> Vec<u32> {
    match interval.interval {
        FrameIntervalEnum::Discrete(fraction) => {
            frame_rate_from_fraction(fraction.numerator, fraction.denominator).into_iter().collect()
        }
        FrameIntervalEnum::Stepwise(stepwise) => {
            let mut frame_rates = Vec::new();
            for fraction in [stepwise.min, stepwise.max] {
                if let Some(frame_rate) =
                    frame_rate_from_fraction(fraction.numerator, fraction.denominator)
                {
                    if !frame_rates.contains(&frame_rate) {
                        frame_rates.push(frame_rate);
                    }
                }
            }
            frame_rates
        }
    }
}

/// Converts a V4L2 frame interval (seconds per frame) to frames per second.
///
/// Non-integer rates (e.g. the NTSC interval 1001/30000 = 29.97fps) round to
/// the nearest whole rate, never below 1.
#[cfg(any(target_os = "linux", test))]
fn frame_rate_from_fraction(numerator: u32, denominator: u32) -> Option<u32> {
    if numerator == 0 || denominator == 0 {
        return None;
    }
    if denominator % numerator == 0 {
        return Some(denominator / numerator);
    }
    let rounded = (u64::from(denominator) + u64::from(numerator) / 2) / u64::from(numerator);
    Some(u32::try_from(rounded).unwrap_or(u32::MAX).max(1))
}

#[cfg(target_os = "linux")]
fn frame_bytes(buffer: &[u8], bytes_used: u32) -> &[u8] {
    let bytes_used = usize::try_from(bytes_used).unwrap_or(buffer.len()).min(buffer.len());
    if bytes_used == 0 {
        buffer
    } else {
        &buffer[..bytes_used]
    }
}

#[cfg(any(target_os = "linux", test))]
fn convert_to_i420(
    source_format: CaptureFrameFormat,
    source: &[u8],
    width: u32,
    height: u32,
    source_stride: u32,
    destination: &mut I420Buffer,
) -> Result<bool, V4lError> {
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (dst_y, dst_u, dst_v) = destination.data_mut();
    let width_i32 = i32_from_u32(width, "width")?;
    let height_i32 = i32_from_u32(height, "height")?;

    let ret = match source_format {
        CaptureFrameFormat::Yuyv => {
            let stride = source_row_stride(source_stride, width as usize * 2);
            validate_len(source, stride * height as usize, "YUYV frame")?;
            let stride_i32 = i32_from_usize(stride, "stride")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_YUY2ToI420(
                    source.as_ptr(),
                    stride_i32,
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
        CaptureFrameFormat::Rgb24 => {
            let stride = source_row_stride(source_stride, width as usize * 3);
            validate_len(source, stride * height as usize, "RGB24 frame")?;
            let stride_i32 = i32_from_usize(stride, "stride")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_RGB24ToI420(
                    source.as_ptr(),
                    stride_i32,
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
        CaptureFrameFormat::Bgr24 => {
            let stride = source_row_stride(source_stride, width as usize * 3);
            validate_len(source, stride * height as usize, "BGR24 frame")?;
            let stride_i32 = i32_from_usize(stride, "stride")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_RAWToI420(
                    source.as_ptr(),
                    stride_i32,
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
        CaptureFrameFormat::Grey => {
            let stride = source_row_stride(source_stride, width as usize);
            validate_len(source, stride * height as usize, "GREY frame")?;
            let stride_i32 = i32_from_usize(stride, "stride")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_I400ToI420(
                    source.as_ptr(),
                    stride_i32,
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
        CaptureFrameFormat::Nv12 => {
            // Single-planar V4L2 NV12: the interleaved chroma plane follows the
            // luma plane at `stride * height` and shares the luma stride.
            let stride = source_row_stride(source_stride, width as usize);
            let y_size = stride * height as usize;
            validate_len(source, y_size + y_size / 2, "NV12 frame")?;
            let stride_i32 = i32_from_usize(stride, "stride")?;
            unsafe {
                // SAFETY: Source and destination slices are valid for the dimensions and strides.
                yuv_sys::rs_NV12ToI420(
                    source.as_ptr(),
                    stride_i32,
                    source[y_size..].as_ptr(),
                    stride_i32,
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
        CaptureFrameFormat::Mjpeg => {
            return convert_mjpeg_to_i420(source, width, height, destination).map(|()| true);
        }
        CaptureFrameFormat::I420 | CaptureFrameFormat::Bgra | CaptureFrameFormat::Uyvy => {
            return Err(V4lError::UnsupportedFrameFormat(source_format));
        }
    };

    if ret == 0 {
        Ok(false)
    } else {
        Err(V4lError::Convert("libyuv conversion failed"))
    }
}

/// Returns the effective source row stride in bytes, falling back to the
/// packed width-derived stride when the driver reports `bytesperline` as zero
/// or smaller than one packed row.
#[cfg(any(target_os = "linux", test))]
fn source_row_stride(reported_stride: u32, packed_stride: usize) -> usize {
    (reported_stride as usize).max(packed_stride)
}

#[cfg(any(target_os = "linux", test))]
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

#[cfg(any(target_os = "linux", test))]
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
        .and_then(|timestamp| u64::try_from(timestamp.as_micros()).ok())
        .and_then(|timestamp_us| validate_capture_timestamp_us(timestamp_us, read_wall_time_us))
        .unwrap_or(fallback_wall_time_us)
}

#[cfg(any(target_os = "linux", test))]
fn i32_from_u32(value: u32, field: &'static str) -> Result<i32, V4lError> {
    i32::try_from(value).map_err(|_| V4lError::OptionOutOfRange(field))
}

#[cfg(any(target_os = "linux", test))]
fn i32_from_usize(value: usize, field: &'static str) -> Result<i32, V4lError> {
    i32::try_from(value).map_err(|_| V4lError::OptionOutOfRange(field))
}

#[cfg(target_os = "linux")]
fn v4l_error(error: std::io::Error) -> V4lError {
    V4lError::Camera(error.to_string())
}

#[cfg(target_os = "linux")]
fn monotonic_to_wallclock(timestamp: v4l::Timestamp) -> Option<Duration> {
    let frame_monotonic = Duration::from(timestamp);
    if frame_monotonic.is_zero() {
        return None;
    }

    let monotonic_now = clock_time(libc::CLOCK_MONOTONIC)?;
    let wall_now = clock_time(libc::CLOCK_REALTIME)?;
    let frame_age = monotonic_now.checked_sub(frame_monotonic)?;
    wall_now.checked_sub(frame_age)
}

#[cfg(target_os = "linux")]
fn clock_time(clock_id: libc::clockid_t) -> Option<Duration> {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    let ret = unsafe {
        // SAFETY: `time` is a valid out pointer and `clock_id` is supplied by libc constants.
        libc::clock_gettime(clock_id, &mut time)
    };
    if ret != 0 || time.tv_sec < 0 || time.tv_nsec < 0 {
        return None;
    }

    Some(Duration::new(time.tv_sec as u64, time.tv_nsec as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::MAX_CAPTURE_TIMESTAMP_AGE_US;

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

    #[test]
    fn accepts_recent_backend_capture_timestamp() {
        let read_us = 20_000_000;
        let recent = Duration::from_micros(read_us - 1_000);
        assert_eq!(select_capture_wall_time_us(Some(recent), 42, read_us), read_us - 1_000);
    }

    #[test]
    fn ignores_backend_capture_timestamp_older_than_max_age() {
        let read_us = 20_000_000;
        let stale = Duration::from_micros(read_us - MAX_CAPTURE_TIMESTAMP_AGE_US - 1);
        assert_eq!(select_capture_wall_time_us(Some(stale), 42, read_us), 42);
    }

    #[test]
    fn frame_rate_from_fraction_rounds_fractional_intervals() {
        assert_eq!(frame_rate_from_fraction(1, 30), Some(30));
        assert_eq!(frame_rate_from_fraction(1001, 30000), Some(30));
        assert_eq!(frame_rate_from_fraction(1001, 60000), Some(60));
        assert_eq!(frame_rate_from_fraction(3, 1), Some(1));
    }

    #[test]
    fn frame_rate_from_fraction_rejects_zero_terms() {
        assert_eq!(frame_rate_from_fraction(0, 30000), None);
        assert_eq!(frame_rate_from_fraction(1001, 0), None);
    }

    #[test]
    fn converts_padded_stride_nv12_frame() {
        let width = 6u32;
        let height = 4u32;
        let stride = 8usize;
        let y_size = stride * height as usize;
        // Padding bytes past each 6-pixel row must never reach the output.
        let mut source = vec![0xEE; y_size + y_size / 2];
        for row in 0..height as usize {
            for col in 0..width as usize {
                source[row * stride + col] = (100 + row * 10 + col) as u8;
            }
        }
        for row in 0..height as usize / 2 {
            for pair in 0..width as usize / 2 {
                source[y_size + row * stride + pair * 2] = (50 + row * 10 + pair) as u8;
                source[y_size + row * stride + pair * 2 + 1] = (150 + row * 10 + pair) as u8;
            }
        }

        let mut destination = I420Buffer::new(width, height);
        let used_decode_path = convert_to_i420(
            CaptureFrameFormat::Nv12,
            &source,
            width,
            height,
            stride as u32,
            &mut destination,
        )
        .expect("padded NV12 frame must convert");
        assert!(!used_decode_path);

        let (stride_y, stride_u, stride_v) = destination.strides();
        let (dst_y, dst_u, dst_v) = destination.data();
        for row in 0..height as usize {
            for col in 0..width as usize {
                assert_eq!(
                    dst_y[row * stride_y as usize + col],
                    (100 + row * 10 + col) as u8,
                    "Y({row},{col})"
                );
            }
        }
        for row in 0..height as usize / 2 {
            for pair in 0..width as usize / 2 {
                assert_eq!(dst_u[row * stride_u as usize + pair], (50 + row * 10 + pair) as u8);
                assert_eq!(dst_v[row * stride_v as usize + pair], (150 + row * 10 + pair) as u8);
            }
        }
    }

    #[test]
    fn converts_padded_stride_yuyv_frame() {
        let width = 6u32;
        let height = 2u32;
        let stride = 16usize;
        // Padding bytes past each 12-byte packed row must never reach the output.
        let mut source = vec![0xEE; stride * height as usize];
        for row in 0..height as usize {
            for col in 0..width as usize {
                source[row * stride + col * 2] = (40 + row * 10 + col) as u8;
                source[row * stride + col * 2 + 1] = 128;
            }
        }

        let mut destination = I420Buffer::new(width, height);
        convert_to_i420(
            CaptureFrameFormat::Yuyv,
            &source,
            width,
            height,
            stride as u32,
            &mut destination,
        )
        .expect("padded YUYV frame must convert");

        let (stride_y, _, _) = destination.strides();
        let (dst_y, _, _) = destination.data();
        for row in 0..height as usize {
            for col in 0..width as usize {
                assert_eq!(
                    dst_y[row * stride_y as usize + col],
                    (40 + row * 10 + col) as u8,
                    "Y({row},{col})"
                );
            }
        }
    }

    #[test]
    fn rejects_nv12_frame_shorter_than_padded_stride_size() {
        let width = 6u32;
        let height = 4u32;
        let packed = vec![0u8; (width * height) as usize * 3 / 2];
        let mut destination = I420Buffer::new(width, height);
        let err =
            convert_to_i420(CaptureFrameFormat::Nv12, &packed, width, height, 8, &mut destination)
                .expect_err("packed-size buffer must fail the stride-aware length check");
        assert!(matches!(err, V4lError::InvalidFrame("NV12 frame")));
    }

    #[test]
    fn falls_back_to_packed_stride_when_driver_reports_zero() {
        let width = 4u32;
        let height = 2u32;
        let y_size = (width * height) as usize;
        let mut source = vec![128u8; y_size + y_size / 2];
        for (index, value) in source.iter_mut().take(y_size).enumerate() {
            *value = index as u8;
        }

        let mut destination = I420Buffer::new(width, height);
        convert_to_i420(CaptureFrameFormat::Nv12, &source, width, height, 0, &mut destination)
            .expect("packed NV12 frame with zero reported stride must convert");

        let (stride_y, _, _) = destination.strides();
        let (dst_y, _, _) = destination.data();
        for row in 0..height as usize {
            for col in 0..width as usize {
                assert_eq!(
                    dst_y[row * stride_y as usize + col],
                    (row * width as usize + col) as u8
                );
            }
        }
    }
}
