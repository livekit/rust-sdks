use anyhow::Result;
use livekit_capture::device::{CaptureDeviceInfo, CaptureFormat};

fn main() -> Result<()> {
    let cameras = platform_devices()?;
    if cameras.is_empty() {
        println!("No cameras detected.");
        return Ok(());
    }

    println!("Available cameras and capabilities:");
    for (idx, info) in cameras.iter().enumerate() {
        println!();
        println!("{}. {}", idx, info.name);
        print_device_details(info);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_devices() -> Result<Vec<CaptureDeviceInfo>> {
    Ok(livekit_capture::platform::avfoundation::devices()?)
}

#[cfg(target_os = "linux")]
fn platform_devices() -> Result<Vec<CaptureDeviceInfo>> {
    Ok(livekit_capture::sources::v4l::devices()?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_devices() -> Result<Vec<CaptureDeviceInfo>> {
    anyhow::bail!(
        "camera listing is not supported on {}; local_video supports macOS AVFoundation and Linux V4L2",
        std::env::consts::OS
    );
}

fn print_device_details(info: &CaptureDeviceInfo) {
    println!("   ID: {}", info.id);
    if let Some(model_id) = info.model_id.as_deref() {
        println!("   Model: {}", model_id);
    }
    if let Some(manufacturer) = info.manufacturer.as_deref() {
        println!("   Manufacturer: {}", manufacturer);
    }
    print_capabilities(&info.formats);
}

fn print_capabilities(formats: &[CaptureFormat]) {
    if formats.is_empty() {
        println!("   Capabilities: none reported by backend");
        return;
    }

    let mut formats = formats.to_vec();
    formats.sort_by_key(|format| {
        (
            format!("{:?}", format.pixel_format),
            format.resolution.width,
            format.resolution.height,
            format.frame_rate,
        )
    });

    println!("   Capabilities:");
    for format in formats {
        println!(
            "   - {:?}: {}x{} @ {} fps",
            format.pixel_format,
            format.resolution.width,
            format.resolution.height,
            format.frame_rate
        );
    }
}
