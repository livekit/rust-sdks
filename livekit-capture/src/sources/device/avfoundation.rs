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

//! macOS device capture backend built on AVFoundation.
//!
//! This module is an implementation detail of [`super::DeviceVideoSource`]:
//! nothing AVFoundation-specific leaves it. Frames are delivered as native
//! IOSurface-backed `CVPixelBuffer`s when the negotiated session supports
//! that (full-range NV12 without software scaling), and converted to I420
//! otherwise.

use std::ffi::c_void;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use dispatch2::{DispatchQueue, DispatchRetained};
use livekit::webrtc::video_frame::{
    native::NativeBuffer, BoxVideoFrame, I420Buffer, VideoBuffer, VideoFrame, VideoRotation,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, Message};
use objc2_av_foundation::{
    AVCaptureDevice, AVCaptureDeviceFormat, AVCaptureDeviceInput, AVCaptureOutput,
    AVCaptureSession, AVCaptureSessionPreset1280x720, AVCaptureSessionPreset1920x1080,
    AVCaptureSessionPreset640x480, AVCaptureSessionPresetHigh, AVCaptureSessionPresetInputPriority,
    AVCaptureSessionPresetMedium, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVCaptureVideoStabilizationMode,
    AVMediaTypeVideo,
};
use objc2_core_media::{
    CMClock, CMSampleBuffer, CMTime, CMTimeFlags, CMVideoFormatDescriptionGetDimensions,
};
use objc2_core_video::{
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, kCVPixelFormatType_420YpCbCr8Planar,
    kCVPixelFormatType_420YpCbCr8PlanarFullRange, kCVPixelFormatType_422YpCbCr8,
    kCVPixelFormatType_422YpCbCr8FullRange, kCVPixelFormatType_422YpCbCr8_yuvs, kCVReturnSuccess,
    CVImageBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidth, CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

use super::timestamp::{
    elapsed_us, unix_time_us_now, validate_capture_timestamp_us, MAX_CAPTURE_TIMESTAMP_AGE_US,
};
use super::{
    capture_frame_metadata, DeviceFormat, DeviceFormatRequest, DeviceFrameFormat, DeviceInfo,
    DeviceSelector, DeviceVideoSourceConfig, DeviceVideoSourceError,
};
use crate::{primitive::VideoResolution, pump::PumpStop};

unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CVPixelBufferGetIOSurface(pixel_buffer: *const CVPixelBuffer) -> *const c_void;
}

/// How long session construction waits for the device's first frame, which
/// establishes the delivered format.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// How long one frame wait may block before the stop token is rechecked.
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Returns whether the backend can request this frame format from a device.
fn is_supported_request_format(frame_format: DeviceFrameFormat) -> bool {
    matches!(
        frame_format,
        DeviceFrameFormat::Nv12 | DeviceFrameFormat::Bgra | DeviceFrameFormat::I420
    )
}

/// AVFoundation capture session satisfying the backend contract.
pub(super) struct Session {
    format: DeviceFormat,
    target_resolution: Option<VideoResolution>,
    native_frame_supported: bool,
    inner: SessionInner,
}

// SAFETY: `Session` owns AVFoundation objects and only exposes `&mut self`
// frame capture plus `Drop`; moving ownership to another thread does not
// create concurrent access to those Objective-C objects.
unsafe impl Send for Session {}

impl Session {
    /// Opens a capture session and waits for the first frame to establish
    /// the delivered format.
    pub(super) fn open(config: &DeviceVideoSourceConfig) -> Result<Self, DeviceVideoSourceError> {
        super::validate_config(config, is_supported_request_format)?;

        let inner = SessionInner::new(config)?;
        let initial_frame = inner.wait_for_format(FIRST_FRAME_TIMEOUT)?;
        inner.discard_pending_frame();
        let mut format = initial_frame.format;
        format.framerate_fps = requested_framerate(&config.format).unwrap_or(30);
        let target_resolution = requested_output_resolution(&config.format, format.resolution);
        if let Some(resolution) = target_resolution {
            format.resolution = resolution;
        }
        let session = Self {
            format,
            target_resolution,
            native_frame_supported: initial_frame.native_frame_supported,
            inner,
        };
        log::info!(
            "Opened device \"{}\" ({}): {} ({})",
            session.inner.device_name,
            session.inner.device_id,
            session.format,
            if session.native_capture() { "native buffers" } else { "converted to I420" },
        );
        Ok(session)
    }

    /// Returns the negotiated capture format.
    pub(super) fn format(&self) -> DeviceFormat {
        self.format
    }

    fn native_capture(&self) -> bool {
        self.native_frame_supported
            && self.target_resolution.is_none()
            && self.format.frame_format == DeviceFrameFormat::Nv12
    }

    /// Blocks until the next frame is available, returning `Ok(None)` once
    /// the stop token fires.
    pub(super) fn next_frame(
        &mut self,
        stop: &PumpStop,
    ) -> Result<Option<BoxVideoFrame>, DeviceVideoSourceError> {
        // Convert only after the frame queue's mutex is released: conversion
        // locks the pixel buffer and runs a full-frame libyuv copy, and
        // holding the mutex through that would block `push_frame` on the
        // AVFoundation delegate queue, which drops camera frames while
        // stalled (`setAlwaysDiscardsLateVideoFrames(true)`).
        let Some(queued) = self.inner.wait_take_queued_frame(stop)? else {
            return Ok(None);
        };

        if self.native_capture() {
            return queued.into_native_frame().map(|frame| Some(box_frame(frame)));
        }

        let mut frame = queued.into_i420_frame()?;
        if let Some(resolution) = self.target_resolution {
            if frame.buffer.width() != resolution.width
                || frame.buffer.height() != resolution.height
            {
                let width = i32::try_from(resolution.width).map_err(|_| {
                    DeviceVideoSourceError::InvalidFrame("scaled width exceeds i32")
                })?;
                let height = i32::try_from(resolution.height).map_err(|_| {
                    DeviceVideoSourceError::InvalidFrame("scaled height exceeds i32")
                })?;
                frame.buffer = frame.buffer.scale(width, height);
            }
        }
        Ok(Some(box_frame(frame)))
    }
}

/// Type-erases a concrete frame for the pixel source contract.
fn box_frame<B: VideoBuffer + AsRef<dyn VideoBuffer> + 'static>(
    frame: VideoFrame<B>,
) -> BoxVideoFrame {
    VideoFrame {
        rotation: frame.rotation,
        timestamp_us: frame.timestamp_us,
        frame_metadata: frame.frame_metadata,
        buffer: Box::new(frame.buffer),
    }
}

/// Lists AVFoundation video capture devices.
pub(super) fn devices() -> Result<Vec<DeviceInfo>, DeviceVideoSourceError> {
    // SAFETY: AVMediaTypeVideo is a framework-provided immutable NSString
    // constant. We only borrow it to ask AVFoundation for video devices.
    let media_type = unsafe { AVMediaTypeVideo }.ok_or(DeviceVideoSourceError::DeviceNotFound)?;
    // SAFETY: AVFoundation returns an immutable NSArray of currently available
    // AVCaptureDevice instances. We only retain/copy string properties from it.
    #[allow(deprecated)]
    let devices = unsafe { AVCaptureDevice::devicesWithMediaType(media_type) };

    let mut results = Vec::with_capacity(devices.len());
    for device in devices.iter() {
        // SAFETY: These Objective-C property getters return retained NSStrings
        // for a live AVCaptureDevice from the immutable devices array.
        let id = unsafe { device.uniqueID() }.to_string();
        let name = unsafe { device.localizedName() }.to_string();
        let model_id = non_empty_string(unsafe { device.modelID() }.to_string());
        let manufacturer = non_empty_string(unsafe { device.manufacturer() }.to_string());

        results.push(DeviceInfo {
            id,
            name,
            model_id,
            manufacturer,
            formats: Vec::new(),
            formats_complete: false,
        });
    }

    Ok(results)
}

fn non_empty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn requested_output_resolution(
    request: &DeviceFormatRequest,
    delivered: VideoResolution,
) -> Option<VideoResolution> {
    let DeviceFormatRequest::Closest(format) = request else {
        return None;
    };
    if format.resolution == delivered {
        return None;
    }
    (resolution_area(format.resolution) <= resolution_area(delivered)).then_some(format.resolution)
}

fn resolution_area(resolution: VideoResolution) -> u64 {
    resolution.width as u64 * resolution.height as u64
}

struct SessionInner {
    session: Retained<AVCaptureSession>,
    _input: Retained<AVCaptureDeviceInput>,
    output: Retained<AVCaptureVideoDataOutput>,
    _delegate: Retained<CaptureDelegate>,
    _queue: DispatchRetained<DispatchQueue>,
    shared: Arc<FrameQueue>,
    device_name: String,
    device_id: String,
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        self.shared.stop();
        // SAFETY: The output and session are owned by this wrapper. Clearing
        // the delegate before stopping prevents callbacks from racing with
        // the delegate being released during teardown.
        unsafe {
            self.output.setSampleBufferDelegate_queue(None, None);
            self.session.stopRunning();
        }
    }
}

impl SessionInner {
    fn new(config: &DeviceVideoSourceConfig) -> Result<Self, DeviceVideoSourceError> {
        let device = select_device(&config.device)?;
        // SAFETY: These property getters return retained NSStrings for a
        // live AVCaptureDevice.
        let device_name = unsafe { device.localizedName() }.to_string();
        let device_id = unsafe { device.uniqueID() }.to_string();
        let session = unsafe { AVCaptureSession::new() };
        let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }.map_err(
            |err| DeviceVideoSourceError::Backend(err.localizedDescription().to_string()),
        )?;
        let output = unsafe { AVCaptureVideoDataOutput::new() };
        let shared = Arc::new(FrameQueue::default());
        let delegate = CaptureDelegate::new(shared.clone());
        let queue = DispatchQueue::new("io.livekit.capture.device", None);
        let active_format = select_active_format(&device, &config.format)?;

        // SAFETY: The session is newly created and not running. We add a
        // camera input and video data output only after canAdd* checks.
        unsafe {
            session.beginConfiguration();
            session.setAutomaticallyConfiguresCaptureDeviceForWideColor(false);
            if active_format.is_none() {
                if let Some(preset) = session_preset(&config.format) {
                    session.setSessionPreset(preset);
                }
            }
            let config_result = (|| {
                if !session.canAddInput(&input) {
                    return Err(DeviceVideoSourceError::Backend(
                        "capture device input could not be added".to_string(),
                    ));
                }
                session.addInput(&input);

                configure_device(&device, &config.format, active_format.as_deref())?;
                if active_format.is_some()
                    && session.canSetSessionPreset(AVCaptureSessionPresetInputPriority)
                {
                    session.setSessionPreset(AVCaptureSessionPresetInputPriority);
                }
                configure_input_frame_duration(&input, &device, &config.format);

                if let Some(video_settings) = preferred_video_settings(&output) {
                    output.setVideoSettings(Some(&video_settings));
                }
                output.setAlwaysDiscardsLateVideoFrames(true);
                output.setSampleBufferDelegate_queue(
                    Some(ProtocolObject::from_ref(&*delegate)),
                    Some(&queue),
                );
                if !session.canAddOutput(&output) {
                    return Err(DeviceVideoSourceError::Backend(
                        "video data output could not be added".to_string(),
                    ));
                }
                session.addOutput(&output);
                configure_output_connection(&output)?;
                Ok(())
            })();
            session.commitConfiguration();
            config_result?;
        }

        // SAFETY: Configuration has been committed and the session is ready
        // to synchronously start delivering video samples.
        unsafe {
            session.startRunning();
        }

        Ok(Self {
            session,
            _input: input,
            output,
            _delegate: delegate,
            _queue: queue,
            shared,
            device_name,
            device_id,
        })
    }

    fn wait_for_format(
        &self,
        timeout: Duration,
    ) -> Result<InitialFrameInfo, DeviceVideoSourceError> {
        self.shared.wait_for_format(timeout)
    }

    fn wait_take_queued_frame(
        &self,
        stop: &PumpStop,
    ) -> Result<Option<QueuedFrame>, DeviceVideoSourceError> {
        self.shared.wait_take_queued_frame(stop)
    }

    fn discard_pending_frame(&self) {
        self.shared.discard_latest();
    }
}

fn preferred_video_settings(
    output: &AVCaptureVideoDataOutput,
) -> Option<Retained<NSDictionary<NSString, AnyObject>>> {
    let preferred = [
        // WebRTC's VideoToolbox H.264 encoder allocates full-range NV12
        // buffers for its CPU upload path. Prefer the same CoreVideo
        // format for direct CVPixelBuffer input so the native path does
        // not have to reset VideoToolbox into a separate video-range pool.
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    ];
    // SAFETY: `output` is a live AVCaptureVideoDataOutput owned by the session setup path, and
    // querying advertised CV pixel formats does not mutate Rust-managed memory.
    let supported_formats = unsafe { output.availableVideoCVPixelFormatTypes() };
    let pixel_format_type = preferred
        .into_iter()
        .find(|preferred| supported_formats.iter().any(|format| format.as_u32() == *preferred))?;

    let pixel_format = NSNumber::new_u32(pixel_format_type);
    let metal_compatible = NSNumber::new_bool(true);
    let iosurface_properties = NSDictionary::<NSString, AnyObject>::new();
    // SAFETY: The CoreVideo constants are immutable CFString keys.
    // `CFString` and `NSString` are toll-free bridged, which
    // objc2-foundation exposes through `AsRef<NSString>`.
    let pixel_format_key: &NSString = unsafe { kCVPixelBufferPixelFormatTypeKey }.as_ref();
    // SAFETY: Same as above.
    let iosurface_key: &NSString = unsafe { kCVPixelBufferIOSurfacePropertiesKey }.as_ref();
    // SAFETY: Same as above.
    let metal_key: &NSString = unsafe { kCVPixelBufferMetalCompatibilityKey }.as_ref();
    Some(NSDictionary::from_slices(
        &[pixel_format_key, iosurface_key, metal_key],
        &[pixel_format.as_ref(), iosurface_properties.as_ref(), metal_compatible.as_ref()],
    ))
}

fn configure_input_frame_duration(
    input: &AVCaptureDeviceInput,
    device: &AVCaptureDevice,
    request: &DeviceFormatRequest,
) {
    let Some(framerate) = requested_framerate(request).filter(|framerate| *framerate > 0) else {
        return;
    };

    // AVCaptureDeviceInput's locked-frame-duration API is macOS 26.0+, while
    // the SDK builds against an older deployment target. Sending a selector the
    // running OS does not implement raises an Objective-C exception, which Rust
    // cannot catch and which therefore aborts the process, so probe first.
    if !input.respondsToSelector(sel!(isLockedVideoFrameDurationSupported))
        || !input.respondsToSelector(sel!(setActiveLockedVideoFrameDuration:))
    {
        return;
    }

    // SAFETY: `input` is the live input just added to the session, and the
    // selector was confirmed present above.
    if !unsafe { input.isLockedVideoFrameDurationSupported() } {
        return;
    }

    // SAFETY: `device` and `input` belong to the same session setup path, and
    // reading activeFormat is valid once the input has been added.
    let duration = unsafe { device_format_frame_duration(&device.activeFormat(), framerate) };
    let Some(duration) = duration else {
        return;
    };

    // SAFETY: `input` reports locked frame duration support, and `duration`
    // came from a frame-rate range of the device's active format.
    unsafe {
        input.setActiveLockedVideoFrameDuration(duration);
    }
}

fn configure_output_connection(
    output: &AVCaptureVideoDataOutput,
) -> Result<(), DeviceVideoSourceError> {
    let media_type = unsafe { AVMediaTypeVideo }.ok_or(DeviceVideoSourceError::DeviceNotFound)?;
    // SAFETY: `output` has just been added to a configured session. Querying
    // its video connection does not mutate Rust-managed memory.
    let Some(connection) = (unsafe { output.connectionWithMediaType(media_type) }) else {
        return Err(DeviceVideoSourceError::Backend(
            "video data output connection was not created".to_string(),
        ));
    };

    // Keep frame-duration control on the device/input path. The deprecated
    // output connection frame-duration setters can change whether macOS
    // delivers IOSurface-backed pixel buffers.
    // SAFETY: The connection is the video data output connection. Each
    // setter is guarded by the corresponding support/configuration checks
    // required by AVFoundation's API contract.
    unsafe {
        if connection.isVideoStabilizationSupported() {
            connection.setPreferredVideoStabilizationMode(AVCaptureVideoStabilizationMode::Off);
        }
        if connection.automaticallyAdjustsVideoMirroring() {
            connection.setAutomaticallyAdjustsVideoMirroring(false);
        }
        if connection.isVideoMirroringSupported() && connection.isVideoMirrored() {
            connection.setVideoMirrored(false);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct CaptureDelegateIvars {
    shared: Arc<FrameQueue>,
}

define_class!(
    // SAFETY:
    // - The superclass NSObject does not have subclassing requirements.
    // - CaptureDelegate does not implement Drop; retained Rust state lives in ivars.
    #[unsafe(super = NSObject)]
    #[thread_kind = AnyThread]
    #[ivars = CaptureDelegateIvars]
    struct CaptureDelegate;

    // SAFETY: `NSObjectProtocol` has no additional safety requirements.
    unsafe impl NSObjectProtocol for CaptureDelegate {}

    // SAFETY: The selector signatures match the generated AVFoundation protocol.
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for CaptureDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        #[allow(non_snake_case)]
        unsafe fn captureOutput_didOutputSampleBuffer_fromConnection(
            &self,
            _output: &AVCaptureOutput,
            sample_buffer: &CMSampleBuffer,
            _connection: &objc2_av_foundation::AVCaptureConnection,
        ) {
            if let Err(err) = process_sample_buffer(sample_buffer, &self.ivars().shared) {
                self.ivars().shared.set_error(err.to_string());
            }
        }
    }
);

impl CaptureDelegate {
    fn new(shared: Arc<FrameQueue>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(CaptureDelegateIvars { shared });
        // SAFETY: `this` is freshly allocated and initialized exactly once
        // using NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Latest-frame mailbox shared between the AVFoundation delegate queue and
/// the capturing thread.
#[derive(Debug)]
struct FrameQueue {
    state: Mutex<FrameQueueState>,
    ready: Condvar,
    started_at: Instant,
}

impl Default for FrameQueue {
    fn default() -> Self {
        Self {
            state: Mutex::new(FrameQueueState::default()),
            ready: Condvar::new(),
            started_at: Instant::now(),
        }
    }
}

#[derive(Debug, Default)]
struct FrameQueueState {
    latest: Option<QueuedFrame>,
    stopped: bool,
    error: Option<String>,
}

#[derive(Debug)]
struct InitialFrameInfo {
    format: DeviceFormat,
    native_frame_supported: bool,
}

impl FrameQueue {
    fn push_frame(&self, frame: QueuedFrame) {
        let mut state = self.state.lock().expect("device frame queue poisoned");
        if state.stopped {
            return;
        }
        state.latest = Some(frame);
        self.ready.notify_one();
    }

    fn set_error(&self, error: String) {
        let mut state = self.state.lock().expect("device frame queue poisoned");
        state.error = Some(error);
        self.ready.notify_all();
    }

    /// Signals session teardown and wakes every blocked frame wait.
    ///
    /// Stopping is idempotent. `push_frame` discards frames delivered after
    /// this point.
    fn stop(&self) {
        let mut state = self.state.lock().expect("device frame queue poisoned");
        state.stopped = true;
        self.ready.notify_all();
    }

    fn discard_latest(&self) {
        let mut state = self.state.lock().expect("device frame queue poisoned");
        state.latest = None;
    }

    fn wait_for_format(
        &self,
        timeout: Duration,
    ) -> Result<InitialFrameInfo, DeviceVideoSourceError> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().expect("device frame queue poisoned");
        loop {
            if let Some(frame) = state.latest.as_ref() {
                return Ok(InitialFrameInfo {
                    format: DeviceFormat::new(
                        VideoResolution::new(frame.width, frame.height),
                        0,
                        frame.source_format,
                    ),
                    native_frame_supported: frame.native_frame_supported(),
                });
            }
            if let Some(error) = state.error.take() {
                return Err(DeviceVideoSourceError::Backend(error));
            }
            if state.stopped {
                return Err(DeviceVideoSourceError::Backend(
                    "capture session stopped before delivering a frame".to_string(),
                ));
            }

            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(DeviceVideoSourceError::FrameTimeout);
            };
            let (next_state, _) =
                self.ready.wait_timeout(state, remaining).expect("device frame queue poisoned");
            state = next_state;
        }
    }

    /// Blocks until a frame, a delegate error, or a stop arrives and moves
    /// the frame out of the shared state, returning `Ok(None)` on stop.
    ///
    /// The state mutex guard is dropped when this returns, so callers convert
    /// the fully owned frame without holding the lock. Each wait is bounded
    /// by [`STOP_CHECK_INTERVAL`] so the stop token is observed promptly even
    /// when the device stalls without delivering frames or errors.
    fn wait_take_queued_frame(
        &self,
        stop: &PumpStop,
    ) -> Result<Option<QueuedFrame>, DeviceVideoSourceError> {
        let mut state = self.state.lock().expect("device frame queue poisoned");
        loop {
            if let Some(frame) = state.latest.take() {
                return Ok(Some(frame));
            }
            if let Some(error) = state.error.take() {
                return Err(DeviceVideoSourceError::Backend(error));
            }
            if state.stopped || stop.is_stopped() {
                return Ok(None);
            }
            let (next_state, _) = self
                .ready
                .wait_timeout(state, STOP_CHECK_INTERVAL)
                .expect("device frame queue poisoned");
            state = next_state;
        }
    }

    fn timestamp_us(&self) -> i64 {
        elapsed_us(self.started_at.elapsed())
    }
}

#[derive(Debug)]
struct QueuedFrame {
    pixel_buffer: RetainedPixelBuffer,
    width: u32,
    height: u32,
    source_format: DeviceFrameFormat,
    core_video_pixel_format: u32,
    // Wall-clock capture time: the validated sensor timestamp when
    // AVFoundation reports one, the read time otherwise.
    capture_wall_time_us: u64,
    timestamp_us: i64,
    is_iosurface_backed: bool,
}

impl QueuedFrame {
    fn into_i420_frame(self) -> Result<VideoFrame<I420Buffer>, DeviceVideoSourceError> {
        let buffer = convert_pixel_buffer(self.pixel_buffer.as_ref())?;
        Ok(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: self.timestamp_us,
            frame_metadata: Some(capture_frame_metadata(self.capture_wall_time_us)),
            buffer,
        })
    }

    fn into_native_frame(self) -> Result<VideoFrame<NativeBuffer>, DeviceVideoSourceError> {
        if !self.native_frame_supported() {
            return Err(DeviceVideoSourceError::Backend(
                "native capture requires an IOSurface-backed full-range NV12 buffer".to_string(),
            ));
        }

        let timestamp_us = self.timestamp_us;
        let capture_wall_time_us = self.capture_wall_time_us;
        let buffer = self.pixel_buffer.into_native_buffer();
        Ok(VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us,
            frame_metadata: Some(capture_frame_metadata(capture_wall_time_us)),
            buffer,
        })
    }

    fn native_frame_supported(&self) -> bool {
        self.source_format == DeviceFrameFormat::Nv12
            && self.core_video_pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
            && self.is_iosurface_backed
    }
}

fn pixel_buffer_has_iosurface(pixel_buffer: &CVPixelBuffer) -> bool {
    // SAFETY: `pixel_buffer` is a valid CVPixelBufferRef. CoreVideo returns
    // an unretained IOSurfaceRef; this code only checks for null and does
    // not store or release the returned pointer.
    !unsafe { CVPixelBufferGetIOSurface(pixel_buffer) }.is_null()
}

#[derive(Debug)]
struct RetainedPixelBuffer {
    ptr: NonNull<CVPixelBuffer>,
}

// SAFETY: `RetainedPixelBuffer` owns a +1 CoreFoundation reference to a
// CVPixelBuffer. CoreFoundation retain/release and CoreVideo pixel-buffer
// inspection are thread-safe for this usage, and mutable pixel access still
// goes through CoreVideo's lock/unlock API.
unsafe impl Send for RetainedPixelBuffer {}
// SAFETY: The wrapper exposes only shared access to the pixel buffer and
// releases its retained reference on drop.
unsafe impl Sync for RetainedPixelBuffer {}

impl RetainedPixelBuffer {
    fn from_image_buffer<T>(image_buffer: T) -> Self
    where
        T: Deref<Target = CVImageBuffer>,
    {
        let ptr = NonNull::from(&*image_buffer).cast::<CVPixelBuffer>();
        std::mem::forget(image_buffer);
        Self { ptr }
    }

    fn as_ref(&self) -> &CVPixelBuffer {
        // SAFETY: `ptr` was created from a retained CVImageBuffer returned
        // by CMSampleBufferGetImageBuffer and remains valid until this
        // wrapper drops or transfers that retain.
        unsafe { self.ptr.as_ref() }
    }

    fn into_native_buffer(self) -> NativeBuffer {
        let ptr = self.ptr.as_ptr().cast::<c_void>();
        std::mem::forget(self);
        // SAFETY: `ptr` is a valid retained CVPixelBufferRef. The WebRTC
        // bridge wraps it in RTCCVPixelBuffer and then releases the +1
        // retain we transfer here, so Rust must not release it afterwards.
        unsafe { NativeBuffer::from_cv_pixel_buffer(ptr) }
    }
}

impl Drop for RetainedPixelBuffer {
    fn drop(&mut self) {
        // SAFETY: `ptr` owns one CoreFoundation retain unless ownership was
        // transferred by `into_native_buffer`, which forgets `self`.
        unsafe { CFRelease(self.ptr.as_ptr().cast::<c_void>()) };
    }
}

fn select_device(
    selector: &DeviceSelector,
) -> Result<Retained<AVCaptureDevice>, DeviceVideoSourceError> {
    let media_type = unsafe { AVMediaTypeVideo }.ok_or(DeviceVideoSourceError::DeviceNotFound)?;
    match selector {
        DeviceSelector::Default => {
            unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) }
                .ok_or(DeviceVideoSourceError::DeviceNotFound)
        }
        DeviceSelector::Index(index) => {
            #[allow(deprecated)]
            let devices = unsafe { AVCaptureDevice::devicesWithMediaType(media_type) };
            devices
                .iter()
                .nth(*index)
                .map(|device| device.retain())
                .ok_or(DeviceVideoSourceError::DeviceNotFound)
        }
        DeviceSelector::Id(id) => {
            let id = NSString::from_str(id);
            unsafe { AVCaptureDevice::deviceWithUniqueID(&id) }
                .ok_or(DeviceVideoSourceError::DeviceNotFound)
        }
    }
}

fn select_active_format(
    device: &AVCaptureDevice,
    request: &DeviceFormatRequest,
) -> Result<Option<Retained<AVCaptureDeviceFormat>>, DeviceVideoSourceError> {
    match request {
        DeviceFormatRequest::Default => Ok(None),
        DeviceFormatRequest::Exact(format) => {
            let selected = best_device_format(
                device,
                Some(format.resolution),
                Some(format.framerate_fps),
                SelectionMode::Exact,
            );
            selected.map(Some).ok_or(DeviceVideoSourceError::UnsupportedFormat(*format))
        }
        DeviceFormatRequest::Closest(format) => Ok(best_device_format(
            device,
            Some(format.resolution),
            Some(format.framerate_fps),
            SelectionMode::Closest,
        )),
        DeviceFormatRequest::HighestFramerate { resolution, .. } => {
            Ok(best_device_format(device, *resolution, None, SelectionMode::HighestFramerate))
        }
        DeviceFormatRequest::HighestResolution { framerate_fps, .. } => {
            Ok(best_device_format(device, None, *framerate_fps, SelectionMode::HighestResolution))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionMode {
    Exact,
    Closest,
    HighestFramerate,
    HighestResolution,
}

#[derive(Debug)]
struct DeviceFormatCandidate {
    format: Retained<AVCaptureDeviceFormat>,
    resolution: VideoResolution,
    framerate_supported: bool,
    max_framerate: u32,
}

fn best_device_format(
    device: &AVCaptureDevice,
    resolution: Option<VideoResolution>,
    framerate: Option<u32>,
    mode: SelectionMode,
) -> Option<Retained<AVCaptureDeviceFormat>> {
    // SAFETY: The AVCaptureDevice is retained for the session setup path; querying the
    // immutable list of supported formats does not mutate Rust-managed memory.
    let formats = unsafe { device.formats() };
    let mut candidates = formats
        .iter()
        .filter_map(|format| {
            let candidate_resolution = device_format_resolution(&format)?;
            let framerate_supported = framerate
                .map(|framerate| device_format_supports_framerate(&format, framerate))
                .unwrap_or(true);
            Some(DeviceFormatCandidate {
                format: format.retain(),
                resolution: candidate_resolution,
                framerate_supported,
                max_framerate: device_format_max_framerate(&format),
            })
        })
        .collect::<Vec<_>>();

    if let Some(resolution) = resolution {
        if mode == SelectionMode::Exact {
            return candidates
                .into_iter()
                .find(|candidate| {
                    candidate.resolution == resolution && candidate.framerate_supported
                })
                .map(|candidate| candidate.format);
        }
    }

    if framerate.is_some() && candidates.iter().any(|candidate| candidate.framerate_supported) {
        candidates.retain(|candidate| candidate.framerate_supported);
    }

    match mode {
        SelectionMode::Exact => None,
        SelectionMode::Closest => {
            let resolution = resolution?;
            candidates
                .into_iter()
                .min_by_key(|candidate| resolution_distance(candidate.resolution, resolution))
                .map(|candidate| candidate.format)
        }
        SelectionMode::HighestFramerate => candidates
            .into_iter()
            .filter(|candidate| {
                resolution.map(|resolution| candidate.resolution == resolution).unwrap_or(true)
            })
            .max_by_key(|candidate| {
                (
                    candidate.max_framerate,
                    candidate.resolution.width as u64 * candidate.resolution.height as u64,
                )
            })
            .map(|candidate| candidate.format),
        SelectionMode::HighestResolution => candidates
            .into_iter()
            .max_by_key(|candidate| {
                (
                    candidate.resolution.width as u64 * candidate.resolution.height as u64,
                    candidate.max_framerate,
                )
            })
            .map(|candidate| candidate.format),
    }
}

fn device_format_resolution(format: &AVCaptureDeviceFormat) -> Option<VideoResolution> {
    // SAFETY: `format` is an AVCaptureDeviceFormat from the device's immutable formats array.
    // Its format description is a valid CMVideoFormatDescription for video capture formats.
    let description = unsafe { format.formatDescription() };
    // SAFETY: `description` is the video format description returned by AVFoundation.
    let dimensions = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
    if dimensions.width <= 0 || dimensions.height <= 0 {
        return None;
    }
    Some(VideoResolution::new(dimensions.width as u32, dimensions.height as u32))
}

fn device_format_supports_framerate(format: &AVCaptureDeviceFormat, framerate: u32) -> bool {
    device_format_frame_duration(format, framerate).is_some()
}

/// Frame duration to apply for `framerate` on `format`, or `None` when no
/// frame-rate range covers it.
///
/// The duration is taken from the matched range's own bounds instead of being
/// derived from `framerate` alone. AVFoundation raises an Objective-C
/// exception — which aborts the process, since Rust cannot catch it — for any
/// duration outside a range's `[minFrameDuration, maxFrameDuration]`, and
/// devices commonly advertise near-integral rates whose exact duration is not
/// the reciprocal of the rounded rate. A UVC camera reporting 30.00003 fps
/// accepts 1/30.00003 s but rejects 1/30 s.
fn device_format_frame_duration(format: &AVCaptureDeviceFormat, framerate: u32) -> Option<CMTime> {
    let requested = framerate as f64;
    // SAFETY: `format` is an AVCaptureDeviceFormat from the device's immutable formats array.
    // The returned frame-rate ranges are immutable AVFoundation objects.
    unsafe { format.videoSupportedFrameRateRanges() }.iter().find_map(|range| {
        // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
        let min = unsafe { range.minFrameRate() };
        // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
        let max = unsafe { range.maxFrameRate() };
        if requested < min.floor() || requested > max.ceil() {
            return None;
        }
        // Rate and duration are inverses, so the slowest rate carries the
        // longest duration. Snapping to an endpoint keeps a rounded request
        // inside the bounds the format actually accepts.
        Some(if requested <= min {
            // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
            unsafe { range.maxFrameDuration() }
        } else if requested >= max {
            // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
            unsafe { range.minFrameDuration() }
        } else {
            // The rate lies strictly inside the range, so its reciprocal lies
            // strictly inside the range's duration bounds.
            // SAFETY: `requested` is finite and greater than zero here.
            unsafe { CMTime::with_seconds(1.0 / requested, 600) }
        })
    })
}

fn device_format_max_framerate(format: &AVCaptureDeviceFormat) -> u32 {
    // SAFETY: `format` is an AVCaptureDeviceFormat from the device's immutable formats array.
    // The returned frame-rate ranges are immutable AVFoundation objects.
    unsafe { format.videoSupportedFrameRateRanges() }
        .iter()
        .map(|range| {
            // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
            unsafe { range.maxFrameRate() }.floor().max(0.0) as u32
        })
        .max()
        .unwrap_or_default()
}

fn resolution_distance(actual: VideoResolution, requested: VideoResolution) -> u64 {
    let width_delta = actual.width.abs_diff(requested.width) as u64;
    let height_delta = actual.height.abs_diff(requested.height) as u64;
    let pixel_delta = (actual.width as u64 * actual.height as u64)
        .abs_diff(requested.width as u64 * requested.height as u64);
    pixel_delta + width_delta * width_delta + height_delta * height_delta
}

fn configure_device(
    device: &AVCaptureDevice,
    request: &DeviceFormatRequest,
    active_format: Option<&AVCaptureDeviceFormat>,
) -> Result<(), DeviceVideoSourceError> {
    let framerate = requested_framerate(request);
    if active_format.is_none() && framerate.is_none() {
        return Ok(());
    }

    unsafe { device.lockForConfiguration() }
        .map_err(|err| DeviceVideoSourceError::Backend(err.localizedDescription().to_string()))?;

    let configure_result = configure_locked_device(device, active_format, framerate);
    // SAFETY: The device was successfully locked above and must be unlocked exactly once.
    unsafe {
        device.unlockForConfiguration();
    }
    configure_result
}

fn configure_locked_device(
    device: &AVCaptureDevice,
    active_format: Option<&AVCaptureDeviceFormat>,
    framerate: Option<u32>,
) -> Result<(), DeviceVideoSourceError> {
    // SAFETY: The caller holds the AVCaptureDevice configuration lock, and `active_format`
    // was selected from this device's formats array.
    unsafe {
        if let Some(active_format) = active_format {
            device.setActiveFormat(active_format);
        }
    }
    configure_low_latency_device_processing(device);

    let Some(framerate) = framerate.filter(|framerate| *framerate > 0) else {
        return Ok(());
    };

    let active_format = match active_format {
        Some(active_format) => active_format.retain(),
        // SAFETY: The caller holds the configuration lock, and reading activeFormat is valid.
        None => unsafe { device.activeFormat() },
    };
    let Some(duration) = device_format_frame_duration(&active_format, framerate) else {
        return Ok(());
    };

    // SAFETY: The device is locked for configuration and `duration` came from a
    // frame-rate range of the format now active on the device.
    unsafe {
        device.setActiveVideoMinFrameDuration(duration);
        device.setActiveVideoMaxFrameDuration(duration);
    }
    Ok(())
}

fn configure_low_latency_device_processing(device: &AVCaptureDevice) {
    // SAFETY: The caller holds the AVCaptureDevice configuration lock.
    // Setters are guarded by their support/current-state predicates where
    // AVFoundation requires that.
    unsafe {
        if device.automaticallyAdjustsVideoHDREnabled() {
            device.setAutomaticallyAdjustsVideoHDREnabled(false);
        }
        if device.isVideoHDREnabled() {
            device.setVideoHDREnabled(false);
        }
        if device.isLowLightBoostSupported()
            && device.automaticallyEnablesLowLightBoostWhenAvailable()
        {
            device.setAutomaticallyEnablesLowLightBoostWhenAvailable(false);
        }
        if device.isSmoothAutoFocusSupported() && device.isSmoothAutoFocusEnabled() {
            device.setSmoothAutoFocusEnabled(false);
        }
    }
}

fn requested_framerate(request: &DeviceFormatRequest) -> Option<u32> {
    match request {
        DeviceFormatRequest::Default => None,
        DeviceFormatRequest::Exact(format) | DeviceFormatRequest::Closest(format) => {
            Some(format.framerate_fps)
        }
        DeviceFormatRequest::HighestFramerate { .. } => None,
        DeviceFormatRequest::HighestResolution { framerate_fps, .. } => *framerate_fps,
    }
}

fn session_preset(
    request: &DeviceFormatRequest,
) -> Option<&'static objc2_av_foundation::AVCaptureSessionPreset> {
    let resolution = match request {
        DeviceFormatRequest::Exact(format) | DeviceFormatRequest::Closest(format) => {
            Some(format.resolution)
        }
        DeviceFormatRequest::HighestFramerate { resolution, .. } => *resolution,
        DeviceFormatRequest::Default | DeviceFormatRequest::HighestResolution { .. } => None,
    }?;

    exact_session_preset(resolution).or(Some(unsafe { AVCaptureSessionPresetHigh }))
}

fn exact_session_preset(
    resolution: VideoResolution,
) -> Option<&'static objc2_av_foundation::AVCaptureSessionPreset> {
    match (resolution.width, resolution.height) {
        (1920, 1080) => Some(unsafe { AVCaptureSessionPreset1920x1080 }),
        (1280, 720) => Some(unsafe { AVCaptureSessionPreset1280x720 }),
        (640, 480) => Some(unsafe { AVCaptureSessionPreset640x480 }),
        (w, h) if w <= 640 && h <= 480 => Some(unsafe { AVCaptureSessionPresetMedium }),
        _ => None,
    }
}

fn process_sample_buffer(
    sample_buffer: &CMSampleBuffer,
    shared: &FrameQueue,
) -> Result<(), DeviceVideoSourceError> {
    let read_wall_time_us = unix_time_us_now().unwrap_or_default();
    let sensor_timestamp_us = sample_buffer_capture_wall_time_us(sample_buffer, read_wall_time_us);
    let image_buffer = unsafe { sample_buffer.image_buffer() }
        .ok_or(DeviceVideoSourceError::InvalidFrame("sample buffer has no image buffer"))?;
    let pixel_buffer = RetainedPixelBuffer::from_image_buffer(image_buffer);
    let pixel_buffer_ref = pixel_buffer.as_ref();
    let width = u32::try_from(CVPixelBufferGetWidth(pixel_buffer_ref))
        .map_err(|_| DeviceVideoSourceError::InvalidFrame("width is out of range"))?;
    let height = u32::try_from(CVPixelBufferGetHeight(pixel_buffer_ref))
        .map_err(|_| DeviceVideoSourceError::InvalidFrame("height is out of range"))?;
    let core_video_pixel_format = CVPixelBufferGetPixelFormatType(pixel_buffer_ref);
    let source_format = frame_format_from_core_video(core_video_pixel_format)?;
    let is_iosurface_backed = pixel_buffer_has_iosurface(pixel_buffer_ref);

    let capture_wall_time_us = sensor_timestamp_us.unwrap_or(read_wall_time_us);
    shared.push_frame(QueuedFrame {
        pixel_buffer,
        width,
        height,
        source_format,
        core_video_pixel_format,
        capture_wall_time_us,
        timestamp_us: shared.timestamp_us(),
        is_iosurface_backed,
    });
    Ok(())
}

fn sample_buffer_capture_wall_time_us(
    sample_buffer: &CMSampleBuffer,
    read_wall_time_us: u64,
) -> Option<u64> {
    let sample_time = unsafe { sample_buffer.presentation_time_stamp() };

    let timestamp_us = cm_time_to_us(sample_time)?;
    if validate_capture_timestamp_us(timestamp_us, read_wall_time_us).is_some() {
        return Some(timestamp_us);
    }

    let host_now_us = current_host_time_us()?;
    let age_us = host_now_us.checked_sub(timestamp_us)?;
    if age_us > MAX_CAPTURE_TIMESTAMP_AGE_US {
        return None;
    }
    read_wall_time_us.checked_sub(age_us)
}

fn current_host_time_us() -> Option<u64> {
    // SAFETY: The CoreMedia host time clock is a process-wide singleton and
    // reading it does not mutate Rust-managed memory.
    let host_clock = unsafe { CMClock::host_time_clock() };
    // SAFETY: `host_clock` is a valid retained CoreMedia clock.
    let host_time = unsafe { host_clock.time() };
    cm_time_to_us(host_time)
}

fn cm_time_to_us(time: CMTime) -> Option<u64> {
    let flags = time.flags;
    if !flags.contains(CMTimeFlags::Valid) || flags.intersects(CMTimeFlags::ImpliedValueFlagsMask) {
        return None;
    }

    // SAFETY: `time` is a valid CMTime value returned by CoreMedia. Invalid
    // and indefinite values were filtered above.
    let seconds = unsafe { time.seconds() };
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }

    let micros = seconds * 1_000_000.0;
    (micros <= u64::MAX as f64).then_some(micros.round() as u64)
}

fn convert_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    let lock_flags = CVPixelBufferLockFlags::ReadOnly;
    let lock_result = unsafe { CVPixelBufferLockBaseAddress(pixel_buffer, lock_flags) };
    if lock_result != kCVReturnSuccess {
        return Err(DeviceVideoSourceError::InvalidFrame("CVPixelBuffer lock failed"));
    }

    let result = convert_locked_pixel_buffer(pixel_buffer);

    // SAFETY: The pixel buffer was locked above with the same flags.
    let unlock_result = unsafe { CVPixelBufferUnlockBaseAddress(pixel_buffer, lock_flags) };
    if unlock_result != kCVReturnSuccess {
        return Err(DeviceVideoSourceError::InvalidFrame("CVPixelBuffer unlock failed"));
    }

    result
}

fn convert_locked_pixel_buffer(
    pixel_buffer: &CVPixelBuffer,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    let width = u32::try_from(CVPixelBufferGetWidth(pixel_buffer))
        .map_err(|_| DeviceVideoSourceError::InvalidFrame("width is out of range"))?;
    let height = u32::try_from(CVPixelBufferGetHeight(pixel_buffer))
        .map_err(|_| DeviceVideoSourceError::InvalidFrame("height is out of range"))?;
    let source_format =
        frame_format_from_core_video(CVPixelBufferGetPixelFormatType(pixel_buffer))?;

    match source_format {
        DeviceFrameFormat::Nv12 => convert_nv12(pixel_buffer, width, height),
        DeviceFrameFormat::Bgra => convert_bgra(pixel_buffer, width, height),
        DeviceFrameFormat::I420 => convert_i420(pixel_buffer, width, height),
        DeviceFrameFormat::Uyvy => convert_uyvy(pixel_buffer, width, height),
        DeviceFrameFormat::Yuyv => convert_yuy2(pixel_buffer, width, height),
        other => Err(DeviceVideoSourceError::UnsupportedFrameFormat(other)),
    }
}

fn frame_format_from_core_video(
    pixel_format: u32,
) -> Result<DeviceFrameFormat, DeviceVideoSourceError> {
    match pixel_format {
        format
            if format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
                || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange =>
        {
            Ok(DeviceFrameFormat::Nv12)
        }
        format if format == kCVPixelFormatType_32BGRA => Ok(DeviceFrameFormat::Bgra),
        format
            if format == kCVPixelFormatType_420YpCbCr8Planar
                || format == kCVPixelFormatType_420YpCbCr8PlanarFullRange =>
        {
            Ok(DeviceFrameFormat::I420)
        }
        format if format == kCVPixelFormatType_422YpCbCr8 => Ok(DeviceFrameFormat::Uyvy),
        format
            if format == kCVPixelFormatType_422YpCbCr8_yuvs
                || format == kCVPixelFormatType_422YpCbCr8FullRange =>
        {
            Ok(DeviceFrameFormat::Yuyv)
        }
        other => Err(DeviceVideoSourceError::Backend(format!(
            "unsupported CoreVideo pixel format 0x{other:08x}"
        ))),
    }
}

fn convert_nv12(
    pixel_buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    if CVPixelBufferGetPlaneCount(pixel_buffer) < 2 {
        return Err(DeviceVideoSourceError::InvalidFrame("NV12 buffer has fewer than two planes"));
    }

    let y = plane(pixel_buffer, 0)?;
    let uv = plane(pixel_buffer, 1)?;
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slices cover the locked CVPixelBuffer planes for the duration of this
    // call, and the destination planes come from a freshly allocated I420Buffer with matching
    // width, height, and strides.
    let ret = unsafe {
        yuv_sys::rs_NV12ToI420(
            y.data.as_ptr(),
            y.stride as i32,
            uv.data.as_ptr(),
            uv.stride as i32,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width as i32,
            height as i32,
        )
    };
    if ret != 0 {
        return Err(DeviceVideoSourceError::Convert("NV12ToI420 failed"));
    }
    Ok(buffer)
}

fn convert_bgra(
    pixel_buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    let bgra = packed_plane(pixel_buffer, 4)?;
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slice covers the locked CVPixelBuffer for the duration of this call,
    // and the destination planes come from a freshly allocated I420Buffer with matching
    // width, height, and strides.
    let ret = unsafe {
        yuv_sys::rs_BGRAToI420(
            bgra.data.as_ptr(),
            bgra.stride as i32,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width as i32,
            height as i32,
        )
    };
    if ret != 0 {
        return Err(DeviceVideoSourceError::Convert("BGRAToI420 failed"));
    }
    Ok(buffer)
}

fn convert_uyvy(
    pixel_buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    let uyvy = packed_plane(pixel_buffer, 2)?;
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slice covers the locked CVPixelBuffer for the duration of this call,
    // and the destination planes come from a freshly allocated I420Buffer with matching
    // width, height, and strides.
    let ret = unsafe {
        yuv_sys::rs_UYVYToI420(
            uyvy.data.as_ptr(),
            uyvy.stride as i32,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width as i32,
            height as i32,
        )
    };
    if ret != 0 {
        return Err(DeviceVideoSourceError::Convert("UYVYToI420 failed"));
    }
    Ok(buffer)
}

fn convert_yuy2(
    pixel_buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    let yuy2 = packed_plane(pixel_buffer, 2)?;
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slice covers the locked CVPixelBuffer for the duration of this call,
    // and the destination planes come from a freshly allocated I420Buffer with matching
    // width, height, and strides.
    let ret = unsafe {
        yuv_sys::rs_YUY2ToI420(
            yuy2.data.as_ptr(),
            yuy2.stride as i32,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width as i32,
            height as i32,
        )
    };
    if ret != 0 {
        return Err(DeviceVideoSourceError::Convert("YUY2ToI420 failed"));
    }
    Ok(buffer)
}

fn convert_i420(
    pixel_buffer: &CVPixelBuffer,
    width: u32,
    height: u32,
) -> Result<I420Buffer, DeviceVideoSourceError> {
    if CVPixelBufferGetPlaneCount(pixel_buffer) < 3 {
        return Err(DeviceVideoSourceError::InvalidFrame(
            "I420 buffer has fewer than three planes",
        ));
    }

    let y = plane(pixel_buffer, 0)?;
    let u = plane(pixel_buffer, 1)?;
    let v = plane(pixel_buffer, 2)?;
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, stride_v) = buffer.strides();
    let (dst_y, dst_u, dst_v) = buffer.data_mut();
    // SAFETY: The source slices cover the locked CVPixelBuffer planes for the duration of this
    // call, and the destination planes come from a freshly allocated I420Buffer with matching
    // width, height, and strides.
    let ret = unsafe {
        yuv_sys::rs_I420Copy(
            y.data.as_ptr(),
            y.stride as i32,
            u.data.as_ptr(),
            u.stride as i32,
            v.data.as_ptr(),
            v.stride as i32,
            dst_y.as_mut_ptr(),
            stride_y as i32,
            dst_u.as_mut_ptr(),
            stride_u as i32,
            dst_v.as_mut_ptr(),
            stride_v as i32,
            width as i32,
            height as i32,
        )
    };
    if ret != 0 {
        return Err(DeviceVideoSourceError::Convert("I420Copy failed"));
    }
    Ok(buffer)
}

struct Plane<'a> {
    data: &'a [u8],
    stride: usize,
}

fn plane(pixel_buffer: &CVPixelBuffer, index: usize) -> Result<Plane<'_>, DeviceVideoSourceError> {
    let plane_count = CVPixelBufferGetPlaneCount(pixel_buffer);
    if index >= plane_count {
        return Err(DeviceVideoSourceError::InvalidFrame("plane index is out of range"));
    }

    let base = CVPixelBufferGetBaseAddressOfPlane(pixel_buffer, index);
    if base.is_null() {
        return Err(DeviceVideoSourceError::InvalidFrame("pixel plane has no base address"));
    }
    let stride = CVPixelBufferGetBytesPerRowOfPlane(pixel_buffer, index);
    let height = CVPixelBufferGetHeightOfPlane(pixel_buffer, index);
    let width = CVPixelBufferGetWidthOfPlane(pixel_buffer, index);
    let min_len = stride
        .checked_mul(height.saturating_sub(1))
        .and_then(|value| value.checked_add(width))
        .ok_or(DeviceVideoSourceError::InvalidFrame("pixel plane size overflow"))?;

    // SAFETY: The CVPixelBuffer is locked for read-only access, the plane
    // base address is non-null, and CoreVideo reports the minimum readable
    // extent for this plane.
    let data = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), min_len) };
    Ok(Plane { data, stride })
}

fn packed_plane(
    pixel_buffer: &CVPixelBuffer,
    bytes_per_pixel: usize,
) -> Result<Plane<'_>, DeviceVideoSourceError> {
    let base = CVPixelBufferGetBaseAddress(pixel_buffer);
    if base.is_null() {
        return Err(DeviceVideoSourceError::InvalidFrame("pixel buffer has no base address"));
    }
    let stride = CVPixelBufferGetBytesPerRow(pixel_buffer);
    let height = CVPixelBufferGetHeight(pixel_buffer);
    let width = CVPixelBufferGetWidth(pixel_buffer)
        .checked_mul(bytes_per_pixel)
        .ok_or(DeviceVideoSourceError::InvalidFrame("packed pixel row size overflow"))?;
    let min_len = stride
        .checked_mul(height.saturating_sub(1))
        .and_then(|value| value.checked_add(width))
        .ok_or(DeviceVideoSourceError::InvalidFrame("packed pixel buffer size overflow"))?;

    // SAFETY: The CVPixelBuffer is locked for read-only access, the base
    // address is non-null, and CoreVideo reports the minimum readable extent
    // for this packed buffer.
    let data = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), min_len) };
    Ok(Plane { data, stride })
}

#[cfg(test)]
mod tests {
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    use super::{FrameQueue, STOP_CHECK_INTERVAL};
    use crate::pump::PumpStop;

    /// Upper bound on how long a woken frame wait may take to return before
    /// the test declares the stop path broken.
    const STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    // `FrameQueue` is pure Rust state, so these tests run on macOS CI hosts
    // without camera hardware or AVFoundation involvement.

    #[test]
    fn stop_token_unblocks_frame_wait() {
        let queue = Arc::new(FrameQueue::default());
        let stop = PumpStop::new();

        let (done_tx, done_rx) = mpsc::channel();
        let stop_in_waiter = stop.clone();
        let queue_in_waiter = Arc::clone(&queue);
        let waiter = std::thread::spawn(move || {
            let result = queue_in_waiter.wait_take_queued_frame(&stop_in_waiter);
            let _ = done_tx.send(());
            result
        });

        // Give the waiter time to block. There is no race if the stop lands
        // first: the wait loop re-checks the token at least every
        // STOP_CHECK_INTERVAL.
        std::thread::sleep(Duration::from_millis(50));
        stop.stop();

        done_rx
            .recv_timeout(STOP_WAIT_TIMEOUT + STOP_CHECK_INTERVAL)
            .expect("frame wait did not return after the stop token fired");
        let result = waiter.join().expect("frame wait thread panicked");
        assert!(matches!(result, Ok(None)), "unexpected frame wait result: {result:?}");
    }

    #[test]
    fn frame_waits_return_none_once_queue_stopped() {
        let queue = FrameQueue::default();
        queue.stop();
        // Stopping is idempotent.
        queue.stop();

        assert!(matches!(queue.wait_take_queued_frame(&PumpStop::new()), Ok(None)));
    }

    #[test]
    fn delegate_errors_surface_from_frame_wait() {
        let queue = FrameQueue::default();
        queue.set_error("camera unplugged".to_string());

        let error = queue
            .wait_take_queued_frame(&PumpStop::new())
            .expect_err("delegate error must surface");
        assert!(error.to_string().contains("camera unplugged"));
    }
}
