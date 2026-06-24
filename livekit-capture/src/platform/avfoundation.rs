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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;

use livekit::webrtc::video_frame::{I420Buffer, VideoFrame};
use thiserror::Error;

use crate::{
    device::{
        CaptureDeviceInfo, CaptureDeviceSelector, CaptureFormat, CaptureFormatRequest,
        CapturePixelFormat, CaptureResolution,
    },
    error::CaptureError,
    track::VideoCaptureTrack,
};

#[cfg(target_os = "macos")]
const FIRST_FRAME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Options used to create an AVFoundation capture session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvFoundationCaptureOptions {
    /// Device to use for capture.
    pub device: CaptureDeviceSelector,
    /// Format requested from the device.
    pub format: CaptureFormatRequest,
    /// Whether the resulting track should be marked as a screencast.
    pub is_screencast: bool,
}

impl Default for AvFoundationCaptureOptions {
    fn default() -> Self {
        Self {
            device: CaptureDeviceSelector::Default,
            format: CaptureFormatRequest::Default,
            is_screencast: false,
        }
    }
}

/// One AVFoundation frame converted to I420.
#[derive(Debug)]
pub struct AvFoundationFrame {
    /// Decoded I420 frame suitable for [`crate::VideoCaptureTrack::capture_frame`].
    pub frame: VideoFrame<I420Buffer>,
    /// Source pixel format delivered by AVFoundation.
    pub source_pixel_format: CapturePixelFormat,
    /// Wall-clock timestamp selected for metadata and timing correlation.
    pub capture_wall_time_us: u64,
    /// Wall-clock timestamp recorded after the frame was read from AVFoundation.
    pub read_wall_time_us: u64,
    /// Whether compressed image decoding was needed.
    pub used_decode_path: bool,
}

impl AvFoundationFrame {
    /// Returns the decoded video frame.
    pub fn video_frame(&self) -> &VideoFrame<I420Buffer> {
        &self.frame
    }
}

/// AVFoundation decoded-frame capture session that emits I420 frames.
pub struct AvFoundationCaptureSession {
    format: CaptureFormat,
    #[cfg(target_os = "macos")]
    inner: macos::SessionInner,
}

impl std::fmt::Debug for AvFoundationCaptureSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvFoundationCaptureSession").field("format", &self.format).finish()
    }
}

// SAFETY: `AvFoundationCaptureSession` owns AVFoundation objects and only exposes
// `&mut self` frame capture plus `Drop`; moving ownership to another thread does
// not create concurrent access to those Objective-C objects.
#[cfg(target_os = "macos")]
unsafe impl Send for AvFoundationCaptureSession {}

impl AvFoundationCaptureSession {
    /// Opens an AVFoundation decoded-frame capture session.
    pub fn new(options: AvFoundationCaptureOptions) -> Result<Self, AvFoundationError> {
        validate_options(&options)?;
        Self::open(options)
    }

    /// Captures the next decoded frame and converts it to I420.
    pub fn capture_frame(&mut self) -> Result<AvFoundationFrame, AvFoundationError> {
        self.capture_frame_inner()
    }

    /// Returns the negotiated capture format.
    pub fn format(&self) -> CaptureFormat {
        self.format
    }

    #[cfg(target_os = "macos")]
    fn open(options: AvFoundationCaptureOptions) -> Result<Self, AvFoundationError> {
        let inner = macos::SessionInner::new(&options)?;
        let mut format = inner.wait_for_format(FIRST_FRAME_TIMEOUT)?;
        format.frame_rate = requested_frame_rate_hint(&options.format).unwrap_or(30);
        Ok(Self { format, inner })
    }

    #[cfg(not(target_os = "macos"))]
    fn open(_options: AvFoundationCaptureOptions) -> Result<Self, AvFoundationError> {
        Err(AvFoundationError::UnsupportedPlatform)
    }

    #[cfg(target_os = "macos")]
    fn capture_frame_inner(&mut self) -> Result<AvFoundationFrame, AvFoundationError> {
        self.inner.capture_frame()
    }

    #[cfg(not(target_os = "macos"))]
    fn capture_frame_inner(&mut self) -> Result<AvFoundationFrame, AvFoundationError> {
        Err(AvFoundationError::UnsupportedPlatform)
    }
}

/// AVFoundation decoded-frame capture session that forwards frames into a track.
pub struct AvFoundationCapture {
    track: VideoCaptureTrack,
    options: AvFoundationCaptureOptions,
    runner: Option<CaptureRunner>,
}

impl std::fmt::Debug for AvFoundationCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvFoundationCapture")
            .field("track", &self.track)
            .field("options", &self.options)
            .field("running", &self.runner.is_some())
            .finish()
    }
}

impl AvFoundationCapture {
    /// Creates an AVFoundation capture session wrapper for a capture track.
    pub fn new(
        track: VideoCaptureTrack,
        options: AvFoundationCaptureOptions,
    ) -> Result<Self, AvFoundationError> {
        ensure_platform_available()?;
        Ok(Self { track, options, runner: None })
    }

    /// Returns the capture track that receives decoded frames.
    pub fn track(&self) -> &VideoCaptureTrack {
        &self.track
    }

    /// Returns the configured capture options.
    pub fn options(&self) -> &AvFoundationCaptureOptions {
        &self.options
    }

    /// Starts AVFoundation capture on a background thread.
    pub fn start(&mut self) -> Result<(), AvFoundationError> {
        start_capture(self)
    }

    /// Stops AVFoundation capture.
    pub fn stop(&mut self) -> Result<(), AvFoundationError> {
        stop_capture(self)
    }
}

impl Drop for AvFoundationCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Debug)]
struct CaptureRunner {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// Lists AVFoundation video capture devices.
pub fn devices() -> Result<Vec<CaptureDeviceInfo>, AvFoundationError> {
    list_devices()
}

/// Error returned by AVFoundation capture.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AvFoundationError {
    /// AVFoundation capture is only available on macOS.
    #[error("AVFoundation capture is only available on macOS")]
    UnsupportedPlatform,
    /// The requested device was not found.
    #[error("AVFoundation capture device was not found")]
    DeviceNotFound,
    /// The requested option is invalid.
    #[error("invalid AVFoundation capture option: {0}")]
    InvalidOption(&'static str),
    /// The requested capture format is not supported by this backend.
    #[error("AVFoundation capture does not support pixel format {0:?}")]
    UnsupportedPixelFormat(CapturePixelFormat),
    /// The requested capture format is not available on the selected device.
    #[error("AVFoundation capture format is not available: {0:?}")]
    UnsupportedFormat(CaptureFormat),
    /// AVFoundation could not configure the capture session.
    #[error("AVFoundation session setup failed: {0}")]
    SessionSetup(String),
    /// Timed out waiting for AVFoundation to deliver a frame.
    #[error("timed out waiting for AVFoundation frame")]
    FrameTimeout,
    /// The capture session is already running.
    #[error("AVFoundation capture is already running")]
    AlreadyRunning,
    /// The capture session is not running.
    #[error("AVFoundation capture is not running")]
    NotRunning,
    /// Captured frame bytes did not match the negotiated format.
    #[error("invalid AVFoundation frame buffer: {0}")]
    InvalidFrame(&'static str),
    /// AVFoundation produced a pixel format this backend cannot convert yet.
    #[error("unsupported AVFoundation pixel format 0x{0:08x}")]
    UnsupportedCoreVideoPixelFormat(u32),
    /// Pixel conversion failed.
    #[error("failed to convert AVFoundation frame to I420: {0}")]
    Convert(&'static str),
    /// AVFoundation reported a runtime capture error.
    #[error("AVFoundation runtime error: {0}")]
    Runtime(String),
    /// The shared capture track rejected a frame.
    #[error(transparent)]
    Capture(#[from] CaptureError),
}

fn validate_options(options: &AvFoundationCaptureOptions) -> Result<(), AvFoundationError> {
    match &options.device {
        CaptureDeviceSelector::Default | CaptureDeviceSelector::Index(_) => {}
        CaptureDeviceSelector::Id(id) => {
            if id.is_empty() {
                return Err(AvFoundationError::InvalidOption("device id must be non-empty"));
            }
        }
    }

    validate_format_request(&options.format)
}

fn validate_format_request(format: &CaptureFormatRequest) -> Result<(), AvFoundationError> {
    let validate_format = |format: &CaptureFormat| {
        if format.resolution.width == 0 {
            return Err(AvFoundationError::InvalidOption("width must be non-zero"));
        }
        if format.resolution.height == 0 {
            return Err(AvFoundationError::InvalidOption("height must be non-zero"));
        }
        if format.frame_rate == 0 {
            return Err(AvFoundationError::InvalidOption("frame_rate must be non-zero"));
        }
        validate_pixel_format(format.pixel_format)?;
        Ok(())
    };

    match format {
        CaptureFormatRequest::Default => Ok(()),
        CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
            validate_format(format)
        }
        CaptureFormatRequest::HighestFrameRate { resolution, pixel_format } => {
            if let Some(resolution) = resolution {
                validate_resolution(*resolution)?;
            }
            if let Some(pixel_format) = pixel_format {
                validate_pixel_format(*pixel_format)?;
            }
            Ok(())
        }
        CaptureFormatRequest::HighestResolution { frame_rate, pixel_format } => {
            if matches!(frame_rate, Some(0)) {
                return Err(AvFoundationError::InvalidOption("frame_rate must be non-zero"));
            }
            if let Some(pixel_format) = pixel_format {
                validate_pixel_format(*pixel_format)?;
            }
            Ok(())
        }
    }
}

fn validate_pixel_format(pixel_format: CapturePixelFormat) -> Result<(), AvFoundationError> {
    if !matches!(
        pixel_format,
        CapturePixelFormat::Nv12 | CapturePixelFormat::Bgra | CapturePixelFormat::I420
    ) {
        return Err(AvFoundationError::UnsupportedPixelFormat(pixel_format));
    }
    Ok(())
}

fn requested_frame_rate_hint(format: &CaptureFormatRequest) -> Option<u32> {
    match format {
        CaptureFormatRequest::Default => None,
        CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
            Some(format.frame_rate)
        }
        CaptureFormatRequest::HighestFrameRate { .. } => None,
        CaptureFormatRequest::HighestResolution { frame_rate, .. } => *frame_rate,
    }
}

fn validate_resolution(resolution: CaptureResolution) -> Result<(), AvFoundationError> {
    if resolution.width == 0 {
        return Err(AvFoundationError::InvalidOption("width must be non-zero"));
    }
    if resolution.height == 0 {
        return Err(AvFoundationError::InvalidOption("height must be non-zero"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_platform_available() -> Result<(), AvFoundationError> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn ensure_platform_available() -> Result<(), AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn list_devices() -> Result<Vec<CaptureDeviceInfo>, AvFoundationError> {
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeVideo};

    // SAFETY: AVMediaTypeVideo is a framework-provided immutable NSString
    // constant. We only borrow it to ask AVFoundation for video devices.
    let media_type = unsafe { AVMediaTypeVideo }.ok_or(AvFoundationError::DeviceNotFound)?;
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

        results.push(CaptureDeviceInfo { id, name, model_id, manufacturer, formats: Vec::new() });
    }

    Ok(results)
}

#[cfg(not(target_os = "macos"))]
fn list_devices() -> Result<Vec<CaptureDeviceInfo>, AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn non_empty_string(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "macos")]
fn start_capture(capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    if capture.runner.is_some() {
        return Err(AvFoundationError::AlreadyRunning);
    }

    let track = capture.track.clone();
    let mut session = AvFoundationCaptureSession::new(capture.options.clone())?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let handle = std::thread::Builder::new()
        .name("avfoundation-capture".into())
        .spawn(move || {
            while !stop_for_thread.load(Ordering::Acquire) {
                match session.capture_frame() {
                    Ok(frame) => track.capture_frame(&frame.frame),
                    Err(_) => break,
                }
            }
        })
        .map_err(|err| AvFoundationError::SessionSetup(err.to_string()))?;

    capture.runner = Some(CaptureRunner { stop, handle });
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn start_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn stop_capture(capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    let Some(runner) = capture.runner.take() else {
        return Ok(());
    };

    runner.stop.store(true, Ordering::Release);
    runner.handle.join().map_err(|_| {
        AvFoundationError::Runtime("AVFoundation capture thread panicked".to_string())
    })?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn stop_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use dispatch2::{DispatchQueue, DispatchRetained};
    use livekit::webrtc::video_frame::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{define_class, msg_send, AnyThread, DefinedClass, Message};
    use objc2_av_foundation::{
        AVCaptureDevice, AVCaptureDeviceFormat, AVCaptureDeviceInput, AVCaptureOutput,
        AVCaptureSession, AVCaptureSessionPreset1280x720, AVCaptureSessionPreset1920x1080,
        AVCaptureSessionPreset640x480, AVCaptureSessionPresetHigh, AVCaptureSessionPresetMedium,
        AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate, AVMediaTypeVideo,
    };
    use objc2_core_media::{CMSampleBuffer, CMTime, CMVideoFormatDescriptionGetDimensions};
    use objc2_core_video::{
        kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, kCVPixelFormatType_420YpCbCr8Planar,
        kCVPixelFormatType_420YpCbCr8PlanarFullRange, kCVPixelFormatType_422YpCbCr8,
        kCVPixelFormatType_422YpCbCr8FullRange, kCVPixelFormatType_422YpCbCr8_yuvs,
        kCVReturnSuccess, CVImageBuffer, CVPixelBuffer, CVPixelBufferGetBaseAddress,
        CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRow,
        CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane,
        CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth,
        CVPixelBufferGetWidthOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
        CVPixelBufferUnlockBaseAddress,
    };
    use objc2_foundation::{NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};

    use super::{AvFoundationCaptureOptions, AvFoundationError, AvFoundationFrame};
    use crate::device::{
        CaptureDeviceSelector, CaptureFormat, CaptureFormatRequest, CapturePixelFormat,
        CaptureResolution,
    };
    use crate::metadata::FrameMetadata;

    pub(super) struct SessionInner {
        session: Retained<AVCaptureSession>,
        _input: Retained<AVCaptureDeviceInput>,
        output: Retained<AVCaptureVideoDataOutput>,
        _delegate: Retained<CaptureDelegate>,
        _queue: DispatchRetained<DispatchQueue>,
        shared: Arc<FrameQueue>,
    }

    impl std::fmt::Debug for SessionInner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SessionInner").finish_non_exhaustive()
        }
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
        pub(super) fn new(options: &AvFoundationCaptureOptions) -> Result<Self, AvFoundationError> {
            let device = select_device(&options.device)?;
            let session = unsafe { AVCaptureSession::new() };
            let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
                .map_err(|err| {
                    AvFoundationError::SessionSetup(err.localizedDescription().to_string())
                })?;
            let output = unsafe { AVCaptureVideoDataOutput::new() };
            let shared = Arc::new(FrameQueue::default());
            let delegate = CaptureDelegate::new(shared.clone());
            let queue = DispatchQueue::new("io.livekit.capture.avfoundation", None);
            let active_format = select_active_format(&device, &options.format)?;

            // SAFETY: The session is newly created and not running. We add a
            // camera input and video data output only after canAdd* checks.
            unsafe {
                session.beginConfiguration();
                if active_format.is_none() {
                    if let Some(preset) = session_preset(&options.format) {
                        session.setSessionPreset(preset);
                    }
                }
                let config_result = (|| {
                    if !session.canAddInput(&input) {
                        return Err(AvFoundationError::SessionSetup(
                            "capture device input could not be added".to_string(),
                        ));
                    }
                    session.addInput(&input);

                    configure_device(&device, &options.format, active_format.as_deref())?;

                    if let Some(video_settings) = preferred_video_settings(&output) {
                        output.setVideoSettings(Some(&video_settings));
                    }
                    output.setAlwaysDiscardsLateVideoFrames(true);
                    output.setSampleBufferDelegate_queue(
                        Some(ProtocolObject::from_ref(&*delegate)),
                        Some(&queue),
                    );
                    if !session.canAddOutput(&output) {
                        return Err(AvFoundationError::SessionSetup(
                            "video data output could not be added".to_string(),
                        ));
                    }
                    session.addOutput(&output);
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

            Ok(Self { session, _input: input, output, _delegate: delegate, _queue: queue, shared })
        }

        pub(super) fn wait_for_format(
            &self,
            timeout: Duration,
        ) -> Result<CaptureFormat, AvFoundationError> {
            self.shared.wait_for_format(timeout)
        }

        pub(super) fn capture_frame(&mut self) -> Result<AvFoundationFrame, AvFoundationError> {
            self.shared.take_frame()
        }
    }

    fn preferred_video_settings(
        output: &AVCaptureVideoDataOutput,
    ) -> Option<Retained<NSDictionary<NSString, AnyObject>>> {
        let preferred = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
        // SAFETY: `output` is a live AVCaptureVideoDataOutput owned by the session setup path, and
        // querying advertised CV pixel formats does not mutate Rust-managed memory.
        let supported = unsafe { output.availableVideoCVPixelFormatTypes() }
            .iter()
            .any(|format| format.as_u32() == preferred);
        if !supported {
            return None;
        }

        let pixel_format = NSNumber::new_u32(preferred);
        // SAFETY: `kCVPixelBufferPixelFormatTypeKey` is a CoreVideo-provided
        // immutable CFString constant. `CFString` and `NSString` are toll-free
        // bridged, which objc2-foundation exposes through `AsRef<NSString>`.
        let key: &NSString = unsafe { kCVPixelBufferPixelFormatTypeKey }.as_ref();
        let value: &AnyObject = pixel_format.as_ref();
        Some(NSDictionary::from_slices(&[key], &[value]))
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
        latest: Option<AvFoundationFrame>,
        stopped: bool,
        error: Option<String>,
    }

    impl FrameQueue {
        fn push_frame(&self, frame: AvFoundationFrame) {
            let mut state = self.state.lock().expect("AVFoundation frame queue poisoned");
            if state.stopped {
                return;
            }
            state.latest = Some(frame);
            self.ready.notify_one();
        }

        fn set_error(&self, error: String) {
            let mut state = self.state.lock().expect("AVFoundation frame queue poisoned");
            state.error = Some(error);
            self.ready.notify_all();
        }

        fn stop(&self) {
            let mut state = self.state.lock().expect("AVFoundation frame queue poisoned");
            state.stopped = true;
            self.ready.notify_all();
        }

        fn wait_for_format(&self, timeout: Duration) -> Result<CaptureFormat, AvFoundationError> {
            let mut state = self.state.lock().expect("AVFoundation frame queue poisoned");
            loop {
                if let Some(frame) = state.latest.as_ref() {
                    let buffer = &frame.frame.buffer;
                    return Ok(CaptureFormat::new(
                        CaptureResolution::new(buffer.width(), buffer.height()),
                        0,
                        frame.source_pixel_format,
                    ));
                }
                if let Some(error) = state.error.take() {
                    return Err(AvFoundationError::Runtime(error));
                }
                if state.stopped {
                    return Err(AvFoundationError::NotRunning);
                }

                let (next_state, wait_result) = self
                    .ready
                    .wait_timeout(state, timeout)
                    .expect("AVFoundation frame queue poisoned");
                if wait_result.timed_out() {
                    return Err(AvFoundationError::FrameTimeout);
                }
                state = next_state;
            }
        }

        fn take_frame(&self) -> Result<AvFoundationFrame, AvFoundationError> {
            let mut state = self.state.lock().expect("AVFoundation frame queue poisoned");
            loop {
                if let Some(frame) = state.latest.take() {
                    return Ok(frame);
                }
                if let Some(error) = state.error.take() {
                    return Err(AvFoundationError::Runtime(error));
                }
                if state.stopped {
                    return Err(AvFoundationError::NotRunning);
                }
                state = self.ready.wait(state).expect("AVFoundation frame queue poisoned");
            }
        }

        fn timestamp_us(&self) -> i64 {
            elapsed_us(self.started_at.elapsed())
        }
    }

    fn select_device(
        selector: &CaptureDeviceSelector,
    ) -> Result<Retained<AVCaptureDevice>, AvFoundationError> {
        let media_type = unsafe { AVMediaTypeVideo }.ok_or(AvFoundationError::DeviceNotFound)?;
        match selector {
            CaptureDeviceSelector::Default => {
                unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media_type) }
                    .ok_or(AvFoundationError::DeviceNotFound)
            }
            CaptureDeviceSelector::Index(index) => {
                #[allow(deprecated)]
                let devices = unsafe { AVCaptureDevice::devicesWithMediaType(media_type) };
                devices
                    .iter()
                    .nth(*index)
                    .map(|device| device.retain())
                    .ok_or(AvFoundationError::DeviceNotFound)
            }
            CaptureDeviceSelector::Id(id) => {
                let id = NSString::from_str(id);
                unsafe { AVCaptureDevice::deviceWithUniqueID(&id) }
                    .ok_or(AvFoundationError::DeviceNotFound)
            }
        }
    }

    fn select_active_format(
        device: &AVCaptureDevice,
        request: &CaptureFormatRequest,
    ) -> Result<Option<Retained<AVCaptureDeviceFormat>>, AvFoundationError> {
        match request {
            CaptureFormatRequest::Default => Ok(None),
            CaptureFormatRequest::Exact(format) => {
                let selected = best_device_format(
                    device,
                    Some(format.resolution),
                    Some(format.frame_rate),
                    SelectionMode::Exact,
                );
                selected.map(Some).ok_or(AvFoundationError::UnsupportedFormat(*format))
            }
            CaptureFormatRequest::Closest(format) => Ok(best_device_format(
                device,
                Some(format.resolution),
                Some(format.frame_rate),
                SelectionMode::Closest,
            )),
            CaptureFormatRequest::HighestFrameRate { resolution, .. } => {
                Ok(best_device_format(device, *resolution, None, SelectionMode::HighestFrameRate))
            }
            CaptureFormatRequest::HighestResolution { frame_rate, .. } => {
                Ok(best_device_format(device, None, *frame_rate, SelectionMode::HighestResolution))
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SelectionMode {
        Exact,
        Closest,
        HighestFrameRate,
        HighestResolution,
    }

    #[derive(Debug)]
    struct DeviceFormatCandidate {
        format: Retained<AVCaptureDeviceFormat>,
        resolution: CaptureResolution,
        frame_rate_supported: bool,
        max_frame_rate: u32,
    }

    fn best_device_format(
        device: &AVCaptureDevice,
        resolution: Option<CaptureResolution>,
        frame_rate: Option<u32>,
        mode: SelectionMode,
    ) -> Option<Retained<AVCaptureDeviceFormat>> {
        // SAFETY: The AVCaptureDevice is retained for the session setup path; querying the
        // immutable list of supported formats does not mutate Rust-managed memory.
        let formats = unsafe { device.formats() };
        let mut candidates = formats
            .iter()
            .filter_map(|format| {
                let candidate_resolution = device_format_resolution(&format)?;
                let frame_rate_supported = frame_rate
                    .map(|frame_rate| device_format_supports_frame_rate(&format, frame_rate))
                    .unwrap_or(true);
                Some(DeviceFormatCandidate {
                    format: format.retain(),
                    resolution: candidate_resolution,
                    frame_rate_supported,
                    max_frame_rate: device_format_max_frame_rate(&format),
                })
            })
            .collect::<Vec<_>>();

        if let Some(resolution) = resolution {
            if mode == SelectionMode::Exact {
                return candidates
                    .into_iter()
                    .find(|candidate| {
                        candidate.resolution == resolution && candidate.frame_rate_supported
                    })
                    .map(|candidate| candidate.format);
            }
        }

        if frame_rate.is_some() && candidates.iter().any(|candidate| candidate.frame_rate_supported)
        {
            candidates.retain(|candidate| candidate.frame_rate_supported);
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
            SelectionMode::HighestFrameRate => candidates
                .into_iter()
                .filter(|candidate| {
                    resolution.map(|resolution| candidate.resolution == resolution).unwrap_or(true)
                })
                .max_by_key(|candidate| {
                    (
                        candidate.max_frame_rate,
                        candidate.resolution.width as u64 * candidate.resolution.height as u64,
                    )
                })
                .map(|candidate| candidate.format),
            SelectionMode::HighestResolution => candidates
                .into_iter()
                .max_by_key(|candidate| {
                    (
                        candidate.resolution.width as u64 * candidate.resolution.height as u64,
                        candidate.max_frame_rate,
                    )
                })
                .map(|candidate| candidate.format),
        }
    }

    fn device_format_resolution(format: &AVCaptureDeviceFormat) -> Option<CaptureResolution> {
        // SAFETY: `format` is an AVCaptureDeviceFormat from the device's immutable formats array.
        // Its format description is a valid CMVideoFormatDescription for video capture formats.
        let description = unsafe { format.formatDescription() };
        // SAFETY: `description` is the video format description returned by AVFoundation.
        let dimensions = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
        if dimensions.width <= 0 || dimensions.height <= 0 {
            return None;
        }
        Some(CaptureResolution::new(dimensions.width as u32, dimensions.height as u32))
    }

    fn device_format_supports_frame_rate(format: &AVCaptureDeviceFormat, frame_rate: u32) -> bool {
        let requested = frame_rate as f64;
        // SAFETY: `format` is an AVCaptureDeviceFormat from the device's immutable formats array.
        // The returned frame-rate ranges are immutable AVFoundation objects.
        unsafe { format.videoSupportedFrameRateRanges() }.iter().any(|range| {
            // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
            let min = unsafe { range.minFrameRate() };
            // SAFETY: AVFrameRateRange values are immutable for the lifetime of the object.
            let max = unsafe { range.maxFrameRate() };
            requested >= min.floor() && requested <= max.ceil()
        })
    }

    fn device_format_max_frame_rate(format: &AVCaptureDeviceFormat) -> u32 {
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

    fn resolution_distance(actual: CaptureResolution, requested: CaptureResolution) -> u64 {
        let width_delta = actual.width.abs_diff(requested.width) as u64;
        let height_delta = actual.height.abs_diff(requested.height) as u64;
        let pixel_delta = (actual.width as u64 * actual.height as u64)
            .abs_diff(requested.width as u64 * requested.height as u64);
        pixel_delta + width_delta * width_delta + height_delta * height_delta
    }

    fn configure_device(
        device: &AVCaptureDevice,
        request: &CaptureFormatRequest,
        active_format: Option<&AVCaptureDeviceFormat>,
    ) -> Result<(), AvFoundationError> {
        let frame_rate = requested_frame_rate(request);
        if active_format.is_none() && frame_rate.is_none() {
            return Ok(());
        }

        unsafe { device.lockForConfiguration() }.map_err(|err| {
            AvFoundationError::SessionSetup(err.localizedDescription().to_string())
        })?;

        let configure_result = configure_locked_device(device, active_format, frame_rate);
        // SAFETY: The device was successfully locked above and must be unlocked exactly once.
        unsafe {
            device.unlockForConfiguration();
        }
        configure_result
    }

    fn configure_locked_device(
        device: &AVCaptureDevice,
        active_format: Option<&AVCaptureDeviceFormat>,
        frame_rate: Option<u32>,
    ) -> Result<(), AvFoundationError> {
        // SAFETY: The caller holds the AVCaptureDevice configuration lock, and `active_format`
        // was selected from this device's formats array.
        unsafe {
            if let Some(active_format) = active_format {
                device.setActiveFormat(active_format);
            }
        }

        let Some(frame_rate) = frame_rate.filter(|frame_rate| *frame_rate > 0) else {
            return Ok(());
        };

        let active_format = match active_format {
            Some(active_format) => active_format.retain(),
            // SAFETY: The caller holds the configuration lock, and reading activeFormat is valid.
            None => unsafe { device.activeFormat() },
        };
        if !device_format_supports_frame_rate(&active_format, frame_rate) {
            return Ok(());
        }

        let duration = unsafe { CMTime::with_seconds(1.0 / frame_rate as f64, 600) };
        // SAFETY: The device is locked for configuration and the CMTime value is finite.
        unsafe {
            device.setActiveVideoMinFrameDuration(duration);
            device.setActiveVideoMaxFrameDuration(duration);
        }
        Ok(())
    }

    fn requested_frame_rate(request: &CaptureFormatRequest) -> Option<u32> {
        match request {
            CaptureFormatRequest::Default => None,
            CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
                Some(format.frame_rate)
            }
            CaptureFormatRequest::HighestFrameRate { .. } => None,
            CaptureFormatRequest::HighestResolution { frame_rate, .. } => *frame_rate,
        }
    }

    fn session_preset(
        request: &CaptureFormatRequest,
    ) -> Option<&'static objc2_av_foundation::AVCaptureSessionPreset> {
        let resolution = match request {
            CaptureFormatRequest::Exact(format) | CaptureFormatRequest::Closest(format) => {
                Some(format.resolution)
            }
            CaptureFormatRequest::HighestFrameRate { resolution, .. } => *resolution,
            CaptureFormatRequest::Default
            | CaptureFormatRequest::HighestResolution { frame_rate: _, pixel_format: _ } => None,
        }?;

        match (resolution.width, resolution.height) {
            (1920, 1080) => Some(unsafe { AVCaptureSessionPreset1920x1080 }),
            (1280, 720) => Some(unsafe { AVCaptureSessionPreset1280x720 }),
            (640, 480) => Some(unsafe { AVCaptureSessionPreset640x480 }),
            (w, h) if w <= 640 && h <= 480 => Some(unsafe { AVCaptureSessionPresetMedium }),
            _ => Some(unsafe { AVCaptureSessionPresetHigh }),
        }
    }

    fn process_sample_buffer(
        sample_buffer: &CMSampleBuffer,
        shared: &FrameQueue,
    ) -> Result<(), AvFoundationError> {
        let read_wall_time_us = unix_time_us_now().unwrap_or_default();
        let image_buffer = unsafe { sample_buffer.image_buffer() }
            .ok_or(AvFoundationError::InvalidFrame("sample buffer has no image buffer"))?;
        let image_buffer_ref: &CVImageBuffer = &image_buffer;
        // SAFETY: Video data output sample buffers deliver CVPixelBuffer-backed
        // CVImageBuffer objects. The retained image buffer keeps the object alive
        // for the duration of this conversion.
        let pixel_buffer =
            unsafe { &*(image_buffer_ref as *const CVImageBuffer as *const CVPixelBuffer) };
        let (buffer, source_pixel_format) = convert_pixel_buffer(pixel_buffer)?;

        let capture_wall_time_us = read_wall_time_us;
        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: shared.timestamp_us(),
            frame_metadata: FrameMetadata {
                user_timestamp: Some(capture_wall_time_us),
                frame_id: None,
            }
            .into_rtc(),
            buffer,
        };

        shared.push_frame(AvFoundationFrame {
            frame,
            source_pixel_format,
            capture_wall_time_us,
            read_wall_time_us,
            used_decode_path: false,
        });
        Ok(())
    }

    fn convert_pixel_buffer(
        pixel_buffer: &CVPixelBuffer,
    ) -> Result<(I420Buffer, CapturePixelFormat), AvFoundationError> {
        let lock_flags = CVPixelBufferLockFlags::ReadOnly;
        let lock_result = unsafe { CVPixelBufferLockBaseAddress(pixel_buffer, lock_flags) };
        if lock_result != kCVReturnSuccess {
            return Err(AvFoundationError::InvalidFrame("CVPixelBuffer lock failed"));
        }

        let result = convert_locked_pixel_buffer(pixel_buffer);

        // SAFETY: The pixel buffer was locked above with the same flags.
        let unlock_result = unsafe { CVPixelBufferUnlockBaseAddress(pixel_buffer, lock_flags) };
        if unlock_result != kCVReturnSuccess {
            return Err(AvFoundationError::InvalidFrame("CVPixelBuffer unlock failed"));
        }

        result
    }

    fn convert_locked_pixel_buffer(
        pixel_buffer: &CVPixelBuffer,
    ) -> Result<(I420Buffer, CapturePixelFormat), AvFoundationError> {
        let width = u32::try_from(CVPixelBufferGetWidth(pixel_buffer))
            .map_err(|_| AvFoundationError::InvalidFrame("width is out of range"))?;
        let height = u32::try_from(CVPixelBufferGetHeight(pixel_buffer))
            .map_err(|_| AvFoundationError::InvalidFrame("height is out of range"))?;
        let pixel_format = CVPixelBufferGetPixelFormatType(pixel_buffer);

        match pixel_format {
            format
                if format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
                    || format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange =>
            {
                convert_nv12(pixel_buffer, width, height)
                    .map(|buffer| (buffer, CapturePixelFormat::Nv12))
            }
            format if format == kCVPixelFormatType_32BGRA => {
                convert_bgra(pixel_buffer, width, height)
                    .map(|buffer| (buffer, CapturePixelFormat::Bgra))
            }
            format
                if format == kCVPixelFormatType_420YpCbCr8Planar
                    || format == kCVPixelFormatType_420YpCbCr8PlanarFullRange =>
            {
                convert_i420(pixel_buffer, width, height)
                    .map(|buffer| (buffer, CapturePixelFormat::I420))
            }
            format if format == kCVPixelFormatType_422YpCbCr8 => {
                convert_uyvy(pixel_buffer, width, height)
                    .map(|buffer| (buffer, CapturePixelFormat::Uyvy))
            }
            format
                if format == kCVPixelFormatType_422YpCbCr8_yuvs
                    || format == kCVPixelFormatType_422YpCbCr8FullRange =>
            {
                convert_yuy2(pixel_buffer, width, height)
                    .map(|buffer| (buffer, CapturePixelFormat::Yuyv))
            }
            other => Err(AvFoundationError::UnsupportedCoreVideoPixelFormat(other)),
        }
    }

    fn convert_nv12(
        pixel_buffer: &CVPixelBuffer,
        width: u32,
        height: u32,
    ) -> Result<I420Buffer, AvFoundationError> {
        if CVPixelBufferGetPlaneCount(pixel_buffer) < 2 {
            return Err(AvFoundationError::InvalidFrame("NV12 buffer has fewer than two planes"));
        }

        let y = plane(pixel_buffer, 0)?;
        let uv = plane(pixel_buffer, 1)?;
        let mut buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (dst_y, dst_u, dst_v) = buffer.data_mut();
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
            return Err(AvFoundationError::Convert("NV12ToI420 failed"));
        }
        Ok(buffer)
    }

    fn convert_bgra(
        pixel_buffer: &CVPixelBuffer,
        width: u32,
        height: u32,
    ) -> Result<I420Buffer, AvFoundationError> {
        let bgra = packed_plane(pixel_buffer, 4)?;
        let mut buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (dst_y, dst_u, dst_v) = buffer.data_mut();
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
            return Err(AvFoundationError::Convert("BGRAToI420 failed"));
        }
        Ok(buffer)
    }

    fn convert_uyvy(
        pixel_buffer: &CVPixelBuffer,
        width: u32,
        height: u32,
    ) -> Result<I420Buffer, AvFoundationError> {
        let uyvy = packed_plane(pixel_buffer, 2)?;
        let mut buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (dst_y, dst_u, dst_v) = buffer.data_mut();
        // SAFETY: The source slice covers the locked CVPixelBuffer plane for the duration of this
        // call, and the destination planes come from a freshly allocated I420Buffer with matching
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
            return Err(AvFoundationError::Convert("UYVYToI420 failed"));
        }
        Ok(buffer)
    }

    fn convert_yuy2(
        pixel_buffer: &CVPixelBuffer,
        width: u32,
        height: u32,
    ) -> Result<I420Buffer, AvFoundationError> {
        let yuy2 = packed_plane(pixel_buffer, 2)?;
        let mut buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (dst_y, dst_u, dst_v) = buffer.data_mut();
        // SAFETY: The source slice covers the locked CVPixelBuffer plane for the duration of this
        // call, and the destination planes come from a freshly allocated I420Buffer with matching
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
            return Err(AvFoundationError::Convert("YUY2ToI420 failed"));
        }
        Ok(buffer)
    }

    fn convert_i420(
        pixel_buffer: &CVPixelBuffer,
        width: u32,
        height: u32,
    ) -> Result<I420Buffer, AvFoundationError> {
        if CVPixelBufferGetPlaneCount(pixel_buffer) < 3 {
            return Err(AvFoundationError::InvalidFrame("I420 buffer has fewer than three planes"));
        }

        let y = plane(pixel_buffer, 0)?;
        let u = plane(pixel_buffer, 1)?;
        let v = plane(pixel_buffer, 2)?;
        let mut buffer = I420Buffer::new(width, height);
        let (stride_y, stride_u, stride_v) = buffer.strides();
        let (dst_y, dst_u, dst_v) = buffer.data_mut();
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
            return Err(AvFoundationError::Convert("I420Copy failed"));
        }
        Ok(buffer)
    }

    struct Plane<'a> {
        data: &'a [u8],
        stride: usize,
    }

    fn plane(pixel_buffer: &CVPixelBuffer, index: usize) -> Result<Plane<'_>, AvFoundationError> {
        let plane_count = CVPixelBufferGetPlaneCount(pixel_buffer);
        if index >= plane_count {
            return Err(AvFoundationError::InvalidFrame("plane index is out of range"));
        }

        let base = CVPixelBufferGetBaseAddressOfPlane(pixel_buffer, index);
        if base.is_null() {
            return Err(AvFoundationError::InvalidFrame("pixel plane has no base address"));
        }
        let stride = CVPixelBufferGetBytesPerRowOfPlane(pixel_buffer, index);
        let height = CVPixelBufferGetHeightOfPlane(pixel_buffer, index);
        let width = CVPixelBufferGetWidthOfPlane(pixel_buffer, index);
        let min_len = stride
            .checked_mul(height.saturating_sub(1))
            .and_then(|value| value.checked_add(width))
            .ok_or(AvFoundationError::InvalidFrame("pixel plane size overflow"))?;

        // SAFETY: The CVPixelBuffer is locked for read-only access, the plane
        // base address is non-null, and CoreVideo reports the minimum readable
        // extent for this plane.
        let data = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), min_len) };
        Ok(Plane { data, stride })
    }

    fn packed_plane(
        pixel_buffer: &CVPixelBuffer,
        bytes_per_pixel: usize,
    ) -> Result<Plane<'_>, AvFoundationError> {
        let base = CVPixelBufferGetBaseAddress(pixel_buffer);
        if base.is_null() {
            return Err(AvFoundationError::InvalidFrame("pixel buffer has no base address"));
        }
        let stride = CVPixelBufferGetBytesPerRow(pixel_buffer);
        let height = CVPixelBufferGetHeight(pixel_buffer);
        let width = CVPixelBufferGetWidth(pixel_buffer)
            .checked_mul(bytes_per_pixel)
            .ok_or(AvFoundationError::InvalidFrame("packed pixel row size overflow"))?;
        let min_len = stride
            .checked_mul(height.saturating_sub(1))
            .and_then(|value| value.checked_add(width))
            .ok_or(AvFoundationError::InvalidFrame("packed pixel buffer size overflow"))?;

        // SAFETY: The CVPixelBuffer is locked for read-only access, the base
        // address is non-null, and CoreVideo reports the minimum readable extent
        // for this packed buffer.
        let data = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), min_len) };
        Ok(Plane { data, stride })
    }

    fn elapsed_us(duration: Duration) -> i64 {
        i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
    }

    fn unix_time_us_now() -> Option<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_micros()).ok())
    }
}
