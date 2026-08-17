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

//! Linux device capture backend built on V4L2.
//!
//! This module is an implementation detail of [`super::DeviceVideoSource`]:
//! nothing V4L2-specific leaves it. Frames are converted to I420 on the CPU
//! (through libyuv, with an image-crate fallback for MJPEG streams that
//! libyuv rejects).

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use livekit::webrtc::video_frame::{BoxVideoFrame, I420Buffer, VideoFrame, VideoRotation};
use v4l::{
    buffer::{Flags as V4lBufferFlags, Type as V4lBufferType},
    capability::Flags as V4lCapabilityFlags,
    context,
    format::{Format as V4lFormat, FourCC},
    frameinterval::FrameIntervalEnum,
    framesize::FrameSizeEnum,
    io::{mmap::Stream as MmapStream, traits::CaptureStream},
    video::{capture::Parameters as V4lCaptureParameters, Capture},
    Device,
};

use super::timestamp::{
    clock_time, elapsed_us, monotonic_timestamp_to_wallclock, select_capture_wall_time_us,
    unix_time_us_now,
};
use super::{
    capture_frame_metadata, DeviceFormat, DeviceFormatRequest, DeviceFrameFormat, DeviceInfo,
    DeviceSelector, DeviceVideoSourceConfig, DeviceVideoSourceError,
};
use crate::{primitive::VideoResolution, pump::PumpStop};

/// How long the stream's own wait may block. Only the first frame read (which
/// starts the stream) can hit this; later reads are gated on a poll and never
/// wait inside the stream. A stream wait that times out cannot be retried, so
/// a timeout here is a hard [`DeviceVideoSourceError::FrameTimeout`].
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one fd poll may block before the stop token is rechecked, in
/// milliseconds.
const STOP_CHECK_INTERVAL_MS: i32 = 100;

/// Number of memory-mapped buffers shared with the driver.
const BUFFER_COUNT: u32 = 4;

/// Returns whether the backend can convert this source frame format.
fn is_supported_source_format(frame_format: DeviceFrameFormat) -> bool {
    matches!(
        frame_format,
        DeviceFrameFormat::Nv12
            | DeviceFrameFormat::Rgb24
            | DeviceFrameFormat::Bgr24
            | DeviceFrameFormat::Yuyv
            | DeviceFrameFormat::Grey
            | DeviceFrameFormat::Mjpeg
    )
}

/// Default ordered source frame formats to try, most preferred first.
fn default_frame_formats() -> Vec<DeviceFrameFormat> {
    vec![
        DeviceFrameFormat::Yuyv,
        DeviceFrameFormat::Mjpeg,
        DeviceFrameFormat::Grey,
        DeviceFrameFormat::Rgb24,
        DeviceFrameFormat::Nv12,
    ]
}

/// V4L2 capture session satisfying the backend contract.
pub(super) struct Session {
    device: Device,
    stream: MmapStream<'static>,
    format: DeviceFormat,
    // Driver-reported row stride in bytes (V4L2 `bytesperline`).
    stride: u32,
    started_at: Instant,
    // Frame pulled while starting the stream, handed out first.
    pending_frame: Option<BoxVideoFrame>,
}

impl Session {
    /// Opens the device, negotiates the capture format, and starts the
    /// stream by pulling its first frame.
    pub(super) fn open(config: &DeviceVideoSourceConfig) -> Result<Self, DeviceVideoSourceError> {
        super::validate_config(config, is_supported_source_format)?;

        let frame_formats = frame_formats_for_request(&config.format);
        let device = open_device(&config.device)?;
        let device_name = device
            .query_caps()
            .ok()
            .map(|caps| caps.card)
            .filter(|card| !card.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let all_formats = enumerate_device_formats(&device)?;
        let (format, stride) =
            apply_format_request(&device, &config.format, &frame_formats, &all_formats)?;
        let mut stream =
            MmapStream::with_buffers(&device, V4lBufferType::VideoCapture, BUFFER_COUNT)
                .map_err(backend_error)?;
        stream.set_timeout(FIRST_FRAME_TIMEOUT);

        let mut session = Self {
            device,
            stream,
            format,
            stride,
            started_at: Instant::now(),
            pending_frame: None,
        };
        // Pull the first frame during construction: it queues the stream's
        // buffers and starts streaming, so every later wait can be
        // poll-bounded to observe the stop token, and it proves the
        // negotiated format actually delivers frames.
        let first_frame = session.read_frame()?;
        session.pending_frame = Some(first_frame);
        log::info!("Opened device \"{}\": {} (converted to I420)", device_name, session.format,);
        Ok(session)
    }

    /// Returns the negotiated capture format.
    pub(super) fn format(&self) -> DeviceFormat {
        self.format
    }

    /// Blocks until the next frame is available, returning `Ok(None)` once
    /// the stop token fires.
    pub(super) fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
        if let Some(frame) = self.pending_frame.take() {
            return Ok(Some(frame));
        }

        // Bounded fd polls keep the stop token observed within
        // STOP_CHECK_INTERVAL_MS even when the device stalls. The stream is
        // only read once the fd signals, so the stream's own wait — which
        // cannot be resumed after a timeout — never blocks here.
        loop {
            if stop.is_stopped() {
                return Ok(None);
            }
            match self.device.handle().poll(libc::POLLIN, STOP_CHECK_INTERVAL_MS) {
                Ok(0) => continue,
                // Readable, or an error condition the stream read surfaces.
                Ok(_) => break,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(backend_error(err)),
            }
        }
        self.read_frame().map(Some)
    }

    /// Dequeues one frame from the stream and converts it to I420.
    fn read_frame(&mut self) -> Result<BoxVideoFrame, DeviceVideoSourceError> {
        let fallback_wall_time_us = unix_time_us_now().unwrap_or_default();
        let format = self.format;
        let stride = self.stride;
        let (buffer, metadata) = self.stream.next().map_err(|err| match err.kind() {
            io::ErrorKind::TimedOut => DeviceVideoSourceError::FrameTimeout,
            _ => backend_error(err),
        })?;
        let timestamp_us = elapsed_us(self.started_at.elapsed());
        let read_wall_time_us = unix_time_us_now().unwrap_or(fallback_wall_time_us);
        let backend_capture_timestamp =
            v4l_timestamp_to_wallclock(metadata.timestamp, v4l_timestamp_clock(metadata.flags));
        let capture_wall_time_us = select_capture_wall_time_us(
            backend_capture_timestamp,
            fallback_wall_time_us,
            read_wall_time_us,
        );

        let width = format.resolution.width;
        let height = format.resolution.height;
        let mut i420 = I420Buffer::new(width, height);
        let source = frame_bytes(buffer, metadata.bytesused);
        convert_to_i420(format.frame_format, source, width, height, stride, &mut i420)?;

        Ok(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            frame_metadata: Some(capture_frame_metadata(capture_wall_time_us)),
            buffer: Box::new(i420),
        })
    }
}

/// Lists Linux V4L2 capture devices.
pub(super) fn devices() -> Result<Vec<DeviceInfo>, DeviceVideoSourceError> {
    let devices = context::enum_devices()
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
            }

            Some(DeviceInfo { id, name, model_id, manufacturer, formats, formats_complete })
        })
        .collect();

    Ok(devices)
}

fn open_device(selector: &DeviceSelector) -> Result<Device, DeviceVideoSourceError> {
    match selector {
        DeviceSelector::Default => Device::new(0).map_err(open_error),
        DeviceSelector::Index(index) => Device::new(*index).map_err(open_error),
        DeviceSelector::Id(id) => open_device_id(id),
    }
}

fn open_device_id(id: &str) -> Result<Device, DeviceVideoSourceError> {
    if let Ok(index) = id.parse::<usize>() {
        return Device::new(index).map_err(open_error);
    }

    Device::with_path(Path::new(id)).map_err(open_error)
}

fn open_error(error: io::Error) -> DeviceVideoSourceError {
    if error.kind() == io::ErrorKind::NotFound {
        DeviceVideoSourceError::DeviceNotFound
    } else {
        backend_error(error)
    }
}

fn backend_error(error: io::Error) -> DeviceVideoSourceError {
    DeviceVideoSourceError::Backend(error.to_string())
}

/// Returns the ordered source frame formats to try for a request.
///
/// The request's own frame format (already validated as supported) is tried
/// first; an explicit constraint on the highest-* requests pins the list to
/// that one format.
fn frame_formats_for_request(request: &DeviceFormatRequest) -> Vec<DeviceFrameFormat> {
    let mut formats = match request {
        DeviceFormatRequest::Exact(format) | DeviceFormatRequest::Closest(format) => {
            ordered_formats_with_first(&default_frame_formats(), format.frame_format)
        }
        DeviceFormatRequest::HighestFramerate { frame_format: Some(frame_format), .. }
        | DeviceFormatRequest::HighestResolution { frame_format: Some(frame_format), .. } => {
            vec![*frame_format]
        }
        DeviceFormatRequest::Default
        | DeviceFormatRequest::HighestFramerate { frame_format: None, .. }
        | DeviceFormatRequest::HighestResolution { frame_format: None, .. } => {
            default_frame_formats()
        }
    };
    formats.dedup();
    formats
}

fn ordered_formats_with_first(
    frame_formats: &[DeviceFrameFormat],
    first: DeviceFrameFormat,
) -> Vec<DeviceFrameFormat> {
    std::iter::once(first)
        .chain(frame_formats.iter().copied().filter(|format| *format != first))
        .collect()
}

fn apply_format_request(
    device: &Device,
    request: &DeviceFormatRequest,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Result<(DeviceFormat, u32), DeviceVideoSourceError> {
    match request {
        DeviceFormatRequest::Default
        | DeviceFormatRequest::HighestFramerate { .. }
        | DeviceFormatRequest::HighestResolution { .. } => {
            let selected = select_format_for_request(request, frame_formats, all_formats)?;
            set_device_format(device, selected)
        }
        DeviceFormatRequest::Exact(_) | DeviceFormatRequest::Closest(_) => {
            apply_ordered_format_request(device, request, frame_formats, all_formats)
        }
    }
}

/// Tries the request once per candidate source frame format, in preference
/// order, returning the first format the device accepts.
fn apply_ordered_format_request(
    device: &Device,
    request: &DeviceFormatRequest,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Result<(DeviceFormat, u32), DeviceVideoSourceError> {
    let mut last_error = None;
    for frame_format in frame_formats {
        let request = format_request_with_frame_format(request, *frame_format);
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

    Err(last_error
        .unwrap_or(DeviceVideoSourceError::InvalidConfig("no source frame formats to request")))
}

fn format_request_with_frame_format(
    request: &DeviceFormatRequest,
    frame_format: DeviceFrameFormat,
) -> DeviceFormatRequest {
    match request {
        DeviceFormatRequest::Exact(format) => DeviceFormatRequest::Exact(DeviceFormat::new(
            format.resolution,
            format.framerate_fps,
            frame_format,
        )),
        DeviceFormatRequest::Closest(format) => DeviceFormatRequest::Closest(DeviceFormat::new(
            format.resolution,
            format.framerate_fps,
            frame_format,
        )),
        DeviceFormatRequest::Default => DeviceFormatRequest::Default,
        DeviceFormatRequest::HighestFramerate { resolution, .. } => {
            DeviceFormatRequest::HighestFramerate {
                resolution: *resolution,
                frame_format: Some(frame_format),
            }
        }
        DeviceFormatRequest::HighestResolution { framerate_fps, .. } => {
            DeviceFormatRequest::HighestResolution {
                framerate_fps: *framerate_fps,
                frame_format: Some(frame_format),
            }
        }
    }
}

fn select_format_for_request(
    request: &DeviceFormatRequest,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Result<DeviceFormat, DeviceVideoSourceError> {
    let selected = match request {
        DeviceFormatRequest::Default => {
            all_formats.iter().find(|format| frame_formats.contains(&format.frame_format)).copied()
        }
        DeviceFormatRequest::Exact(format) => {
            if frame_formats.contains(&format.frame_format) {
                Some(*format)
            } else {
                None
            }
        }
        DeviceFormatRequest::Closest(format) => {
            select_closest_format(*format, frame_formats, all_formats)
        }
        DeviceFormatRequest::HighestFramerate { .. } => {
            select_highest_framerate_format(request, frame_formats, all_formats)
        }
        DeviceFormatRequest::HighestResolution { .. } => {
            select_highest_resolution_format(request, frame_formats, all_formats)
        }
    };

    selected.ok_or_else(|| match request {
        DeviceFormatRequest::Exact(format) | DeviceFormatRequest::Closest(format) => {
            DeviceVideoSourceError::UnsupportedFormat(*format)
        }
        _ => DeviceVideoSourceError::Backend(
            "no device format satisfies the format request".to_string(),
        ),
    })
}

fn select_closest_format(
    requested: DeviceFormat,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Option<DeviceFormat> {
    if !frame_formats.contains(&requested.frame_format) {
        return None;
    }

    let resolution = all_formats
        .iter()
        .copied()
        .filter(|format| format.frame_format == requested.frame_format)
        .min_by_key(|format| resolution_distance(format.resolution, requested.resolution))?
        .resolution;

    let framerate_fps = all_formats
        .iter()
        .copied()
        .filter(|format| {
            format.frame_format == requested.frame_format && format.resolution == resolution
        })
        .min_by_key(|format| format.framerate_fps.abs_diff(requested.framerate_fps))?
        .framerate_fps;

    Some(DeviceFormat::new(resolution, framerate_fps, requested.frame_format))
}

fn select_highest_framerate_format(
    request: &DeviceFormatRequest,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Option<DeviceFormat> {
    all_formats
        .iter()
        .copied()
        .filter(|format| frame_formats.contains(&format.frame_format))
        .filter(|format| match request {
            DeviceFormatRequest::HighestFramerate { resolution, frame_format } => {
                resolution.map(|resolution| format.resolution == resolution).unwrap_or(true)
                    && frame_format
                        .map(|frame_format| format.frame_format == frame_format)
                        .unwrap_or(true)
            }
            _ => false,
        })
        .max_by(|left, right| {
            left.framerate_fps
                .cmp(&right.framerate_fps)
                .then_with(|| compare_resolution(left.resolution, right.resolution))
                .then_with(|| {
                    compare_format_preference(left.frame_format, right.frame_format, frame_formats)
                })
        })
}

fn select_highest_resolution_format(
    request: &DeviceFormatRequest,
    frame_formats: &[DeviceFrameFormat],
    all_formats: &[DeviceFormat],
) -> Option<DeviceFormat> {
    all_formats
        .iter()
        .copied()
        .filter(|format| frame_formats.contains(&format.frame_format))
        .filter(|format| match request {
            DeviceFormatRequest::HighestResolution { framerate_fps, frame_format } => {
                framerate_fps
                    .map(|framerate_fps| format.framerate_fps == framerate_fps)
                    .unwrap_or(true)
                    && frame_format
                        .map(|frame_format| format.frame_format == frame_format)
                        .unwrap_or(true)
            }
            _ => false,
        })
        .max_by(|left, right| {
            compare_resolution(left.resolution, right.resolution)
                .then_with(|| left.framerate_fps.cmp(&right.framerate_fps))
                .then_with(|| {
                    compare_format_preference(left.frame_format, right.frame_format, frame_formats)
                })
        })
}

fn compare_resolution(left: VideoResolution, right: VideoResolution) -> std::cmp::Ordering {
    frame_area(left)
        .cmp(&frame_area(right))
        .then_with(|| left.width.cmp(&right.width))
        .then_with(|| left.height.cmp(&right.height))
}

fn resolution_distance(left: VideoResolution, right: VideoResolution) -> u64 {
    let width = i64::from(left.width) - i64::from(right.width);
    let height = i64::from(left.height) - i64::from(right.height);
    width.unsigned_abs().pow(2) + height.unsigned_abs().pow(2)
}

fn frame_area(resolution: VideoResolution) -> u64 {
    u64::from(resolution.width) * u64::from(resolution.height)
}

fn compare_format_preference(
    left: DeviceFrameFormat,
    right: DeviceFrameFormat,
    frame_formats: &[DeviceFrameFormat],
) -> std::cmp::Ordering {
    let left_index = frame_formats.iter().position(|format| *format == left).unwrap_or(usize::MAX);
    let right_index =
        frame_formats.iter().position(|format| *format == right).unwrap_or(usize::MAX);
    right_index.cmp(&left_index)
}

fn set_device_format(
    device: &Device,
    selected: DeviceFormat,
) -> Result<(DeviceFormat, u32), DeviceVideoSourceError> {
    let (current, _) = device_capture_format(device)?;
    let format_changed =
        current.resolution != selected.resolution || current.frame_format != selected.frame_format;
    if format_changed {
        device
            .set_format(&V4lFormat::new(
                selected.resolution.width,
                selected.resolution.height,
                fourcc_for_frame_format(selected.frame_format)
                    .ok_or(DeviceVideoSourceError::UnsupportedFrameFormat(selected.frame_format))?,
            ))
            .map_err(backend_error)?;
    }
    if format_changed || current.framerate_fps != selected.framerate_fps {
        device
            .set_params(&V4lCaptureParameters::with_fps(selected.framerate_fps))
            .map_err(backend_error)?;
    }

    let (actual, stride) = device_capture_format(device)?;
    if actual != selected {
        return Err(DeviceVideoSourceError::Backend(format!(
            "device rejected capture format: requested {selected}, got {actual}"
        )));
    }
    Ok((actual, stride))
}

/// Returns the device's current capture format and its row stride in bytes
/// (V4L2 `bytesperline`).
fn device_capture_format(device: &Device) -> Result<(DeviceFormat, u32), DeviceVideoSourceError> {
    let format = device.format().map_err(backend_error)?;
    let params = device.params().map_err(backend_error)?;
    let framerate_fps =
        framerate_from_fraction(params.interval.numerator, params.interval.denominator).ok_or(
            DeviceVideoSourceError::Backend("device reports a zero frame interval".to_string()),
        )?;
    let capture_format = DeviceFormat::new(
        VideoResolution::new(format.width, format.height),
        framerate_fps,
        frame_format_from_fourcc(format.fourcc).ok_or_else(|| {
            DeviceVideoSourceError::Backend(format!("unsupported fourcc {}", format.fourcc))
        })?,
    );
    Ok((capture_format, format.stride))
}

fn enumerate_device_formats(device: &Device) -> Result<Vec<DeviceFormat>, DeviceVideoSourceError> {
    let mut formats = Vec::new();
    let mut seen_fourccs = Vec::new();

    for description in device.enum_formats().map_err(backend_error)? {
        let fourcc = description.fourcc;
        let Some(frame_format) = frame_format_from_fourcc(fourcc) else {
            continue;
        };
        if seen_fourccs.contains(&fourcc) {
            continue;
        }
        seen_fourccs.push(fourcc);
        let frame_sizes = device.enum_framesizes(fourcc).map_err(backend_error)?;
        for resolution in frame_sizes.into_iter().flat_map(resolutions_from_frame_size) {
            let intervals = device
                .enum_frameintervals(fourcc, resolution.width, resolution.height)
                .unwrap_or_default();
            for framerate_fps in intervals.into_iter().flat_map(framerates_from_interval) {
                formats.push(DeviceFormat::new(resolution, framerate_fps, frame_format));
            }
        }
    }

    Ok(formats)
}

fn fourcc_for_frame_format(frame_format: DeviceFrameFormat) -> Option<FourCC> {
    match frame_format {
        DeviceFrameFormat::Nv12 => Some(FourCC::new(b"NV12")),
        DeviceFrameFormat::Rgb24 => Some(FourCC::new(b"RGB3")),
        DeviceFrameFormat::Bgr24 => Some(FourCC::new(b"BGR3")),
        DeviceFrameFormat::Yuyv => Some(FourCC::new(b"YUYV")),
        DeviceFrameFormat::Grey => Some(FourCC::new(b"GREY")),
        DeviceFrameFormat::Mjpeg => Some(FourCC::new(b"MJPG")),
        DeviceFrameFormat::I420 | DeviceFrameFormat::Bgra | DeviceFrameFormat::Uyvy => None,
    }
}

fn frame_format_from_fourcc(fourcc: FourCC) -> Option<DeviceFrameFormat> {
    match fourcc.str().ok()? {
        "NV12" => Some(DeviceFrameFormat::Nv12),
        "RGB3" => Some(DeviceFrameFormat::Rgb24),
        "BGR3" => Some(DeviceFrameFormat::Bgr24),
        "YUYV" | "YUY2" => Some(DeviceFrameFormat::Yuyv),
        "GREY" => Some(DeviceFrameFormat::Grey),
        "MJPG" | "JPEG" => Some(DeviceFrameFormat::Mjpeg),
        _ => None,
    }
}

fn resolutions_from_frame_size(size: v4l::FrameSize) -> Vec<VideoResolution> {
    match size.size {
        FrameSizeEnum::Discrete(discrete) => {
            vec![VideoResolution::new(discrete.width, discrete.height)]
        }
        FrameSizeEnum::Stepwise(stepwise) => {
            let mut resolutions = Vec::new();
            push_stepwise_resolution(
                &mut resolutions,
                VideoResolution::new(stepwise.min_width, stepwise.min_height),
            );
            push_stepwise_resolution(
                &mut resolutions,
                VideoResolution::new(stepwise.max_width, stepwise.max_height),
            );
            resolutions
        }
    }
}

fn push_stepwise_resolution(resolutions: &mut Vec<VideoResolution>, resolution: VideoResolution) {
    if resolution.width != 0 && resolution.height != 0 && !resolutions.contains(&resolution) {
        resolutions.push(resolution);
    }
}

fn framerates_from_interval(interval: v4l::FrameInterval) -> Vec<u32> {
    match interval.interval {
        FrameIntervalEnum::Discrete(fraction) => {
            framerate_from_fraction(fraction.numerator, fraction.denominator).into_iter().collect()
        }
        FrameIntervalEnum::Stepwise(stepwise) => {
            let mut framerates = Vec::new();
            for fraction in [stepwise.min, stepwise.max] {
                if let Some(framerate) =
                    framerate_from_fraction(fraction.numerator, fraction.denominator)
                {
                    if !framerates.contains(&framerate) {
                        framerates.push(framerate);
                    }
                }
            }
            framerates
        }
    }
}

/// Converts a V4L2 frame interval (seconds per frame) to frames per second.
///
/// Non-integer rates (e.g. the NTSC interval 1001/30000 = 29.97fps) round to
/// the nearest whole rate, never below 1.
fn framerate_from_fraction(numerator: u32, denominator: u32) -> Option<u32> {
    if numerator == 0 || denominator == 0 {
        return None;
    }
    if denominator % numerator == 0 {
        return Some(denominator / numerator);
    }
    let rounded = (u64::from(denominator) + u64::from(numerator) / 2) / u64::from(numerator);
    Some(u32::try_from(rounded).unwrap_or(u32::MAX).max(1))
}

fn frame_bytes(buffer: &[u8], bytes_used: u32) -> &[u8] {
    let bytes_used = usize::try_from(bytes_used).unwrap_or(buffer.len()).min(buffer.len());
    if bytes_used == 0 {
        buffer
    } else {
        &buffer[..bytes_used]
    }
}

fn convert_to_i420(
    source_format: DeviceFrameFormat,
    source: &[u8],
    width: u32,
    height: u32,
    source_stride: u32,
    destination: &mut I420Buffer,
) -> Result<(), DeviceVideoSourceError> {
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (dst_y, dst_u, dst_v) = destination.data_mut();
    let width_i32 = i32_from_u32(width, "width exceeds supported range")?;
    let height_i32 = i32_from_u32(height, "height exceeds supported range")?;

    let ret = match source_format {
        DeviceFrameFormat::Yuyv => {
            let stride = source_row_stride(source_stride, width as usize * 2);
            validate_len(source, stride * height as usize, "YUYV frame is too short")?;
            let stride_i32 = i32_from_usize(stride, "stride exceeds supported range")?;
            // SAFETY: Source and destination slices are valid for the dimensions and strides.
            unsafe {
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
        DeviceFrameFormat::Rgb24 => {
            let stride = source_row_stride(source_stride, width as usize * 3);
            validate_len(source, stride * height as usize, "RGB24 frame is too short")?;
            let stride_i32 = i32_from_usize(stride, "stride exceeds supported range")?;
            // SAFETY: Source and destination slices are valid for the dimensions and strides.
            unsafe {
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
        DeviceFrameFormat::Bgr24 => {
            let stride = source_row_stride(source_stride, width as usize * 3);
            validate_len(source, stride * height as usize, "BGR24 frame is too short")?;
            let stride_i32 = i32_from_usize(stride, "stride exceeds supported range")?;
            // SAFETY: Source and destination slices are valid for the dimensions and strides.
            unsafe {
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
        DeviceFrameFormat::Grey => {
            let stride = source_row_stride(source_stride, width as usize);
            validate_len(source, stride * height as usize, "GREY frame is too short")?;
            let stride_i32 = i32_from_usize(stride, "stride exceeds supported range")?;
            // SAFETY: Source and destination slices are valid for the dimensions and strides.
            unsafe {
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
        DeviceFrameFormat::Nv12 => {
            // Single-planar V4L2 NV12: the interleaved chroma plane follows the
            // luma plane at `stride * height` and shares the luma stride.
            let stride = source_row_stride(source_stride, width as usize);
            let y_size = stride * height as usize;
            validate_len(source, y_size + y_size / 2, "NV12 frame is too short")?;
            let stride_i32 = i32_from_usize(stride, "stride exceeds supported range")?;
            // SAFETY: Source and destination slices are valid for the dimensions and strides.
            unsafe {
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
        DeviceFrameFormat::Mjpeg => {
            return convert_mjpeg_to_i420(source, width, height, destination);
        }
        DeviceFrameFormat::I420 | DeviceFrameFormat::Bgra | DeviceFrameFormat::Uyvy => {
            return Err(DeviceVideoSourceError::UnsupportedFrameFormat(source_format));
        }
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(DeviceVideoSourceError::Convert("libyuv conversion failed"))
    }
}

/// Returns the effective source row stride in bytes, falling back to the
/// packed width-derived stride when the driver reports `bytesperline` as zero
/// or smaller than one packed row.
fn source_row_stride(reported_stride: u32, packed_stride: usize) -> usize {
    (reported_stride as usize).max(packed_stride)
}

fn convert_mjpeg_to_i420(
    source: &[u8],
    width: u32,
    height: u32,
    destination: &mut I420Buffer,
) -> Result<(), DeviceVideoSourceError> {
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (dst_y, dst_u, dst_v) = destination.data_mut();
    let width_i32 = i32_from_u32(width, "width exceeds supported range")?;
    let height_i32 = i32_from_u32(height, "height exceeds supported range")?;

    // SAFETY: Source and destination slices are valid for the dimensions and strides.
    let ret = unsafe {
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
        .map_err(|error| DeviceVideoSourceError::Decode(error.to_string()))?
        .to_rgb8();
    if rgb.width() != width || rgb.height() != height {
        return Err(DeviceVideoSourceError::InvalidFrame("decoded MJPEG dimensions changed"));
    }
    // SAFETY: Source and destination slices are valid for the dimensions and strides.
    let ret = unsafe {
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
        Err(DeviceVideoSourceError::Convert("RGB24 fallback conversion failed"))
    }
}

fn validate_len(
    source: &[u8],
    expected: usize,
    label: &'static str,
) -> Result<(), DeviceVideoSourceError> {
    if source.len() < expected {
        return Err(DeviceVideoSourceError::InvalidFrame(label));
    }
    Ok(())
}

fn i32_from_u32(value: u32, label: &'static str) -> Result<i32, DeviceVideoSourceError> {
    i32::try_from(value).map_err(|_| DeviceVideoSourceError::InvalidFrame(label))
}

fn i32_from_usize(value: usize, label: &'static str) -> Result<i32, DeviceVideoSourceError> {
    i32::try_from(value).map_err(|_| DeviceVideoSourceError::InvalidFrame(label))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V4lTimestampClock {
    Unknown,
    Monotonic,
    Copy,
    Unsupported,
}

fn v4l_timestamp_clock(flags: V4lBufferFlags) -> V4lTimestampClock {
    let timestamp_type = flags.bits() & V4lBufferFlags::TIMESTAMP_MASK.bits();
    if timestamp_type == V4lBufferFlags::TIMESTAMP_MONOTONIC.bits() {
        V4lTimestampClock::Monotonic
    } else if timestamp_type == V4lBufferFlags::TIMESTAMP_COPY.bits() {
        V4lTimestampClock::Copy
    } else if timestamp_type == V4lBufferFlags::TIMESTAMP_UNKNOWN.bits() {
        V4lTimestampClock::Unknown
    } else {
        V4lTimestampClock::Unsupported
    }
}

fn v4l_timestamp_to_wallclock(
    timestamp: v4l::Timestamp,
    clock: V4lTimestampClock,
) -> Option<Duration> {
    let frame_timestamp = Duration::from(timestamp);
    if frame_timestamp.is_zero() {
        return None;
    }

    let monotonic_now = clock_time(libc::CLOCK_MONOTONIC)?;
    let wall_now = clock_time(libc::CLOCK_REALTIME)?;
    timestamp_to_wallclock(frame_timestamp, clock, monotonic_now, wall_now)
}

fn timestamp_to_wallclock(
    frame_timestamp: Duration,
    clock: V4lTimestampClock,
    monotonic_now: Duration,
    wall_now: Duration,
) -> Option<Duration> {
    if frame_timestamp.is_zero() {
        return None;
    }

    match clock {
        V4lTimestampClock::Monotonic => {
            monotonic_timestamp_to_wallclock(frame_timestamp, monotonic_now, wall_now)
        }
        V4lTimestampClock::Unknown => {
            monotonic_timestamp_to_wallclock(frame_timestamp, monotonic_now, wall_now)
                .or(Some(frame_timestamp))
        }
        V4lTimestampClock::Copy | V4lTimestampClock::Unsupported => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::device::timestamp::MAX_CAPTURE_TIMESTAMP_AGE_US;

    #[test]
    fn source_formats_exclude_unconvertible_ones() {
        assert!(!is_supported_source_format(DeviceFrameFormat::I420));
        assert!(!is_supported_source_format(DeviceFrameFormat::Bgra));
        assert!(!is_supported_source_format(DeviceFrameFormat::Uyvy));
        assert!(is_supported_source_format(DeviceFrameFormat::Yuyv));
    }

    #[test]
    fn frame_formats_for_request_prefers_the_requested_format() {
        let request = DeviceFormatRequest::Exact(DeviceFormat::new(
            VideoResolution::new(1280, 720),
            30,
            DeviceFrameFormat::Mjpeg,
        ));
        let formats = frame_formats_for_request(&request);
        assert_eq!(formats.first(), Some(&DeviceFrameFormat::Mjpeg));
        assert_eq!(formats.len(), default_frame_formats().len());
    }

    #[test]
    fn frame_formats_for_request_pins_explicit_constraints() {
        let request = DeviceFormatRequest::HighestFramerate {
            resolution: None,
            frame_format: Some(DeviceFrameFormat::Grey),
        };
        assert_eq!(frame_formats_for_request(&request), vec![DeviceFrameFormat::Grey]);
    }

    #[test]
    fn ignores_stream_relative_capture_timestamp() {
        // A small timestamp (relative to stream start rather than a clock)
        // fails wall-clock validation and falls back to the read time.
        let selected =
            select_capture_wall_time_us(Some(Duration::from_micros(5)), 9_000_000, 10_000_000);
        assert_eq!(selected, 9_000_000);
    }

    #[test]
    fn accepts_recent_backend_capture_timestamp() {
        let selected = select_capture_wall_time_us(
            Some(Duration::from_micros(9_999_000)),
            9_000_000,
            10_000_000,
        );
        assert_eq!(selected, 9_999_000);
    }

    #[test]
    fn ignores_backend_capture_timestamp_older_than_max_age() {
        let read_wall_time_us = 10_000_000 + MAX_CAPTURE_TIMESTAMP_AGE_US;
        let selected = select_capture_wall_time_us(
            Some(Duration::from_micros(10_000_000 - 1)),
            9_000_000,
            read_wall_time_us,
        );
        assert_eq!(selected, 9_000_000);
    }

    #[test]
    fn converts_monotonic_v4l_timestamp_to_wallclock() {
        let converted = timestamp_to_wallclock(
            Duration::from_secs(90),
            V4lTimestampClock::Monotonic,
            Duration::from_secs(100),
            Duration::from_secs(1_000),
        );
        assert_eq!(converted, Some(Duration::from_secs(990)));
    }

    #[test]
    fn infers_unknown_v4l_timestamp_clock() {
        // Convertible as monotonic: treated as monotonic.
        let converted = timestamp_to_wallclock(
            Duration::from_secs(90),
            V4lTimestampClock::Unknown,
            Duration::from_secs(100),
            Duration::from_secs(1_000),
        );
        assert_eq!(converted, Some(Duration::from_secs(990)));

        // Ahead of the monotonic clock: passed through as-is.
        let converted = timestamp_to_wallclock(
            Duration::from_secs(500),
            V4lTimestampClock::Unknown,
            Duration::from_secs(100),
            Duration::from_secs(1_000),
        );
        assert_eq!(converted, Some(Duration::from_secs(500)));
    }

    #[test]
    fn rejects_copied_and_unsupported_v4l_timestamps() {
        for clock in [V4lTimestampClock::Copy, V4lTimestampClock::Unsupported] {
            let converted = timestamp_to_wallclock(
                Duration::from_secs(90),
                clock,
                Duration::from_secs(100),
                Duration::from_secs(1_000),
            );
            assert_eq!(converted, None);
        }
    }

    #[test]
    fn framerate_from_fraction_rounds_fractional_intervals() {
        assert_eq!(framerate_from_fraction(1, 30), Some(30));
        assert_eq!(framerate_from_fraction(1001, 30000), Some(30));
        assert_eq!(framerate_from_fraction(1001, 60000), Some(60));
    }

    #[test]
    fn framerate_from_fraction_rejects_zero_terms() {
        assert_eq!(framerate_from_fraction(0, 30), None);
        assert_eq!(framerate_from_fraction(30, 0), None);
    }
}
