#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct RtpHeaderExtensionParameters {
    pub uri: String,
    pub id: i32,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RtpParameters {
    pub codecs: Vec<RtpCodecParameters>,
    pub header_extensions: Vec<RtpHeaderExtensionParameters>,
    pub rtcp: RtcpParameters,
}

#[derive(Debug, Clone)]
pub struct RtpCodecParameters {
    pub payload_type: u8,
    pub mime_type: String, // read-only
    pub clock_rate: Option<u64>,
    pub channels: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct RtcpParameters {
    pub cname: String,
    pub reduced_size: bool,
}

#[derive(Debug, Clone)]
pub struct RtpEncodingParameters {
    pub active: bool,
    pub max_bitrate: Option<u64>,
    pub max_framerate: Option<f64>,
    pub priority: Priority,
    pub rid: String,
    pub scale_resolution_down_by: Option<f64>,
}

impl Default for RtpCodecParameters {
    fn default() -> Self {
        Self {
            payload_type: 0,
            mime_type: String::default(),
            clock_rate: None,
            channels: None,
        }
    }
}

impl Default for RtpEncodingParameters {
    fn default() -> Self {
        Self {
            active: true,
            max_bitrate: None,
            max_framerate: None,
            priority: Priority::Low,
            rid: String::default(),
            scale_resolution_down_by: None,
        }
    }
}
