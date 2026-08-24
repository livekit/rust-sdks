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

use crate::proto;
use livekit_capture::{encoded::EncodedVideoCodec, primitive::VideoResolution};

#[cfg(any(
    feature = "capture-device",
    feature = "capture-gstreamer",
    feature = "capture-pattern",
    feature = "capture-rtsp"
))]
use crate::{FfiError, FfiResult};
#[cfg(feature = "capture-clock")]
use livekit_capture::sources::clock::ClockVideoSourceConfig;
#[cfg(feature = "capture-device")]
use livekit_capture::sources::device::{
    DeviceFormat, DeviceFormatRequest, DeviceFrameFormat, DeviceInfo, DeviceSelector,
    DeviceVideoSourceConfig,
};
#[cfg(feature = "capture-gstreamer")]
use livekit_capture::sources::gstreamer::{
    GStreamerBitrateUnit, GStreamerRateControlConfig, GStreamerVideoSourceConfig,
};
#[cfg(feature = "capture-pattern")]
use livekit_capture::sources::pattern::{Pattern, PatternVideoSourceConfig};
#[cfg(feature = "capture-rtsp")]
use livekit_capture::sources::rtsp::RtspVideoSourceConfig;

impl From<proto::VideoSourceResolution> for VideoResolution {
    fn from(resolution: proto::VideoSourceResolution) -> Self {
        Self::new(resolution.width, resolution.height)
    }
}

#[cfg(feature = "capture-clock")]
impl From<proto::ClockVideoSourceConfig> for ClockVideoSourceConfig {
    fn from(config: proto::ClockVideoSourceConfig) -> Self {
        Self { resolution: config.resolution.into(), framerate_fps: config.framerate_fps }
    }
}

#[cfg(feature = "capture-pattern")]
pub fn pattern_config_from_proto(
    config: proto::PatternVideoSourceConfig,
) -> FfiResult<PatternVideoSourceConfig> {
    let pattern = proto::Pattern::try_from(config.pattern)
        .map_err(|_| FfiError::InvalidRequest("invalid pattern".into()))?;
    Ok(PatternVideoSourceConfig {
        resolution: config.resolution.into(),
        framerate_fps: config.framerate_fps,
        pattern: match pattern {
            proto::Pattern::Gradient => Pattern::Gradient,
            proto::Pattern::Logo => Pattern::Logo,
        },
    })
}

#[cfg(feature = "capture-gstreamer")]
impl From<proto::GstreamerBitrateUnit> for GStreamerBitrateUnit {
    fn from(unit: proto::GstreamerBitrateUnit) -> Self {
        match unit {
            proto::GstreamerBitrateUnit::Bps => Self::BitsPerSecond,
            proto::GstreamerBitrateUnit::Kbps => Self::KilobitsPerSecond,
        }
    }
}

#[cfg(any(feature = "capture-gstreamer", feature = "capture-rtsp"))]
pub fn video_codec_from_proto(codec: proto::VideoCodec) -> EncodedVideoCodec {
    match codec {
        proto::VideoCodec::H264 => EncodedVideoCodec::H264,
        proto::VideoCodec::H265 => EncodedVideoCodec::H265,
        proto::VideoCodec::Vp8 => EncodedVideoCodec::VP8,
        proto::VideoCodec::Vp9 => EncodedVideoCodec::VP9,
        proto::VideoCodec::Av1 => EncodedVideoCodec::AV1,
    }
}

pub fn video_codec_to_proto(codec: EncodedVideoCodec) -> Option<proto::VideoCodec> {
    match codec {
        EncodedVideoCodec::H264 => Some(proto::VideoCodec::H264),
        EncodedVideoCodec::H265 => Some(proto::VideoCodec::H265),
        EncodedVideoCodec::VP8 => Some(proto::VideoCodec::Vp8),
        EncodedVideoCodec::VP9 => Some(proto::VideoCodec::Vp9),
        EncodedVideoCodec::AV1 => Some(proto::VideoCodec::Av1),
        // The codec enum is non-exhaustive; codecs unknown to the protocol
        // are simply not reported.
        _ => None,
    }
}

#[cfg(feature = "capture-device")]
fn device_frame_format_from_proto(format: proto::DeviceFrameFormat) -> DeviceFrameFormat {
    match format {
        proto::DeviceFrameFormat::I420 => DeviceFrameFormat::I420,
        proto::DeviceFrameFormat::Nv12 => DeviceFrameFormat::Nv12,
        proto::DeviceFrameFormat::Bgra => DeviceFrameFormat::Bgra,
        proto::DeviceFrameFormat::Rgb24 => DeviceFrameFormat::Rgb24,
        proto::DeviceFrameFormat::Bgr24 => DeviceFrameFormat::Bgr24,
        proto::DeviceFrameFormat::Yuyv => DeviceFrameFormat::Yuyv,
        proto::DeviceFrameFormat::Uyvy => DeviceFrameFormat::Uyvy,
        proto::DeviceFrameFormat::Grey => DeviceFrameFormat::Grey,
        proto::DeviceFrameFormat::Mjpeg => DeviceFrameFormat::Mjpeg,
    }
}

#[cfg(feature = "capture-device")]
fn device_frame_format_to_proto(format: DeviceFrameFormat) -> Option<proto::DeviceFrameFormat> {
    match format {
        DeviceFrameFormat::I420 => Some(proto::DeviceFrameFormat::I420),
        DeviceFrameFormat::Nv12 => Some(proto::DeviceFrameFormat::Nv12),
        DeviceFrameFormat::Bgra => Some(proto::DeviceFrameFormat::Bgra),
        DeviceFrameFormat::Rgb24 => Some(proto::DeviceFrameFormat::Rgb24),
        DeviceFrameFormat::Bgr24 => Some(proto::DeviceFrameFormat::Bgr24),
        DeviceFrameFormat::Yuyv => Some(proto::DeviceFrameFormat::Yuyv),
        DeviceFrameFormat::Uyvy => Some(proto::DeviceFrameFormat::Uyvy),
        DeviceFrameFormat::Grey => Some(proto::DeviceFrameFormat::Grey),
        DeviceFrameFormat::Mjpeg => Some(proto::DeviceFrameFormat::Mjpeg),
        // The frame format enum is non-exhaustive; formats unknown to the
        // protocol are simply not reported.
        _ => None,
    }
}

#[cfg(feature = "capture-device")]
fn decode_device_frame_format(value: i32) -> FfiResult<DeviceFrameFormat> {
    proto::DeviceFrameFormat::try_from(value)
        .map(device_frame_format_from_proto)
        .map_err(|_| FfiError::InvalidRequest("invalid device frame format".into()))
}

#[cfg(feature = "capture-device")]
fn device_format_from_proto(format: proto::DeviceFormat) -> FfiResult<DeviceFormat> {
    Ok(DeviceFormat {
        resolution: format.resolution.into(),
        framerate_fps: format.framerate_fps,
        frame_format: decode_device_frame_format(format.frame_format)?,
    })
}

#[cfg(feature = "capture-device")]
fn device_format_to_proto(format: DeviceFormat) -> Option<proto::DeviceFormat> {
    Some(proto::DeviceFormat {
        resolution: proto::VideoSourceResolution {
            width: format.resolution.width,
            height: format.resolution.height,
        },
        framerate_fps: format.framerate_fps,
        frame_format: device_frame_format_to_proto(format.frame_format)?.into(),
    })
}

#[cfg(feature = "capture-device")]
fn device_format_request_from_proto(
    request: proto::DeviceFormatRequest,
) -> FfiResult<DeviceFormatRequest> {
    use proto::device_format_request::Request;
    Ok(match request.request {
        None => DeviceFormatRequest::Default,
        Some(Request::Exact(format)) => {
            DeviceFormatRequest::Exact(device_format_from_proto(format)?)
        }
        Some(Request::Closest(format)) => {
            DeviceFormatRequest::Closest(device_format_from_proto(format)?)
        }
        Some(Request::HighestFramerate(constraint)) => DeviceFormatRequest::HighestFramerate {
            resolution: constraint.resolution.map(VideoResolution::from),
            frame_format: constraint.frame_format.map(decode_device_frame_format).transpose()?,
        },
        Some(Request::HighestResolution(constraint)) => DeviceFormatRequest::HighestResolution {
            framerate_fps: constraint.framerate_fps,
            frame_format: constraint.frame_format.map(decode_device_frame_format).transpose()?,
        },
    })
}

#[cfg(feature = "capture-device")]
pub fn device_config_from_proto(
    config: proto::DeviceVideoSourceConfig,
) -> FfiResult<DeviceVideoSourceConfig> {
    use proto::device_video_source_config::Device;
    let device = match config.device {
        None => DeviceSelector::Default,
        Some(Device::DeviceIndex(index)) => DeviceSelector::Index(index as usize),
        Some(Device::DeviceId(id)) => DeviceSelector::Id(id),
    };
    let format =
        config.format.map(device_format_request_from_proto).transpose()?.unwrap_or_default();
    Ok(DeviceVideoSourceConfig { device, format })
}

#[cfg(feature = "capture-device")]
pub fn device_info_to_proto(info: DeviceInfo) -> proto::CaptureDeviceInfo {
    proto::CaptureDeviceInfo {
        id: info.id,
        name: info.name,
        model_id: info.model_id,
        manufacturer: info.manufacturer,
        formats: info.formats.into_iter().filter_map(device_format_to_proto).collect(),
        formats_complete: info.formats_complete,
    }
}

#[cfg(feature = "capture-rtsp")]
pub fn rtsp_config_from_proto(
    config: proto::RtspVideoSourceConfig,
) -> FfiResult<RtspVideoSourceConfig> {
    let codec = config
        .codec
        .map(|value| {
            proto::VideoCodec::try_from(value)
                .map(video_codec_from_proto)
                .map_err(|_| FfiError::InvalidRequest("invalid codec".into()))
        })
        .transpose()?;

    Ok(RtspVideoSourceConfig {
        url: config.url,
        username: config.username,
        password: config.password,
        codec,
        resolution: config.resolution.map(VideoResolution::from),
        connect_timeout_ms: config.connect_timeout_ms,
        idle_timeout_ms: config.idle_timeout_ms,
        accept_invalid_tls_certs: config.accept_invalid_tls_certs.unwrap_or_default(),
    })
}

#[cfg(feature = "capture-gstreamer")]
pub fn gstreamer_config_from_proto(
    config: proto::GstreamerVideoSourceConfig,
) -> FfiResult<GStreamerVideoSourceConfig> {
    let codec = config
        .codec
        .map(|value| {
            proto::VideoCodec::try_from(value)
                .map(video_codec_from_proto)
                .map_err(|_| FfiError::InvalidRequest("invalid codec".into()))
        })
        .transpose()?;

    let rate_control = config
        .rate_control
        .map(|rate_control| {
            let unit = proto::GstreamerBitrateUnit::try_from(rate_control.unit)
                .map_err(|_| FfiError::InvalidRequest("invalid bitrate unit".into()))?;
            Ok::<_, FfiError>(GStreamerRateControlConfig {
                element: rate_control.element,
                property: rate_control.property,
                unit: unit.into(),
            })
        })
        .transpose()?;

    Ok(GStreamerVideoSourceConfig {
        pipeline: config.pipeline,
        codec,
        resolution: config.resolution.map(VideoResolution::from),
        rate_control,
    })
}
