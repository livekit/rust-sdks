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

use thiserror::Error;

use crate::{
    device::{CaptureDeviceInfo, CaptureDeviceSelector, CaptureFormatRequest},
    error::CaptureError,
    track::VideoCaptureTrack,
};

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

/// AVFoundation decoded-frame capture session.
#[derive(Debug)]
pub struct AvFoundationCapture {
    track: VideoCaptureTrack,
    options: AvFoundationCaptureOptions,
}

impl AvFoundationCapture {
    /// Creates an AVFoundation capture session wrapper for a capture track.
    pub fn new(
        track: VideoCaptureTrack,
        options: AvFoundationCaptureOptions,
    ) -> Result<Self, AvFoundationError> {
        ensure_platform_available()?;
        Ok(Self { track, options })
    }

    /// Returns the capture track that receives decoded frames.
    pub fn track(&self) -> &VideoCaptureTrack {
        &self.track
    }

    /// Returns the configured capture options.
    pub fn options(&self) -> &AvFoundationCaptureOptions {
        &self.options
    }

    /// Starts AVFoundation capture.
    pub fn start(&mut self) -> Result<(), AvFoundationError> {
        start_capture(self)
    }

    /// Stops AVFoundation capture.
    pub fn stop(&mut self) -> Result<(), AvFoundationError> {
        stop_capture(self)
    }
}

/// Lists AVFoundation video capture devices.
pub fn devices() -> Result<Vec<CaptureDeviceInfo>, AvFoundationError> {
    list_devices()
}

/// Error returned by AVFoundation capture.
#[derive(Debug, Error)]
pub enum AvFoundationError {
    /// AVFoundation capture is only available on macOS.
    #[error("AVFoundation capture is only available on macOS")]
    UnsupportedPlatform,
    /// The requested device was not found.
    #[error("AVFoundation capture device was not found")]
    DeviceNotFound,
    /// The requested operation is represented by the API but not implemented yet.
    #[error("{0}")]
    NotImplemented(&'static str),
    /// The shared capture track rejected a frame.
    #[error(transparent)]
    Capture(#[from] CaptureError),
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
fn start_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Err(AvFoundationError::NotImplemented(
        "AVFoundation decoded-frame delegate capture is not wired yet",
    ))
}

#[cfg(not(target_os = "macos"))]
fn start_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
fn stop_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn stop_capture(_capture: &mut AvFoundationCapture) -> Result<(), AvFoundationError> {
    Err(AvFoundationError::UnsupportedPlatform)
}
