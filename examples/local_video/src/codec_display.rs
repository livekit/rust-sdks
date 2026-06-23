#[allow(dead_code)]
pub(crate) fn codec_from_mime(mime: &str) -> String {
    let base = mime.split(';').next().unwrap_or(mime).trim();
    let last = base.rsplit('/').next().unwrap_or(base).trim();
    last.to_ascii_uppercase()
}

pub(crate) fn codec_with_implementation(codec: &str, implementation: &str) -> String {
    let codec = if codec.is_empty() { "Unknown" } else { codec };
    let Some(implementation) = implementation_label(implementation) else {
        return codec.to_string();
    };

    format!("{codec} {implementation}")
}

fn implementation_label(implementation: &str) -> Option<String> {
    let implementation = implementation.trim();
    if implementation.is_empty() {
        return None;
    }

    let lower = implementation.to_ascii_lowercase();
    if lower.contains("nvidia") {
        return Some(if lower.contains("decoder") { "NVDEC" } else { "NVENC" }.to_string());
    }
    if lower.contains("vaapi") {
        return Some("VAAPI".to_string());
    }
    if lower.contains("videotoolbox") {
        return Some("VideoToolbox".to_string());
    }
    if lower.contains("openh264") {
        return Some("OpenH264".to_string());
    }
    if lower.contains("libvpx") {
        return Some("libvpx".to_string());
    }
    if lower.contains("libaom") {
        return Some("libaom".to_string());
    }

    Some(
        implementation
            .strip_suffix(" Encoder")
            .or_else(|| implementation.strip_suffix(" Decoder"))
            .unwrap_or(implementation)
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_codec_from_mime_type() {
        assert_eq!(codec_from_mime("video/H264;profile-level-id=42e01f"), "H264");
    }

    #[test]
    fn shortens_common_hardware_implementations() {
        assert_eq!(codec_with_implementation("H264", "NVIDIA H264 Encoder"), "H264 NVENC");
        assert_eq!(codec_with_implementation("H265", "VAAPI H264 Encoder"), "H265 VAAPI");
        assert_eq!(codec_with_implementation("H264", "NVIDIA H264 Decoder"), "H264 NVDEC");
    }
}
