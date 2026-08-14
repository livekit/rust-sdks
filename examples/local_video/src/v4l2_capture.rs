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

//! Direct Linux V4L2 capture for the local-video publisher.

use super::{
    rockchip_mjpeg::MppMjpegDecoder, CaptureConfig as PublisherCaptureConfig, CaptureFormat,
};
use anyhow::{Context, Result};
use livekit::webrtc::video_frame::{
    FrameMetadata, I420Buffer, NV12Buffer, VideoBuffer, VideoFrame, VideoRotation,
};
use livekit::webrtc::video_source::native::NativeVideoSource;
use log::{debug, info, warn};
use std::fs::File;
use std::io;
use std::ops::Range;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::ptr::NonNull;
use std::sync::{atomic::AtomicBool, atomic::Ordering, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use v4l::buffer::Type as BufferType;
use v4l::capability::Flags as CapabilityFlags;
use v4l::io::traits::CaptureStream;
use v4l::prelude::*;
use v4l::v4l_sys::*;
use v4l::video::Capture;
use v4l::{v4l2, FourCC};

const DEFAULT_DEVICE_PATH: &str = "/dev/video-camera0";
const STREAM_BUFFER_COUNT: u32 = 4;
const CAPTURE_TIMEOUT_MS: i32 = 5_000;
const MAX_CONSECUTIVE_ERRORS: u32 = 30;
const MAX_CONSECUTIVE_MPP_ERRORS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PixelFormat {
    Nv12,
    Yuyv,
    Mjpeg,
}

impl PixelFormat {
    fn from_capture_format(format: CaptureFormat) -> Self {
        match format {
            CaptureFormat::Auto | CaptureFormat::Nv12 => Self::Nv12,
            CaptureFormat::Yuv => Self::Yuyv,
            CaptureFormat::Mjpeg => Self::Mjpeg,
        }
    }

    fn fourcc(self) -> FourCC {
        match self {
            Self::Nv12 => FourCC::new(b"NV12"),
            Self::Yuyv => FourCC::new(b"YUYV"),
            Self::Mjpeg => FourCC::new(b"MJPG"),
        }
    }

    fn minimum_stride(self, width: u32) -> Result<Option<u32>> {
        match self {
            Self::Nv12 => Ok(Some(width)),
            Self::Yuyv => {
                width.checked_mul(2).map(Some).context("YUYV width overflows its packed row size")
            }
            Self::Mjpeg => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureApi {
    SinglePlanar,
    MultiPlanar,
}

/// A validated direct-capture device configuration.
pub(super) struct CaptureDevice {
    device_path: String,
    sensor_subdevice: Option<String>,
    format: PixelFormat,
    api: CaptureApi,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug, Clone, Copy)]
struct NegotiatedFormat {
    width: u32,
    height: u32,
    fourcc: FourCC,
    stride: u32,
}

/// Lists V4L2 devices visible to the process.
pub(super) fn list_devices() {
    println!("V4L2 devices:");
    for device in v4l::context::enum_devices() {
        println!("  {} - {}", device.path().display(), device.name().unwrap_or_default());
    }
}

/// Negotiates and validates a direct V4L2 capture configuration.
pub(super) fn configure(
    device_path: Option<&str>,
    sensor_subdevice: Option<&str>,
    requested_format: CaptureFormat,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<CaptureDevice> {
    anyhow::ensure!(width > 0 && height > 0, "capture dimensions must be non-zero");
    anyhow::ensure!(fps > 0, "capture frame rate must be non-zero");

    let device_path = device_path.unwrap_or(DEFAULT_DEVICE_PATH).to_owned();
    let sensor_subdevice = sensor_subdevice.map(str::to_owned);
    let format = PixelFormat::from_capture_format(requested_format);
    let device = Device::with_path(&device_path)
        .with_context(|| format!("failed to open V4L2 device {device_path}"))?;
    let api = select_capture_api(&device, format)?;
    let negotiated = negotiate_format(&device, api, format, width, height)?;
    set_frame_rate(&device, api, sensor_subdevice.as_deref(), fps);

    info!(
        "V4L2 negotiated ({api:?}): {}x{} fourcc={} stride={}",
        negotiated.width, negotiated.height, negotiated.fourcc, negotiated.stride
    );

    Ok(CaptureDevice {
        device_path,
        sensor_subdevice,
        format,
        api,
        width: negotiated.width,
        height: negotiated.height,
    })
}

/// Runs capture on Tokio's blocking pool so raw device I/O does not occupy an async worker.
pub(super) async fn run(
    config: PublisherCaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    capture: CaptureDevice,
    width: u32,
    height: u32,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        run_blocking(config, ctrl_c_received, rtc_source, capture, width, height)
    })
    .await
    .context("V4L2 capture task panicked")?
}

fn run_blocking(
    config: PublisherCaptureConfig,
    ctrl_c_received: Arc<AtomicBool>,
    rtc_source: NativeVideoSource,
    capture: CaptureDevice,
    width: u32,
    height: u32,
) -> Result<()> {
    let device = Device::with_path(&capture.device_path)
        .with_context(|| format!("failed to reopen V4L2 device {}", capture.device_path))?;
    let negotiated =
        negotiate_format(&device, capture.api, capture.format, capture.width, capture.height)?;
    anyhow::ensure!(
        negotiated.width == width && negotiated.height == height,
        "V4L2 renegotiated {}x{} after publishing {}x{}",
        negotiated.width,
        negotiated.height,
        width,
        height
    );
    set_frame_rate(&device, capture.api, capture.sensor_subdevice.as_deref(), config.fps);

    match capture.api {
        CaptureApi::SinglePlanar => {
            let mut stream = SinglePlanarStream::new(&device)?;
            run_capture_loop(
                &rtc_source,
                &mut stream,
                capture.format,
                negotiated.stride,
                width,
                height,
                &ctrl_c_received,
                config,
                "single-planar",
            )
        }
        CaptureApi::MultiPlanar => {
            let mut stream = MultiPlanarStream::new(&device, STREAM_BUFFER_COUNT)?;
            run_capture_loop(
                &rtc_source,
                &mut stream,
                capture.format,
                negotiated.stride,
                width,
                height,
                &ctrl_c_received,
                config,
                "multi-planar",
            )
        }
    }
}

fn select_capture_api(device: &Device, format: PixelFormat) -> Result<CaptureApi> {
    let capabilities = device.query_caps()?.capabilities;
    let supports_single = capabilities.contains(CapabilityFlags::VIDEO_CAPTURE);
    let supports_multi = capabilities.contains(CapabilityFlags::VIDEO_CAPTURE_MPLANE);
    let requested = format.fourcc();

    let single_formats = if supports_single {
        device.enum_formats().context("failed to enumerate single-planar formats")?
    } else {
        Vec::new()
    };
    let multi_formats =
        if supports_multi { enumerate_multi_planar_formats(device)? } else { Vec::new() };

    for description in &single_formats {
        debug!("V4L2 single-planar format: {description:?}");
    }
    for description in &multi_formats {
        debug!("V4L2 multi-planar format: {description:?}");
    }

    if single_formats.iter().any(|description| description.fourcc == requested) {
        return Ok(CaptureApi::SinglePlanar);
    }
    if multi_formats.iter().any(|description| description.fourcc == requested) {
        return Ok(CaptureApi::MultiPlanar);
    }

    let supported = single_formats
        .iter()
        .chain(&multi_formats)
        .map(|description| description.fourcc.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "V4L2 device does not support requested format {}; available formats: [{}]",
        requested,
        supported
    )
}

fn negotiate_format(
    device: &Device,
    api: CaptureApi,
    format: PixelFormat,
    width: u32,
    height: u32,
) -> Result<NegotiatedFormat> {
    let negotiated = match api {
        CaptureApi::SinglePlanar => {
            let requested = v4l::Format::new(width, height, format.fourcc());
            let actual = device.set_format(&requested)?;
            NegotiatedFormat {
                width: actual.width,
                height: actual.height,
                fourcc: actual.fourcc,
                stride: actual.stride,
            }
        }
        CaptureApi::MultiPlanar => set_multi_planar_format(device, width, height, format.fourcc())?,
    };
    validate_negotiated_format(format, negotiated)?;
    Ok(negotiated)
}

fn validate_negotiated_format(format: PixelFormat, negotiated: NegotiatedFormat) -> Result<()> {
    anyhow::ensure!(
        negotiated.fourcc == format.fourcc(),
        "V4L2 substituted format {} for requested {}",
        negotiated.fourcc,
        format.fourcc()
    );
    anyhow::ensure!(
        negotiated.width > 0 && negotiated.height > 0,
        "V4L2 returned zero-sized capture dimensions"
    );
    anyhow::ensure!(
        negotiated.width <= i32::MAX as u32 && negotiated.height <= i32::MAX as u32,
        "V4L2 returned dimensions that exceed the converter's signed range"
    );
    if let Some(minimum_stride) = format.minimum_stride(negotiated.width)? {
        anyhow::ensure!(
            negotiated.stride >= minimum_stride,
            "V4L2 returned stride {} for {:?}, expected at least {}",
            negotiated.stride,
            format,
            minimum_stride
        );
        anyhow::ensure!(
            negotiated.stride <= i32::MAX as u32,
            "V4L2 returned a stride that exceeds the converter's signed range"
        );
    }
    Ok(())
}

fn set_frame_rate(device: &Device, api: CaptureApi, sensor_subdevice: Option<&str>, fps: u32) {
    let result = match api {
        CaptureApi::SinglePlanar => device
            .set_params(&v4l::video::capture::Parameters::with_fps(fps))
            .map(|parameters| (parameters.interval.denominator, parameters.interval.numerator)),
        CaptureApi::MultiPlanar => set_multi_planar_frame_rate(device, fps),
    };

    match result {
        Ok((numerator, denominator)) => {
            info!("V4L2 frame rate set: {numerator}/{denominator}")
        }
        Err(capture_error) => {
            let Some(sensor_subdevice) = sensor_subdevice else {
                warn!(
                    "Could not set V4L2 frame rate to {fps} fps: {capture_error}; pass --sensor-subdevice only when the capture node delegates frame timing to a known sensor"
                );
                return;
            };
            match set_sensor_subdevice_frame_rate(sensor_subdevice, fps) {
                Ok((numerator, denominator)) => info!(
                    "Sensor frame rate set on {sensor_subdevice}: {numerator}/{denominator}"
                ),
                Err(sensor_error) => warn!(
                    "Could not set {fps} fps through capture node ({capture_error}) or sensor subdevice {sensor_subdevice} ({sensor_error})"
                ),
            }
        }
    }
}

fn enumerate_multi_planar_formats(device: &Device) -> io::Result<Vec<v4l::format::Description>> {
    let mut formats = Vec::new();
    // SAFETY: v4l2_fmtdesc is a plain C data structure where all-zero is a valid
    // starting state before setting the buffer type and index.
    let mut description: v4l2_fmtdesc = unsafe { std::mem::zeroed() };
    description.type_ = BufferType::VideoCaptureMplane as u32;

    loop {
        // SAFETY: description points to initialized writable storage for the
        // duration of the ioctl, and the device fd remains owned by Device.
        let result = unsafe {
            v4l2::ioctl(
                device.handle().fd(),
                v4l2::vidioc::VIDIOC_ENUM_FMT,
                &mut description as *mut _ as *mut std::os::raw::c_void,
            )
        };
        if let Err(error) = result {
            if description.index == 0 && error.raw_os_error() != Some(libc::EINVAL) {
                return Err(error);
            }
            break;
        }
        formats.push(v4l::format::Description::from(description));
        description.index += 1;
        description.description.fill(0);
    }
    Ok(formats)
}

fn set_multi_planar_format(
    device: &Device,
    width: u32,
    height: u32,
    fourcc: FourCC,
) -> Result<NegotiatedFormat> {
    // SAFETY: v4l2_format is zero-initialized as required by V4L2. The active
    // union member is pix_mp because type_ is VIDEO_CAPTURE_MPLANE, and the
    // pointer passed to ioctl remains valid for the call.
    unsafe {
        let mut raw_format: v4l2_format = std::mem::zeroed();
        raw_format.type_ = BufferType::VideoCaptureMplane as u32;
        let pixel_format = &mut raw_format.fmt.pix_mp;
        pixel_format.width = width;
        pixel_format.height = height;
        pixel_format.pixelformat = fourcc.into();
        pixel_format.num_planes = 1;

        v4l2::ioctl(
            device.handle().fd(),
            v4l2::vidioc::VIDIOC_S_FMT,
            &mut raw_format as *mut _ as *mut std::os::raw::c_void,
        )?;

        let pixel_format = &raw_format.fmt.pix_mp;
        anyhow::ensure!(
            pixel_format.num_planes == 1,
            "V4L2 negotiated {} memory planes; only contiguous one-plane capture is supported",
            pixel_format.num_planes
        );
        Ok(NegotiatedFormat {
            width: pixel_format.width,
            height: pixel_format.height,
            fourcc: FourCC::from(pixel_format.pixelformat),
            stride: pixel_format.plane_fmt[0].bytesperline,
        })
    }
}

fn set_multi_planar_frame_rate(device: &Device, fps: u32) -> io::Result<(u32, u32)> {
    // SAFETY: v4l2_streamparm is zero-initialized, its capture union member is
    // selected by VIDEO_CAPTURE_MPLANE, and ioctl receives valid writable storage.
    unsafe {
        let mut parameters: v4l2_streamparm = std::mem::zeroed();
        parameters.type_ = BufferType::VideoCaptureMplane as u32;
        parameters.parm.capture.timeperframe.numerator = 1;
        parameters.parm.capture.timeperframe.denominator = fps;

        v4l2::ioctl(
            device.handle().fd(),
            v4l2::vidioc::VIDIOC_S_PARM,
            &mut parameters as *mut _ as *mut std::os::raw::c_void,
        )?;
        let interval = parameters.parm.capture.timeperframe;
        Ok((interval.denominator, interval.numerator))
    }
}

fn set_sensor_subdevice_frame_rate(path: &str, fps: u32) -> io::Result<(u32, u32)> {
    #[repr(C)]
    struct SubdeviceFrameInterval {
        pad: u32,
        numerator: u32,
        denominator: u32,
        reserved: [u32; 9],
    }

    const IOC_WRITE: u32 = 1;
    const IOC_READ: u32 = 2;
    fn ioctl_read_write(ty: u8, number: u8, size: usize) -> libc::c_ulong {
        (((IOC_READ | IOC_WRITE) as libc::c_ulong) << 30)
            | ((ty as libc::c_ulong) << 8)
            | (number as libc::c_ulong)
            | ((size as libc::c_ulong) << 16)
    }

    let raw_fd = v4l2::open(path, libc::O_RDWR | libc::O_NONBLOCK)?;
    // SAFETY: v4l2::open returned a newly owned descriptor. File takes sole
    // ownership here and closes it exactly once on drop.
    let file = unsafe { File::from_raw_fd(raw_fd) };
    let request = ioctl_read_write(b'V', 22, std::mem::size_of::<SubdeviceFrameInterval>());
    let mut interval =
        SubdeviceFrameInterval { pad: 0, numerator: 1, denominator: fps, reserved: [0; 9] };
    // SAFETY: interval has the kernel ABI layout expected by
    // VIDIOC_SUBDEV_S_FRAME_INTERVAL and remains writable for the ioctl call.
    unsafe {
        v4l2::ioctl(
            file.as_raw_fd(),
            request,
            &mut interval as *mut _ as *mut std::os::raw::c_void,
        )?;
    }
    Ok((interval.denominator, interval.numerator))
}

trait FrameStream {
    fn next_frame(&mut self) -> io::Result<&[u8]>;
}

struct SinglePlanarStream<'a> {
    stream: v4l::io::mmap::Stream<'a>,
}

impl<'a> SinglePlanarStream<'a> {
    fn new(device: &'a Device) -> io::Result<Self> {
        let mut stream = v4l::io::mmap::Stream::with_buffers(
            device,
            BufferType::VideoCapture,
            STREAM_BUFFER_COUNT,
        )?;
        stream.set_timeout(Duration::from_millis(CAPTURE_TIMEOUT_MS as u64));
        Ok(Self { stream })
    }
}

impl FrameStream for SinglePlanarStream<'_> {
    fn next_frame(&mut self) -> io::Result<&[u8]> {
        let (buffer, metadata) = self.stream.next()?;
        let bytes_used = metadata.bytesused as usize;
        buffer.get(..bytes_used).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("V4L2 reported {bytes_used} bytes for a {}-byte mmap buffer", buffer.len()),
            )
        })
    }
}

struct MappedBuffer {
    pointer: NonNull<u8>,
    length: usize,
}

impl MappedBuffer {
    fn map(fd: RawFd, length: usize, offset: libc::off_t) -> io::Result<Self> {
        // SAFETY: fd is an open V4L2 descriptor, length and offset were returned
        // by VIDIOC_QUERYBUF, and the mapping is released by Drop.
        let pointer = unsafe {
            v4l2::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                offset,
            )?
        };
        let pointer = NonNull::new(pointer.cast::<u8>())
            .ok_or_else(|| io::Error::other("V4L2 mmap returned a null pointer"))?;
        Ok(Self { pointer, length })
    }

    fn payload(&self, data_offset: usize, bytes_used: usize) -> io::Result<&[u8]> {
        let payload = validate_payload_bounds(data_offset, bytes_used, self.length)?;
        // SAFETY: the bounds above prove the requested range lies inside the
        // live mmap allocation owned by self.
        Ok(unsafe {
            std::slice::from_raw_parts(self.pointer.as_ptr().add(payload.start), payload.len())
        })
    }
}

fn validate_payload_bounds(
    data_offset: usize,
    bytes_used: usize,
    mapped_length: usize,
) -> io::Result<Range<usize>> {
    if data_offset > bytes_used || bytes_used > mapped_length {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "invalid V4L2 plane bounds: offset={data_offset}, bytesused={bytes_used}, mapped={mapped_length}"
            ),
        ));
    }
    Ok(data_offset..bytes_used)
}

impl Drop for MappedBuffer {
    fn drop(&mut self) {
        // SAFETY: pointer and length describe the live mapping created in map,
        // and Drop runs exactly once for this owner.
        unsafe {
            let _ = v4l2::munmap(self.pointer.as_ptr().cast(), self.length);
        }
    }
}

struct MultiPlanarStream {
    handle: Arc<v4l::device::Handle>,
    buffers: Vec<MappedBuffer>,
    buffer_count: u32,
    dequeued_index: Option<usize>,
    active: bool,
}

impl MultiPlanarStream {
    fn new(device: &Device, requested_count: u32) -> io::Result<Self> {
        let handle = device.handle();
        // SAFETY: request_buffers is zero-initialized as required by V4L2 and
        // ioctl receives valid writable storage tied to the live device fd.
        let count = unsafe {
            let mut request_buffers: v4l2_requestbuffers = std::mem::zeroed();
            request_buffers.count = requested_count;
            request_buffers.type_ = BufferType::VideoCaptureMplane as u32;
            request_buffers.memory = v4l::memory::Memory::Mmap as u32;
            v4l2::ioctl(
                handle.fd(),
                v4l2::vidioc::VIDIOC_REQBUFS,
                &mut request_buffers as *mut _ as *mut std::os::raw::c_void,
            )?;
            request_buffers.count
        };
        if count == 0 {
            return Err(io::Error::other("V4L2 allocated no multi-planar buffers"));
        }

        let mut stream = Self {
            handle,
            buffers: Vec::with_capacity(count as usize),
            buffer_count: count,
            dequeued_index: None,
            active: false,
        };
        for index in 0..count {
            let (length, offset) = stream.query_buffer(index)?;
            stream.buffers.push(MappedBuffer::map(stream.handle.fd(), length, offset)?);
        }
        Ok(stream)
    }

    fn query_buffer(&self, index: u32) -> io::Result<(usize, libc::off_t)> {
        // SAFETY: plane and buffer are zero-initialized V4L2 structures. The
        // planes pointer remains valid throughout VIDIOC_QUERYBUF.
        unsafe {
            let mut plane: v4l2_plane = std::mem::zeroed();
            let mut buffer: v4l2_buffer = std::mem::zeroed();
            buffer.type_ = BufferType::VideoCaptureMplane as u32;
            buffer.memory = v4l::memory::Memory::Mmap as u32;
            buffer.index = index;
            buffer.length = 1;
            buffer.m.planes = &mut plane;
            v4l2::ioctl(
                self.handle.fd(),
                v4l2::vidioc::VIDIOC_QUERYBUF,
                &mut buffer as *mut _ as *mut std::os::raw::c_void,
            )?;
            Ok((plane.length as usize, plane.m.mem_offset as libc::off_t))
        }
    }

    fn queue(&self, index: usize) -> io::Result<()> {
        let mapped = self.buffers.get(index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("V4L2 requested invalid buffer index {index}"),
            )
        })?;
        // SAFETY: plane and buffer live through VIDIOC_QBUF. The index was
        // bounds-checked and plane.length matches the corresponding mmap.
        unsafe {
            let mut plane: v4l2_plane = std::mem::zeroed();
            plane.length = mapped.length.try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "V4L2 plane is larger than u32")
            })?;
            let mut buffer: v4l2_buffer = std::mem::zeroed();
            buffer.type_ = BufferType::VideoCaptureMplane as u32;
            buffer.memory = v4l::memory::Memory::Mmap as u32;
            buffer.index = index as u32;
            buffer.length = 1;
            buffer.m.planes = &mut plane;
            v4l2::ioctl(
                self.handle.fd(),
                v4l2::vidioc::VIDIOC_QBUF,
                &mut buffer as *mut _ as *mut std::os::raw::c_void,
            )
        }
    }

    fn start(&mut self) -> io::Result<()> {
        for index in 0..self.buffer_count as usize {
            self.queue(index)?;
        }
        // SAFETY: the stream type value is valid and writable for STREAMON.
        unsafe {
            let mut buffer_type = BufferType::VideoCaptureMplane as u32;
            v4l2::ioctl(
                self.handle.fd(),
                v4l2::vidioc::VIDIOC_STREAMON,
                &mut buffer_type as *mut _ as *mut std::os::raw::c_void,
            )?;
        }
        self.active = true;
        Ok(())
    }

    fn dequeue(&mut self) -> io::Result<(usize, usize, usize)> {
        let mut poll_fd = libc::pollfd { fd: self.handle.fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: poll_fd points to one initialized descriptor for the entire call.
        let poll_result = unsafe { libc::poll(&mut poll_fd, 1, CAPTURE_TIMEOUT_MS) };
        if poll_result == 0 {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "VIDIOC_DQBUF poll timeout"));
        }
        if poll_result < 0 {
            return Err(io::Error::last_os_error());
        }
        if poll_fd.revents & libc::POLLIN == 0 {
            return Err(io::Error::other(format!(
                "V4L2 poll returned unexpected events 0x{:x}",
                poll_fd.revents
            )));
        }

        // SAFETY: plane and buffer are valid writable V4L2 structures, and the
        // planes pointer remains live for the duration of VIDIOC_DQBUF.
        unsafe {
            let mut plane: v4l2_plane = std::mem::zeroed();
            let mut buffer: v4l2_buffer = std::mem::zeroed();
            buffer.type_ = BufferType::VideoCaptureMplane as u32;
            buffer.memory = v4l::memory::Memory::Mmap as u32;
            buffer.length = 1;
            buffer.m.planes = &mut plane;
            v4l2::ioctl(
                self.handle.fd(),
                v4l2::vidioc::VIDIOC_DQBUF,
                &mut buffer as *mut _ as *mut std::os::raw::c_void,
            )?;
            Ok((buffer.index as usize, plane.data_offset as usize, plane.bytesused as usize))
        }
    }
}

impl FrameStream for MultiPlanarStream {
    fn next_frame(&mut self) -> io::Result<&[u8]> {
        if !self.active {
            self.start()?;
        } else if let Some(index) = self.dequeued_index {
            self.queue(index)?;
            self.dequeued_index = None;
        }

        let (index, data_offset, bytes_used) = self.dequeue()?;
        if self.buffers.get(index).is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("V4L2 dequeued invalid buffer index {index}"),
            ));
        }
        self.dequeued_index = Some(index);
        self.buffers[index].payload(data_offset, bytes_used)
    }
}

impl Drop for MultiPlanarStream {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: the stream type is valid and the device handle outlives this call.
            unsafe {
                let mut buffer_type = BufferType::VideoCaptureMplane as u32;
                let _ = v4l2::ioctl(
                    self.handle.fd(),
                    v4l2::vidioc::VIDIOC_STREAMOFF,
                    &mut buffer_type as *mut _ as *mut std::os::raw::c_void,
                );
            }
        }
        self.buffers.clear();
        // SAFETY: a zero-count REQBUFS releases buffers allocated by new. The
        // structure and fd remain valid through the ioctl.
        unsafe {
            let mut request_buffers: v4l2_requestbuffers = std::mem::zeroed();
            request_buffers.count = 0;
            request_buffers.type_ = BufferType::VideoCaptureMplane as u32;
            request_buffers.memory = v4l::memory::Memory::Mmap as u32;
            let _ = v4l2::ioctl(
                self.handle.fd(),
                v4l2::vidioc::VIDIOC_REQBUFS,
                &mut request_buffers as *mut _ as *mut std::os::raw::c_void,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_capture_loop<S: FrameStream>(
    rtc_source: &NativeVideoSource,
    stream: &mut S,
    format: PixelFormat,
    source_stride: u32,
    width: u32,
    height: u32,
    ctrl_c_received: &AtomicBool,
    config: PublisherCaptureConfig,
    api_name: &str,
) -> Result<()> {
    let target = Duration::from_secs_f64(1.0 / config.fps as f64);
    let start_timestamp = Instant::now();
    let mut frames = 0_u64;
    let mut last_fps_log = Instant::now();
    let mut capture_time_ms = 0.0;
    let mut conversion_time_ms = 0.0;
    let mut submit_time_ms = 0.0;
    let mut mpp_decode_time_ms = 0.0;
    let mut software_decode_time_ms = 0.0;
    let mut mpp_decoded_frames = 0_u64;
    let mut software_decoded_frames = 0_u64;
    let mut consecutive_errors = 0_u32;
    let mut consecutive_invalid_frames = 0_u32;
    let mut consecutive_mpp_errors = 0_u32;
    let mut frame_counter = 1_u32;
    let mut mpp_decoder = if format == PixelFormat::Mjpeg {
        match MppMjpegDecoder::new(width, height) {
            Ok(decoder) => {
                info!(
                    "Rockchip MPP MJPEG hardware decoder enabled: {}x{} NV12 output",
                    width, height
                );
                Some(decoder)
            }
            Err(error) => {
                info!("Rockchip MPP MJPEG hardware decoder unavailable; using libyuv: {error}");
                None
            }
        }
    } else {
        None
    };

    info!(
        "Direct V4L2 {format:?} {api_name} capture started: {}x{} stride={source_stride}",
        width, height
    );

    loop {
        if ctrl_c_received.load(Ordering::Acquire) {
            break;
        }

        let capture_started = Instant::now();
        let source = match stream.next_frame() {
            Ok(source) => {
                consecutive_errors = 0;
                source
            }
            Err(error) => {
                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    anyhow::bail!(
                        "V4L2 capture failed {consecutive_errors} consecutive times: {error}"
                    );
                }
                warn!(
                    "V4L2 capture error ({consecutive_errors}/{MAX_CONSECUTIVE_ERRORS}): {error}"
                );
                continue;
            }
        };
        let capture_wall_time_us = unix_time_us_now();
        let capture_finished = Instant::now();

        let timestamp_us = start_timestamp.elapsed().as_micros() as i64;
        match format {
            PixelFormat::Nv12 => {
                let mut buffer =
                    NV12Buffer::with_strides(width, height, source_stride, source_stride);
                if let Err(error) = copy_nv12_to_buffer(source, &mut buffer, source_stride, height)
                {
                    record_invalid_frame(&error, &mut consecutive_invalid_frames)?;
                    continue;
                }
                let conversion_finished = Instant::now();
                let frame_metadata =
                    next_frame_metadata(config, capture_wall_time_us, &mut frame_counter);
                rtc_source.capture_frame(&VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us,
                    frame_metadata,
                    buffer,
                });
                conversion_time_ms +=
                    (conversion_finished - capture_finished).as_secs_f64() * 1_000.0;
                submit_time_ms += conversion_finished.elapsed().as_secs_f64() * 1_000.0;
            }
            PixelFormat::Yuyv => {
                let mut buffer = I420Buffer::new(width, height);
                if let Err(error) = convert_to_i420(format, source, source_stride, &mut buffer) {
                    record_invalid_frame(&error, &mut consecutive_invalid_frames)?;
                    continue;
                }
                let conversion_finished = Instant::now();
                let frame_metadata =
                    next_frame_metadata(config, capture_wall_time_us, &mut frame_counter);
                rtc_source.capture_frame(&VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us,
                    frame_metadata,
                    buffer,
                });
                conversion_time_ms +=
                    (conversion_finished - capture_finished).as_secs_f64() * 1_000.0;
                submit_time_ms += conversion_finished.elapsed().as_secs_f64() * 1_000.0;
            }
            PixelFormat::Mjpeg => {
                let mut captured_with_mpp = false;
                if let Some(decoder) = mpp_decoder.as_mut() {
                    let decode_started = Instant::now();
                    let mut buffer = NV12Buffer::new(width, height);
                    match decoder.decode(source, &mut buffer) {
                        Ok(()) => {
                            let conversion_finished = Instant::now();
                            let frame_metadata = next_frame_metadata(
                                config,
                                capture_wall_time_us,
                                &mut frame_counter,
                            );
                            rtc_source.capture_frame(&VideoFrame {
                                rotation: VideoRotation::VideoRotation0,
                                timestamp_us,
                                frame_metadata,
                                buffer,
                            });
                            let decode_ms =
                                (conversion_finished - decode_started).as_secs_f64() * 1_000.0;
                            conversion_time_ms += decode_ms;
                            mpp_decode_time_ms += decode_ms;
                            mpp_decoded_frames += 1;
                            submit_time_ms += conversion_finished.elapsed().as_secs_f64() * 1_000.0;
                            consecutive_mpp_errors = 0;
                            captured_with_mpp = true;
                        }
                        Err(error) => {
                            consecutive_mpp_errors += 1;
                            warn!(
                                "Rockchip MPP MJPEG decode failed ({consecutive_mpp_errors}/{MAX_CONSECUTIVE_MPP_ERRORS}); using libyuv for this frame: {error}"
                            );
                        }
                    }
                }

                if !captured_with_mpp {
                    if consecutive_mpp_errors >= MAX_CONSECUTIVE_MPP_ERRORS {
                        warn!(
                            "Disabling Rockchip MPP MJPEG decoding after {consecutive_mpp_errors} consecutive failures"
                        );
                        mpp_decoder = None;
                        consecutive_mpp_errors = 0;
                    }

                    let decode_started = Instant::now();
                    let mut buffer = I420Buffer::new(width, height);
                    if let Err(error) = convert_to_i420(format, source, source_stride, &mut buffer)
                    {
                        record_invalid_frame(&error, &mut consecutive_invalid_frames)?;
                        continue;
                    }
                    let conversion_finished = Instant::now();
                    let frame_metadata =
                        next_frame_metadata(config, capture_wall_time_us, &mut frame_counter);
                    rtc_source.capture_frame(&VideoFrame {
                        rotation: VideoRotation::VideoRotation0,
                        timestamp_us,
                        frame_metadata,
                        buffer,
                    });
                    let decode_ms = (conversion_finished - decode_started).as_secs_f64() * 1_000.0;
                    conversion_time_ms += decode_ms;
                    software_decode_time_ms += decode_ms;
                    software_decoded_frames += 1;
                    submit_time_ms += conversion_finished.elapsed().as_secs_f64() * 1_000.0;
                }
            }
        }

        consecutive_invalid_frames = 0;
        frames += 1;
        capture_time_ms += (capture_finished - capture_started).as_secs_f64() * 1_000.0;

        if last_fps_log.elapsed() >= Duration::from_secs(2) {
            let elapsed = last_fps_log.elapsed().as_secs_f64();
            let frame_count = frames.max(1) as f64;
            if format == PixelFormat::Mjpeg {
                let average_mpp_ms = mpp_decode_time_ms / mpp_decoded_frames.max(1) as f64;
                let average_software_ms =
                    software_decode_time_ms / software_decoded_frames.max(1) as f64;
                info!(
                    "V4L2 {format:?} {api_name}: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, decode+copy mpp {:.2} ({} frames), libyuv {:.2} ({} frames), submit {:.2} | target {:.2}",
                    width,
                    height,
                    frames as f64 / elapsed,
                    capture_time_ms / frame_count,
                    average_mpp_ms,
                    mpp_decoded_frames,
                    average_software_ms,
                    software_decoded_frames,
                    submit_time_ms / frame_count,
                    target.as_secs_f64() * 1_000.0,
                );
            } else {
                info!(
                    "V4L2 {format:?} {api_name}: {}x{}, ~{:.1} fps | avg ms: capture {:.2}, convert {:.2}, submit {:.2} | target {:.2}",
                    width,
                    height,
                    frames as f64 / elapsed,
                    capture_time_ms / frame_count,
                    conversion_time_ms / frame_count,
                    submit_time_ms / frame_count,
                    target.as_secs_f64() * 1_000.0,
                );
            }
            frames = 0;
            capture_time_ms = 0.0;
            conversion_time_ms = 0.0;
            submit_time_ms = 0.0;
            mpp_decode_time_ms = 0.0;
            software_decode_time_ms = 0.0;
            mpp_decoded_frames = 0;
            software_decoded_frames = 0;
            last_fps_log = Instant::now();
        }
    }

    Ok(())
}

fn record_invalid_frame(error: &anyhow::Error, consecutive_invalid_frames: &mut u32) -> Result<()> {
    *consecutive_invalid_frames += 1;
    if *consecutive_invalid_frames >= MAX_CONSECUTIVE_ERRORS {
        anyhow::bail!(
            "V4L2 produced {consecutive_invalid_frames} consecutive invalid frames: {error}"
        );
    }
    warn!(
        "Skipping invalid V4L2 frame ({consecutive_invalid_frames}/{MAX_CONSECUTIVE_ERRORS}): {error}"
    );
    Ok(())
}

fn copy_nv12_to_buffer(
    source: &[u8],
    destination: &mut NV12Buffer,
    stride: u32,
    height: u32,
) -> Result<()> {
    let stride = stride as usize;
    let height = height as usize;
    let y_length = stride.checked_mul(height).context("NV12 Y plane size overflow")?;
    let uv_height = height.div_ceil(2);
    let uv_length = stride.checked_mul(uv_height).context("NV12 UV plane size overflow")?;
    let total_length = y_length.checked_add(uv_length).context("NV12 frame size overflow")?;
    anyhow::ensure!(
        source.len() >= total_length,
        "short NV12 frame: received {} bytes, expected at least {}",
        source.len(),
        total_length
    );

    let (destination_y, destination_uv) = destination.data_mut();
    anyhow::ensure!(
        destination_y.len() == y_length && destination_uv.len() == uv_length,
        "NV12 destination layout does not match its negotiated strides"
    );
    destination_y.copy_from_slice(&source[..y_length]);
    destination_uv.copy_from_slice(&source[y_length..total_length]);
    Ok(())
}

fn convert_to_i420(
    format: PixelFormat,
    source: &[u8],
    source_stride: u32,
    destination: &mut I420Buffer,
) -> Result<()> {
    let width = destination.width();
    let height = destination.height();
    let (stride_y, stride_u, stride_v) = destination.strides();
    let (data_y, data_u, data_v) = destination.data_mut();

    match format {
        PixelFormat::Nv12 => unreachable!("NV12 uses its native capture path"),
        PixelFormat::Yuyv => {
            let required_length = (source_stride as usize)
                .checked_mul(height as usize)
                .context("YUYV frame size overflow")?;
            anyhow::ensure!(
                source.len() >= required_length,
                "short YUYV frame: received {} bytes, expected at least {}",
                source.len(),
                required_length
            );
            // SAFETY: source length was validated for source_stride * height.
            // I420Buffer owns writable Y/U/V planes sized for width and height,
            // and their exact strides are passed to libyuv.
            let result = unsafe {
                yuv_sys::rs_YUY2ToI420(
                    source.as_ptr(),
                    source_stride as i32,
                    data_y.as_mut_ptr(),
                    stride_y as i32,
                    data_u.as_mut_ptr(),
                    stride_u as i32,
                    data_v.as_mut_ptr(),
                    stride_v as i32,
                    width as i32,
                    height as i32,
                )
            };
            anyhow::ensure!(result == 0, "YUY2ToI420 failed with status {result}");
            Ok(())
        }
        PixelFormat::Mjpeg => {
            anyhow::ensure!(!source.is_empty(), "received an empty MJPEG frame");
            // SAFETY: libyuv receives the exact source slice length and writable
            // destination planes owned by I420Buffer with their actual strides.
            let result = unsafe {
                yuv_sys::rs_MJPGToI420(
                    source.as_ptr(),
                    source.len(),
                    data_y.as_mut_ptr(),
                    stride_y as i32,
                    data_u.as_mut_ptr(),
                    stride_u as i32,
                    data_v.as_mut_ptr(),
                    stride_v as i32,
                    width as i32,
                    height as i32,
                    width as i32,
                    height as i32,
                )
            };
            anyhow::ensure!(result == 0, "MJPGToI420 failed with status {result}");
            Ok(())
        }
    }
}

fn next_frame_metadata(
    config: PublisherCaptureConfig,
    capture_wall_time_us: u64,
    frame_counter: &mut u32,
) -> Option<FrameMetadata> {
    let user_timestamp = config.attach_timestamp.then_some(capture_wall_time_us);
    let frame_id = config.attach_frame_id.then(|| {
        let frame_id = *frame_counter;
        *frame_counter = frame_counter.wrapping_add(1);
        frame_id
    });

    (user_timestamp.is_some() || frame_id.is_some()).then_some(FrameMetadata {
        user_timestamp,
        frame_id,
        user_data: None,
    })
}

fn unix_time_us_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_nv12_frames() {
        let mut destination = NV12Buffer::new(4, 4);
        let error = copy_nv12_to_buffer(&[0; 23], &mut destination, 4, 4)
            .expect_err("one byte short must be rejected");
        assert!(error.to_string().contains("short NV12 frame"));
    }

    #[test]
    fn validates_negotiated_fourcc() {
        let error = validate_negotiated_format(
            PixelFormat::Nv12,
            NegotiatedFormat {
                width: 640,
                height: 480,
                fourcc: FourCC::new(b"YUYV"),
                stride: 1_280,
            },
        )
        .expect_err("format substitution must be rejected");
        assert!(error.to_string().contains("substituted format"));
    }

    #[test]
    fn validates_uncompressed_stride() {
        let error = validate_negotiated_format(
            PixelFormat::Yuyv,
            NegotiatedFormat { width: 640, height: 480, fourcc: FourCC::new(b"YUYV"), stride: 640 },
        )
        .expect_err("undersized stride must be rejected");
        assert!(error.to_string().contains("expected at least 1280"));
    }

    #[test]
    fn validates_multi_planar_payload_bounds() {
        assert_eq!(validate_payload_bounds(8, 64, 128).unwrap(), 8..64);
        assert!(validate_payload_bounds(65, 64, 128).is_err());
        assert!(validate_payload_bounds(8, 129, 128).is_err());
    }
}
