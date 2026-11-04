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

use std::str::FromStr;

/// A video codec recognized by [`PeerConnectionFactory::video_encoder_codecs`](crate::peer_connection_factory::PeerConnectionFactory::video_encoder_codecs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VideoCodec {
    /// VP8 video.
    VP8,
    /// VP9 video.
    VP9,
    /// AV1 video.
    AV1,
    /// H.264/AVC video.
    H264,
    /// H.265/HEVC video.
    H265,
}

impl VideoCodec {
    /// Returns the lowercase SDP codec name, suitable for a video MIME subtype.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VP8 => "vp8",
            Self::VP9 => "vp9",
            Self::AV1 => "av1",
            Self::H264 => "h264",
            Self::H265 => "h265",
        }
    }
}

/// An SDP codec name is not recognized as a [`VideoCodec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("unsupported video codec name")]
pub struct UnsupportedVideoCodec;

impl FromStr for VideoCodec {
    type Err = UnsupportedVideoCodec;

    /// Parses a case-insensitive SDP codec name, accepting HEVC as an H265 alias.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            name if name.eq_ignore_ascii_case("vp8") => Ok(Self::VP8),
            name if name.eq_ignore_ascii_case("vp9") => Ok(Self::VP9),
            name if name.eq_ignore_ascii_case("av1") => Ok(Self::AV1),
            name if name.eq_ignore_ascii_case("h264") => Ok(Self::H264),
            name if name.eq_ignore_ascii_case("h265") || name.eq_ignore_ascii_case("hevc") => {
                Ok(Self::H265)
            }
            _ => Err(UnsupportedVideoCodec),
        }
    }
}

impl TryFrom<&str> for VideoCodec {
    type Error = UnsupportedVideoCodec;

    /// Parses an SDP codec name using [`FromStr`].
    fn try_from(name: &str) -> Result<Self, Self::Error> {
        name.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::{UnsupportedVideoCodec, VideoCodec};

    #[test]
    fn parses_sdp_names_and_hevc_alias() {
        for (name, expected) in [
            ("VP8", VideoCodec::VP8),
            ("vp9", VideoCodec::VP9),
            ("Av1", VideoCodec::AV1),
            ("H264", VideoCodec::H264),
            ("h265", VideoCodec::H265),
            ("HeVc", VideoCodec::H265),
        ] {
            assert_eq!(name.parse::<VideoCodec>(), Ok(expected));
            assert_eq!(VideoCodec::try_from(name), Ok(expected));
            assert_eq!(expected.as_str().parse::<VideoCodec>(), Ok(expected));
        }
    }

    #[test]
    fn rejects_unknown_codec_names() {
        for name in ["vp11", "vp08", "", "video/vp8", " VP8", "rtx"] {
            assert_eq!(name.parse::<VideoCodec>(), Err(UnsupportedVideoCodec));
            assert_eq!(VideoCodec::try_from(name), Err(UnsupportedVideoCodec));
        }
    }
}
