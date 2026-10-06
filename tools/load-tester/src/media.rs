use std::{cmp::Reverse, fmt};

use livekit::options::{
    compute_video_encodings, video_layers_from_encodings, TrackPublishOptions, VideoCodec,
};
use livekit::prelude::TrackSource;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Vp8,
    H264,
    Vp9,
    Av1,
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(VideoCodec::from(*self).as_str())
    }
}

impl From<Codec> for VideoCodec {
    fn from(codec: Codec) -> Self {
        match codec {
            Codec::Vp8 => VideoCodec::VP8,
            Codec::H264 => VideoCodec::H264,
            Codec::Vp9 => VideoCodec::VP9,
            Codec::Av1 => VideoCodec::AV1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
}

impl fmt::Display for VideoProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}p{}", self.height, self.fps)
    }
}

pub fn publish_options(profile: &VideoProfile) -> TrackPublishOptions {
    TrackPublishOptions {
        source: TrackSource::Camera,
        simulcast: true,
        video_codec: profile.codec.into(),
        ..Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub height: u32,
    pub fps: f32,
}

#[derive(Clone, Debug)]
pub struct Ladder(Vec<Layer>);

impl Ladder {
    pub fn of(profile: &VideoProfile) -> Self {
        let max_fps = profile.fps as f32;
        let encodings =
            compute_video_encodings(profile.width, profile.height, &publish_options(profile));
        let mut layers: Vec<Layer> =
            video_layers_from_encodings(profile.width, profile.height, &encodings)
                .iter()
                .zip(&encodings)
                .map(|(layer, encoding)| Layer {
                    height: layer.height,
                    fps: encoding.max_framerate.map_or(max_fps, |f| (f as f32).min(max_fps)),
                })
                .collect();
        if layers.is_empty() {
            layers.push(Layer { height: profile.height, fps: max_fps });
        }
        layers.sort_by_key(|l| Reverse(l.height));
        Self(layers)
    }

    pub fn rung(&self, height: u32) -> usize {
        self.0
            .iter()
            .enumerate()
            .min_by_key(|(_, l)| l.height.abs_diff(height))
            .map(|(rung, _)| rung)
            .expect("a ladder has at least one layer")
    }

    pub fn layer(&self, rung: usize) -> Layer {
        self.0[rung]
    }
}

#[cfg(test)]
pub(crate) fn test_profile(height: u32, fps: u32) -> VideoProfile {
    VideoProfile { width: height * 16 / 9, height, fps, codec: Codec::Vp8 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rungs_are_ladder_steps_not_height_halvings() {
        let ladder = Ladder::of(&test_profile(1080, 30));
        assert_eq!(ladder.layer(0).height, 1080);
        assert_eq!(ladder.rung(360), 1, "1080p publishes 1080/360/180");
        assert_eq!(ladder.rung(180), 2);
    }
}
