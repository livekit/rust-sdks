use anyhow::Result;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    ApiBackend, CameraFormat, CameraInfo, FrameFormat, RequestedFormat, RequestedFormatType,
    Resolution,
};
use nokhwa::Camera;
use std::collections::BTreeMap;

#[cfg(target_os = "macos")]
use nokhwa_bindings_macos::AVCaptureDevice;

fn main() -> Result<()> {
    let cameras = nokhwa::query(ApiBackend::Auto)?;
    if cameras.is_empty() {
        println!("No cameras detected.");
        return Ok(());
    }

    println!("Available cameras and capabilities:");
    for (idx, info) in cameras.iter().enumerate() {
        println!();
        println!("{}. {}", idx, info.human_name());
        match enumerate_capabilities(info) {
            Ok(formats) => print_capabilities(&formats),
            Err(err) => println!("   Capabilities: unavailable ({})", err),
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn enumerate_capabilities(
    info: &CameraInfo,
) -> Result<BTreeMap<FrameFormat, BTreeMap<Resolution, Vec<u32>>>> {
    let device = AVCaptureDevice::new(info.index())?;
    let formats = device.supported_formats()?;
    Ok(capabilities_from_formats(formats))
}

#[cfg(not(target_os = "macos"))]
fn enumerate_capabilities(
    info: &CameraInfo,
) -> Result<BTreeMap<FrameFormat, BTreeMap<Resolution, Vec<u32>>>> {
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let mut camera = Camera::new(info.index().clone(), requested)?;
    let mut capabilities = BTreeMap::new();
    if let Ok(mut fourccs) = camera.compatible_fourcc() {
        fourccs.sort();
        for fourcc in fourccs {
            let mut res_map = camera.compatible_list_by_resolution(fourcc)?;
            let mut res_sorted = BTreeMap::new();
            for (res, mut fps_list) in res_map.drain() {
                fps_list.sort();
                res_sorted.insert(res, fps_list);
            }
            capabilities.insert(fourcc, res_sorted);
        }
    } else {
        let formats = camera.compatible_camera_formats()?;
        capabilities = capabilities_from_formats(formats);
    }

    Ok(capabilities)
}

fn capabilities_from_formats(
    formats: Vec<CameraFormat>,
) -> BTreeMap<FrameFormat, BTreeMap<Resolution, Vec<u32>>> {
    let mut capabilities = BTreeMap::new();
    for fmt in formats {
        let res_map = capabilities.entry(fmt.format()).or_insert_with(BTreeMap::new);
        let fps_list = res_map.entry(fmt.resolution()).or_insert_with(Vec::new);
        fps_list.push(fmt.frame_rate());
    }
    for res_map in capabilities.values_mut() {
        for fps_list in res_map.values_mut() {
            fps_list.sort();
            fps_list.dedup();
        }
    }
    capabilities
}

fn print_capabilities(capabilities: &BTreeMap<FrameFormat, BTreeMap<Resolution, Vec<u32>>>) {
    if capabilities.is_empty() {
        println!("   Capabilities: none reported");
        return;
    }

    println!("   Capabilities:");
    for (format, resolutions) in capabilities {
        println!("   - Format: {}", format);
        if resolutions.is_empty() {
            println!("     (no resolutions reported)");
            continue;
        }
        for (resolution, fps_list) in resolutions {
            let fps_text = if fps_list.is_empty() {
                "unknown".to_string()
            } else {
                fps_list.iter().map(|fps| fps.to_string()).collect::<Vec<String>>().join(", ")
            };
            println!("     {} @ {} fps", resolution, fps_text);
        }
    }
}
