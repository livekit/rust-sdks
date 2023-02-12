use crate::prelude::*;

pub const TELEPHONE_BITRATE: u32 = 12_000;
pub const SPEECH_BITRATE: u32 = 20_000;
pub const MUSIC_BITRATE: u32 = 32_000;
pub const MUSIC_STEREO_BITRATE: u32 = 48_000;
pub const MUSIC_HIGH_QUALITY_BITRATE: u32 = 64_000;
pub const MUSIC_HIGH_QUALITY_STEREO_BITRATE: u32 = 96_000;

#[derive(Debug)]
pub enum VideoCodec {
    VP8,
    H264,
    AV1,
}

impl From<VideoCodec> for &'static str {
    fn from(codec: VideoCodec) -> Self {
        match codec {
            VideoCodec::VP8 => "vp8",
            VideoCodec::H264 => "h264",
            VideoCodec::AV1 => "av1",
        }
    }
}

#[derive(Debug)]
pub struct TrackPublishOptions {
    pub dynacast: bool,
    pub codec: VideoCodec,
    pub audio_bitrate: u32,
    pub dtx: bool,
    pub red: bool,
    pub simulcast: bool,
    pub name: String,
    pub source: TrackSource,
}

impl Default for TrackPublishOptions {
    fn default() -> Self {
        Self {
            dynacast: false,
            codec: VideoCodec::VP8,
            audio_bitrate: SPEECH_BITRATE,
            dtx: true,
            red: true,
            simulcast: true,
            name: "unnamed track".to_owned(),
            source: TrackSource::Unknown,
        }
    }
}
