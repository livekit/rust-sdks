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

//! USB UVC webcam capture via `nokhwa`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libwebrtc::video_frame::I420Buffer;
use log::{info, warn};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution,
};
use nokhwa::Camera;

use crate::{Capture, CaptureConfig, CaptureError, CaptureFrame, StreamFormat};

/// USB / UVC camera capture backend. Always produces
/// [`CaptureFrame::I420`].
pub struct UvcCapture {
    camera: Option<Camera>,
    format: Option<StreamFormat>,
    logged_mjpeg_fallback: bool,
    logged_sensor_ts_source: bool,
    logged_sensor_ts_missing: bool,
}

impl UvcCapture {
    /// Create a new (unopened) UVC capture backend.
    pub fn new() -> Self {
        Self {
            camera: None,
            format: None,
            logged_mjpeg_fallback: false,
            logged_sensor_ts_source: false,
            logged_sensor_ts_missing: false,
        }
    }
}

impl Default for UvcCapture {
    fn default() -> Self {
        Self::new()
    }
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

impl Capture for UvcCapture {
    fn start(&mut self, cfg: &CaptureConfig) -> Result<StreamFormat, CaptureError> {
        let index = CameraIndex::Index(cfg.camera_index as u32);
        let requested =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut camera = Camera::new(index, requested)
            .map_err(|e| CaptureError::DeviceUnavailable(format!("nokhwa: {e}")))?;

        // Try raw YUYV first (cheaper than MJPEG), fall back to MJPEG.
        let wanted =
            CameraFormat::new(Resolution::new(cfg.width, cfg.height), FrameFormat::YUYV, cfg.fps);
        let mut using_fmt = "YUYV";
        if camera
            .set_camera_requset(RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(
                wanted,
            )))
            .is_err()
        {
            let alt = CameraFormat::new(
                Resolution::new(cfg.width, cfg.height),
                FrameFormat::MJPEG,
                cfg.fps,
            );
            using_fmt = "MJPEG";
            let _ = camera.set_camera_requset(RequestedFormat::new::<RgbFormat>(
                RequestedFormatType::Exact(alt),
            ));
        }
        camera
            .open_stream()
            .map_err(|e| CaptureError::DeviceUnavailable(format!("open_stream: {e}")))?;
        let fmt = camera.camera_format();
        let stream_fmt =
            StreamFormat { width: fmt.width(), height: fmt.height(), fps: fmt.frame_rate() };
        info!(
            "UvcCapture: opened {}x{} @ {} fps (format: {})",
            stream_fmt.width, stream_fmt.height, stream_fmt.fps, using_fmt
        );
        info!(
            "UvcCapture: capture path: CPU I420 copy ({} camera frames converted before publish)",
            using_fmt
        );
        self.camera = Some(camera);
        self.format = Some(stream_fmt);
        Ok(stream_fmt)
    }

    fn next_frame(&mut self, _timeout: Duration) -> Result<Option<CaptureFrame>, CaptureError> {
        let camera = match self.camera.as_mut() {
            Some(c) => c,
            None => return Err(CaptureError::DeviceUnavailable("not started".into())),
        };
        let fmt = self.format.ok_or_else(|| {
            CaptureError::DeviceUnavailable("StreamFormat unset after start".into())
        })?;

        // `Camera::frame` blocks; the caller-provided timeout is best-effort here.
        let fallback_wall_time_us = unix_time_us_now();
        let frame_buf =
            camera.frame().map_err(|e| CaptureError::FrameRead(format!("nokhwa: {e}")))?;

        let capture_wall_time_us = match frame_buf.capture_timestamp() {
            Some(d) => {
                if !self.logged_sensor_ts_source {
                    info!("UvcCapture: using sensor capture_timestamp for user_timestamp");
                    self.logged_sensor_ts_source = true;
                }
                d.as_micros() as u64
            }
            None => {
                if !self.logged_sensor_ts_missing {
                    warn!(
                        "UvcCapture: Buffer::capture_timestamp() not available; falling back to \
                         system wall clock"
                    );
                    self.logged_sensor_ts_missing = true;
                }
                fallback_wall_time_us
            }
        };

        // Allocate a fresh I420 buffer per frame -- the webrtc-sys
        // I420Buffer is refcounted so callers can hand it directly to
        // NativeVideoSource::capture_frame and forget about it.
        let mut frame = I420Buffer::new(fmt.width, fmt.height);
        let (stride_y, stride_u, stride_v) = frame.strides();
        let (data_y, data_u, data_v) = frame.data_mut();

        let src = frame_buf.buffer();
        let src_bytes = src.as_ref();
        let camera_fmt = camera.camera_format();
        if camera_fmt.format() == FrameFormat::YUYV {
            let src_stride = (fmt.width * 2) as i32;
            unsafe {
                let _ = yuv_sys::rs_YUY2ToI420(
                    src_bytes.as_ptr(),
                    src_stride,
                    data_y.as_mut_ptr(),
                    stride_y as i32,
                    data_u.as_mut_ptr(),
                    stride_u as i32,
                    data_v.as_mut_ptr(),
                    stride_v as i32,
                    fmt.width as i32,
                    fmt.height as i32,
                );
            }
        } else if src_bytes.len() == (fmt.width as usize * fmt.height as usize * 3) {
            // Already RGB24 from backend; convert directly.
            unsafe {
                let _ = yuv_sys::rs_RGB24ToI420(
                    src_bytes.as_ptr(),
                    (fmt.width * 3) as i32,
                    data_y.as_mut_ptr(),
                    stride_y as i32,
                    data_u.as_mut_ptr(),
                    stride_u as i32,
                    data_v.as_mut_ptr(),
                    stride_v as i32,
                    fmt.width as i32,
                    fmt.height as i32,
                );
            }
        } else {
            // Try fast MJPEG -> I420 via libyuv; fall back to the image crate.
            let mut used_fast_mjpeg = false;
            unsafe {
                let ret = yuv_sys::rs_MJPGToI420(
                    src_bytes.as_ptr(),
                    src_bytes.len(),
                    data_y.as_mut_ptr(),
                    stride_y as i32,
                    data_u.as_mut_ptr(),
                    stride_u as i32,
                    data_v.as_mut_ptr(),
                    stride_v as i32,
                    fmt.width as i32,
                    fmt.height as i32,
                    fmt.width as i32,
                    fmt.height as i32,
                );
                if ret == 0 {
                    used_fast_mjpeg = true;
                }
            }
            if !used_fast_mjpeg {
                match image::load_from_memory(src_bytes) {
                    Ok(img_dyn) => {
                        let rgb8 = img_dyn.to_rgb8();
                        if rgb8.width() != fmt.width || rgb8.height() != fmt.height {
                            return Err(CaptureError::Conversion(format!(
                                "decoded MJPEG size {}x{} differs from requested {}x{}",
                                rgb8.width(),
                                rgb8.height(),
                                fmt.width,
                                fmt.height
                            )));
                        }
                        unsafe {
                            let _ = yuv_sys::rs_RGB24ToI420(
                                rgb8.as_raw().as_ptr(),
                                (fmt.width * 3) as i32,
                                data_y.as_mut_ptr(),
                                stride_y as i32,
                                data_u.as_mut_ptr(),
                                stride_u as i32,
                                data_v.as_mut_ptr(),
                                stride_v as i32,
                                fmt.width as i32,
                                fmt.height as i32,
                            );
                        }
                    }
                    Err(e) => {
                        if !self.logged_mjpeg_fallback {
                            log::error!(
                                "UvcCapture: MJPEG decode failed; buffer not RGB24 and image \
                                 decode failed: {e}"
                            );
                            self.logged_mjpeg_fallback = true;
                        }
                        return Err(CaptureError::Conversion(format!("MJPEG decode: {e}")));
                    }
                }
            }
        }

        Ok(Some(CaptureFrame::I420 { buffer: frame, capture_ts_us: Some(capture_wall_time_us) }))
    }

    fn stop(&mut self) {
        if let Some(mut cam) = self.camera.take() {
            let _ = cam.stop_stream();
        }
        self.format = None;
    }
}
