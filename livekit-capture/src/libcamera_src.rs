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

//! Raspberry Pi CSI camera capture via libcamera, producing DMABUF-backed
//! [`CaptureFrame::Native`] frames.
//!
//! All libcamera state (manager, camera, active_camera, requests) lives
//! on a dedicated worker thread. Captured DMABUF descriptors are sent to
//! the consumer via an mpsc channel; the consumer signals completion by
//! dropping an [`InflightToken`], which lets the worker re-queue the
//! underlying request to libcamera.
//!
//! This sidesteps the self-referential `CameraManager`/`ActiveCamera`
//! lifetime tangle that an in-place implementation would require.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use libcamera::camera::CameraConfigurationStatus;
use libcamera::camera_manager::CameraManager;
use libcamera::color_space::ColorSpace;
use libcamera::control::{ControlEntry, ControlList};
use libcamera::controls::FrameDurationLimits;
use libcamera::framebuffer::{AsFrameBuffer, FrameMetadataStatus};
use libcamera::framebuffer_allocator::{FrameBuffer, FrameBufferAllocator};
use libcamera::geometry::Size;
use libcamera::pixel_format::PixelFormat;
use libcamera::request::{RequestStatus, ReuseFlag};
use libcamera::stream::StreamRole;
use libwebrtc::video_frame::native::{DmabufFrameDesc, DmabufPlane, Fourcc, NativeBuffer};
use log::{info, warn};

use crate::{Capture, CaptureConfig, CaptureError, CaptureFrame, StreamFormat};

/// `YU12` -- planar I420.
const PIXEL_FORMAT_YUV420: PixelFormat =
    PixelFormat::new(u32::from_le_bytes([b'Y', b'U', b'1', b'2']), 0);
/// `NV12` -- planar Y + interleaved UV.
const PIXEL_FORMAT_NV12: PixelFormat =
    PixelFormat::new(u32::from_le_bytes([b'N', b'V', b'1', b'2']), 0);

/// Number of libcamera buffers to allocate. Must be larger than
/// `MAX_INFLIGHT` so libcamera always has at least 2 free buffers to
/// feed the ISP.
const NUM_BUFFERS: u32 = 6;
/// Maximum number of completed requests parked while waiting for the
/// encoder to drain. Provides ~4 frames of slack at 30fps.
const MAX_INFLIGHT: usize = 4;

/// A single captured frame, delivered from worker to consumer.
struct CaptureMessage {
    desc: DmabufFrameDesc,
    capture_ts_us: Option<u64>,
    /// Cookie (index) the consumer must send back through `release_tx`
    /// once it's done with the frame.
    cookie: u64,
    release_tx: mpsc::Sender<u64>,
}

/// Raspberry Pi CSI camera capture via libcamera.
///
/// Frames yield as [`CaptureFrame::Native`] with a DMABUF fd that the
/// V4L2 hardware encoder can import via `V4L2_MEMORY_DMABUF`.
pub struct LibCameraCapture {
    worker: Option<WorkerHandles>,
    format: Option<StreamFormat>,
    inflight: VecDeque<InflightToken>,
}

struct WorkerHandles {
    join: Option<JoinHandle<()>>,
    frame_rx: mpsc::Receiver<CaptureMessage>,
    shutdown_tx: mpsc::Sender<()>,
}

/// Holds a release sender; dropping it signals the worker that the
/// associated request can be re-queued to libcamera.
struct InflightToken {
    release_tx: mpsc::Sender<u64>,
    cookie: u64,
}

impl Drop for InflightToken {
    fn drop(&mut self) {
        let _ = self.release_tx.send(self.cookie);
    }
}

fn frame_duration_us_for_fps(fps: u32) -> i64 {
    let fps = fps.max(1);
    ((1_000_000u64 + u64::from(fps / 2)) / u64::from(fps)).max(1) as i64
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0)
}

#[derive(Debug, Default)]
struct SensorTimestampMapper {
    anchor_sensor_ns: Option<u64>,
    anchor_wall_us: u64,
}

impl SensorTimestampMapper {
    fn wall_time_us(&mut self, sensor_ns: u64) -> u64 {
        let Some(anchor_sensor_ns) = self.anchor_sensor_ns else {
            let wall_us = unix_time_us_now();
            self.anchor_sensor_ns = Some(sensor_ns);
            self.anchor_wall_us = wall_us;
            return wall_us;
        };

        let delta_us = sensor_ns.saturating_sub(anchor_sensor_ns) / 1000;
        self.anchor_wall_us.saturating_add(delta_us)
    }
}

fn apply_frame_duration_control(req: &mut libcamera::request::Request, fps: u32) {
    let frame_duration_us = frame_duration_us_for_fps(fps);
    if let Err(e) =
        req.controls_mut().set(FrameDurationLimits([frame_duration_us, frame_duration_us]))
    {
        warn!("LibCameraCapture: failed to set request frame duration: {e}");
    }
}

impl LibCameraCapture {
    /// Construct a new (unstarted) libcamera capture backend.
    pub fn new() -> Self {
        Self { worker: None, format: None, inflight: VecDeque::with_capacity(MAX_INFLIGHT + 1) }
    }
}

impl Default for LibCameraCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Capture for LibCameraCapture {
    fn start(&mut self, cfg: &CaptureConfig) -> Result<StreamFormat, CaptureError> {
        let (init_tx, init_rx) = mpsc::channel::<Result<StreamFormat, CaptureError>>();
        let (frame_tx, frame_rx) = mpsc::channel::<CaptureMessage>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let cfg_cloned = cfg.clone();
        let join = thread::Builder::new()
            .name("livekit-capture-libcamera".into())
            .spawn(move || {
                run_worker(cfg_cloned, init_tx, frame_tx, shutdown_rx);
            })
            .map_err(|e| CaptureError::DeviceUnavailable(format!("spawn worker: {e}")))?;

        let fmt = init_rx
            .recv()
            .map_err(|_| CaptureError::DeviceUnavailable("worker thread died".into()))??;

        self.worker = Some(WorkerHandles { join: Some(join), frame_rx, shutdown_tx });
        self.format = Some(fmt);
        Ok(fmt)
    }

    fn next_frame(&mut self, timeout: Duration) -> Result<Option<CaptureFrame>, CaptureError> {
        let worker = self.worker.as_ref().ok_or_else(|| {
            CaptureError::DeviceUnavailable("LibCameraCapture not started".into())
        })?;

        let msg = match worker.frame_rx.recv_timeout(timeout) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(CaptureError::FrameRead("worker thread disconnected".into()));
            }
        };

        let CaptureMessage { desc, capture_ts_us, cookie, release_tx } = msg;
        let buffer = NativeBuffer::from_dmabuf(&desc).ok_or_else(|| {
            CaptureError::Conversion("NativeBuffer::from_dmabuf rejected descriptor".into())
        })?;

        // Park the release token. Once we exceed `MAX_INFLIGHT`, drop
        // the oldest to free that libcamera buffer for re-use.
        self.inflight.push_back(InflightToken { release_tx, cookie });
        while self.inflight.len() > MAX_INFLIGHT {
            let _ = self.inflight.pop_front();
        }

        Ok(Some(CaptureFrame::Native { buffer, capture_ts_us }))
    }

    fn stop(&mut self) {
        self.inflight.clear();
        if let Some(mut worker) = self.worker.take() {
            let _ = worker.shutdown_tx.send(());
            if let Some(join) = worker.join.take() {
                let _ = join.join();
            }
        }
        self.format = None;
    }
}

impl Drop for LibCameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

fn run_worker(
    cfg: CaptureConfig,
    init_tx: mpsc::Sender<Result<StreamFormat, CaptureError>>,
    frame_tx: mpsc::Sender<CaptureMessage>,
    shutdown_rx: mpsc::Receiver<()>,
) {
    let mgr = match CameraManager::new() {
        Ok(m) => m,
        Err(e) => {
            let _ = init_tx
                .send(Err(CaptureError::DeviceUnavailable(format!("CameraManager::new: {e}"))));
            return;
        }
    };

    let cameras = mgr.cameras();
    let cam_ref = match cameras.get(cfg.camera_index) {
        Some(c) => c,
        None => {
            let _ = init_tx.send(Err(CaptureError::DeviceUnavailable(format!(
                "no libcamera at index {}",
                cfg.camera_index
            ))));
            return;
        }
    };

    let mut active_cam = match cam_ref.acquire() {
        Ok(c) => c,
        Err(e) => {
            let _ =
                init_tx.send(Err(CaptureError::DeviceUnavailable(format!("Camera::acquire: {e}"))));
            return;
        }
    };
    info!("LibCameraCapture: acquired camera index {}", cfg.camera_index);

    // Configure a single video-recording stream at the requested size,
    // preferring YUV420 (matches V4L2 encoder default).
    let mut cfgs = match active_cam.generate_configuration(&[StreamRole::VideoRecording]) {
        Some(c) => c,
        None => {
            let _ = init_tx.send(Err(CaptureError::UnsupportedFormat(
                "generate_configuration returned None".into(),
            )));
            return;
        }
    };
    {
        let mut stream_cfg = match cfgs.get_mut(0) {
            Some(s) => s,
            None => {
                let _ = init_tx.send(Err(CaptureError::UnsupportedFormat(
                    "no stream in configuration".into(),
                )));
                return;
            }
        };
        stream_cfg.set_size(Size { width: cfg.width, height: cfg.height });
        stream_cfg.set_buffer_count(NUM_BUFFERS);
        stream_cfg.set_pixel_format(PIXEL_FORMAT_YUV420);
        let color_space = if cfg.width >= 1280 || cfg.height >= 720 {
            ColorSpace::rec709()
        } else {
            ColorSpace::smpte170m()
        };
        stream_cfg.set_color_space(Some(color_space));
    }

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => {}
        CameraConfigurationStatus::Adjusted => {
            info!("LibCameraCapture: configuration adjusted by libcamera");
        }
        CameraConfigurationStatus::Invalid => {
            let _ = init_tx.send(Err(CaptureError::UnsupportedFormat(
                "libcamera rejected configuration".into(),
            )));
            return;
        }
    }

    let (fourcc, neg_w, neg_h, neg_stride, neg_frame_size) = {
        let Some(stream_cfg) = cfgs.get(0) else {
            let _ = init_tx.send(Err(CaptureError::UnsupportedFormat(
                "no stream in validated configuration".into(),
            )));
            return;
        };
        let pf = stream_cfg.get_pixel_format();
        let fourcc = if pf == PIXEL_FORMAT_YUV420 {
            Fourcc::YUV420
        } else if pf == PIXEL_FORMAT_NV12 {
            Fourcc::NV12
        } else {
            let _ = init_tx.send(Err(CaptureError::UnsupportedFormat(format!(
                "libcamera negotiated unsupported pixel format {pf:?}"
            ))));
            return;
        };
        let size = stream_cfg.get_size();
        (fourcc, size.width, size.height, stream_cfg.get_stride(), stream_cfg.get_frame_size())
    };
    info!(
        "LibCameraCapture: negotiated {}x{} stride {}, frame_size {}, fourcc {:?}",
        neg_w, neg_h, neg_stride, neg_frame_size, fourcc
    );

    if let Err(e) = active_cam.configure(&mut cfgs) {
        let _ = init_tx.send(Err(CaptureError::DeviceUnavailable(format!("configure: {e}"))));
        return;
    }

    let stream = match cfgs.get(0).and_then(|c| c.stream()) {
        Some(s) => s,
        None => {
            let _ = init_tx
                .send(Err(CaptureError::DeviceUnavailable("no stream after configure".into())));
            return;
        }
    };

    let mut alloc = FrameBufferAllocator::new(&active_cam);
    let buffers = match alloc.alloc(&stream) {
        Ok(b) => b,
        Err(e) => {
            let _ = init_tx.send(Err(CaptureError::DeviceUnavailable(format!(
                "FrameBufferAllocator::alloc: {e}"
            ))));
            return;
        }
    };
    info!("LibCameraCapture: allocated {} dmabuf buffers", buffers.len());
    let num_buffers = buffers.len();

    // One slot per buffer; cookie == slot index. Requests are owned by
    // libcamera while queued (see ActiveCameraState.requests).
    let mut requests = Vec::with_capacity(num_buffers);
    for (idx, buf) in buffers.into_iter().enumerate() {
        let mut req = match active_cam.create_request(Some(idx as u64)) {
            Some(r) => r,
            None => {
                let _ = init_tx.send(Err(CaptureError::DeviceUnavailable(
                    "create_request returned None".into(),
                )));
                return;
            }
        };
        if let Err(e) = req.add_buffer(&stream, buf) {
            let _ = init_tx
                .send(Err(CaptureError::DeviceUnavailable(format!("Request::add_buffer: {e}"))));
            return;
        }
        apply_frame_duration_control(&mut req, cfg.fps);
        requests.push(Some(req));
    }

    let completed_rx = active_cam.subscribe_request_completed();

    let frame_duration_us = frame_duration_us_for_fps(cfg.fps);
    let mut start_controls = ControlList::new();
    if active_cam.controls().count(<FrameDurationLimits as ControlEntry>::ID) > 0 {
        if let Err(e) =
            start_controls.set(FrameDurationLimits([frame_duration_us, frame_duration_us]))
        {
            warn!("LibCameraCapture: failed to set start frame duration: {e}");
        } else {
            info!(
                "LibCameraCapture: requested fixed frame duration {} us (~{} fps)",
                frame_duration_us, cfg.fps
            );
        }
    } else {
        warn!("LibCameraCapture: camera does not advertise FrameDurationLimits control");
    }

    if let Err(e) = active_cam.start(Some(&start_controls)) {
        let _ =
            init_tx.send(Err(CaptureError::DeviceUnavailable(format!("ActiveCamera::start: {e}"))));
        return;
    }

    for slot in requests.iter_mut() {
        if let Some(req) = slot.take() {
            if let Err((req, e)) = active_cam.queue_request(req) {
                warn!("LibCameraCapture: initial queue_request failed: {e}");
                *slot = Some(req);
            }
        }
    }

    let _ = init_tx.send(Ok(StreamFormat { width: neg_w, height: neg_h, fps: cfg.fps }));

    // ---- main pump loop ----

    let (release_tx, release_rx) = mpsc::channel::<u64>();
    let mut timestamp_mapper = SensorTimestampMapper::default();

    'pump: loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        // Drain release notifications, re-queueing each.
        while let Ok(idx) = release_rx.try_recv() {
            let slot = match requests.get_mut(idx as usize) {
                Some(s) => s,
                None => continue,
            };
            if let Some(mut req) = slot.take() {
                req.reuse(ReuseFlag::REUSE_BUFFERS);
                apply_frame_duration_control(&mut req, cfg.fps);
                if let Err((req, e)) = active_cam.queue_request(req) {
                    warn!("LibCameraCapture: re-queue failed: {e}");
                    *slot = Some(req);
                }
            }
        }

        // Wait for the next completed request (short timeout so we keep
        // polling release and shutdown signals).
        let req = match completed_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        let cookie = req.cookie();
        let idx = cookie as usize;

        let Some(descriptor) =
            build_descriptor(&req, &stream, fourcc, neg_w, neg_h, neg_stride, neg_frame_size)
        else {
            // Couldn't build descriptor; immediately re-queue.
            let mut req = req;
            req.reuse(ReuseFlag::REUSE_BUFFERS);
            apply_frame_duration_control(&mut req, cfg.fps);
            if let Err((req, e)) = active_cam.queue_request(req) {
                warn!("LibCameraCapture: re-queue (drop) failed: {e}");
                if idx < requests.len() {
                    requests[idx] = Some(req);
                }
            }
            continue;
        };

        // Park the Request until the consumer signals completion.
        if idx >= requests.len() {
            // Defensive: drop the request.
            continue 'pump;
        }
        requests[idx] = Some(req);

        let capture_ts_us =
            descriptor.sensor_ts_ns.map(|sensor_ns| timestamp_mapper.wall_time_us(sensor_ns));
        let msg = CaptureMessage {
            desc: descriptor.desc,
            capture_ts_us,
            cookie,
            release_tx: release_tx.clone(),
        };
        if frame_tx.send(msg).is_err() {
            break; // consumer hung up
        }
    }

    if let Err(e) = active_cam.stop() {
        warn!("LibCameraCapture: camera stop failed: {e}");
    }
    info!("LibCameraCapture: worker thread exiting");
}

struct DescriptorResult {
    desc: DmabufFrameDesc,
    sensor_ts_ns: Option<u64>,
}

/// Build a [`DmabufFrameDesc`] from a completed libcamera [`Request`].
fn build_descriptor(
    req: &libcamera::request::Request,
    stream: &libcamera::stream::Stream,
    fourcc: Fourcc,
    width: u32,
    height: u32,
    stride: u32,
    frame_size: u32,
) -> Option<DescriptorResult> {
    if req.status() != RequestStatus::Complete {
        warn!("LibCameraCapture: dropping request with status {:?}", req.status());
        return None;
    }

    let fb: &FrameBuffer = match req.buffer::<FrameBuffer>(stream) {
        Some(b) => b,
        None => return None,
    };

    let metadata = match fb.metadata() {
        Some(metadata) => metadata,
        None => return None,
    };
    let status = metadata.status();
    if status != FrameMetadataStatus::Success {
        warn!("LibCameraCapture: dropping frame with metadata status {status:?}");
        return None;
    }
    let sensor_ts_ns = Some(metadata.timestamp());

    let planes = fb.planes();
    let mut plane_descs = Vec::with_capacity(planes.len());
    let mut total_size: u64 = 0;
    let mut primary_fd: i32 = -1;
    let mut primary_offset: u64 = 0;
    for i in 0..planes.len() {
        let plane = match planes.get(i) {
            Some(p) => p,
            None => continue,
        };
        let fd = plane.fd();
        let offset = plane.offset().unwrap_or(0) as u64;
        let length = plane.len() as u64;
        if primary_fd == -1 {
            primary_fd = fd;
            primary_offset = offset;
        } else if fd != primary_fd {
            warn!(
                "LibCameraCapture: planes use multiple dmabuf fds; \
                 not supported by V4L2 encoder import path"
            );
            return None;
        }
        // libcamera reports a single stream-level stride (the Y row pitch).
        // Per-plane strides have to be derived from the pixel format:
        //   YUV420: Y=stride, U=stride/2, V=stride/2
        //   NV12:   Y=stride, UV=stride (interleaved chroma uses full width)
        // Using the Y stride for chroma planes makes ToI420's libyuv read
        // walk off the end of the mapped region (CVE-style OOB read /
        // segfault), so this must be precise.
        let plane_stride = match fourcc {
            Fourcc::YUV420 => {
                if i == 0 {
                    stride as i32
                } else {
                    (stride / 2) as i32
                }
            }
            Fourcc::NV12 => stride as i32,
            _ => stride as i32,
        };
        plane_descs.push(DmabufPlane { offset, stride: plane_stride });
        total_size = total_size.max(offset + length);
    }

    if primary_fd < 0 || plane_descs.is_empty() {
        return None;
    }

    if frame_size > 0 {
        total_size = total_size.max(primary_offset + u64::from(frame_size));
    }

    let desc =
        DmabufFrameDesc { fd: primary_fd, fourcc, width, height, total_size, planes: plane_descs };
    Some(DescriptorResult { desc, sensor_ts_ns })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_duration_rounds_to_nearest_microsecond() {
        assert_eq!(frame_duration_us_for_fps(30), 33_333);
        assert_eq!(frame_duration_us_for_fps(0), 1_000_000);
    }

    #[test]
    fn sensor_timestamp_mapper_keeps_monotonic_wall_clock() {
        let mut mapper = SensorTimestampMapper::default();
        let first = mapper.wall_time_us(1_000_000_000);
        let second = mapper.wall_time_us(1_033_333_000);

        assert_eq!(second.saturating_sub(first), 33_333);
    }
}
