use std::{
    cmp::Reverse,
    fmt,
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::{Duration, Instant},
};

use futures::executor::block_on;
use livekit::{
    options::{
        compute_video_encodings, video_layers_from_encodings, TrackPublishOptions, VideoCodec,
    },
    prelude::TrackSource,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::native::NativeAudioSource,
        prelude::{I420Buffer, VideoFrame, VideoRotation},
        video_source::native::NativeVideoSource,
    },
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::record::ParticipantId;

pub const SAMPLE_RATE: u32 = 48_000;
const AUDIO_FRAME: Duration = Duration::from_millis(10);
const FRAME_SAMPLES: usize = (SAMPLE_RATE / 100) as usize;
const TONE_HZ: f32 = 500.0;
const TONE_AMPLITUDE: f32 = 0.3 * i16::MAX as f32;
const RING_FRAMES: usize = 16;
const RING_SHIFT: usize = 256 / RING_FRAMES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
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

// 500 Hz fits five whole cycles in 10 ms, so the same frame loops without a click
fn tone_frame() -> Vec<i16> {
    let step = 2.0 * std::f32::consts::PI * TONE_HZ / SAMPLE_RATE as f32;
    (0..FRAME_SAMPLES).map(|i| ((i as f32 * step).sin() * TONE_AMPLITUDE) as i16).collect()
}

fn frame_ring(width: u32, height: u32) -> Vec<I420Buffer> {
    (0..RING_FRAMES).map(|i| painted(width, height, i * RING_SHIFT)).collect()
}

fn painted(width: u32, height: u32, shift: usize) -> I420Buffer {
    let mut buffer = I420Buffer::new(width, height);
    let (stride_y, stride_u, _) = buffer.strides();
    let (y, u, v) = buffer.data_mut();
    for (row, line) in y.chunks_mut(stride_y as usize).enumerate() {
        for (col, luma) in line.iter_mut().enumerate() {
            *luma = ((row + col + shift) % 256) as u8;
        }
    }
    for (row, line) in u.chunks_mut(stride_u as usize).enumerate() {
        line.fill(((row + shift) % 256) as u8);
    }
    v.fill(128);
    buffer
}

type Publisher = (ParticipantId, NativeAudioSource, NativeVideoSource);

#[derive(Default)]
pub struct Publishers(Mutex<Vec<Publisher>>);

impl Publishers {
    pub fn add(&self, id: ParticipantId, audio: NativeAudioSource, video: NativeVideoSource) {
        self.0.lock().push((id, audio, video));
    }

    pub fn remove(&self, id: ParticipantId) {
        self.0.lock().retain(|(publ, ..)| *publ != id);
    }

    fn snapshot(&self) -> Vec<Publisher> {
        self.0.lock().clone()
    }
}

/// Feeds every publisher's sources from its own OS thread, paced by its own clock.
#[derive(Default)]
pub struct Pump {
    stop: AtomicBool,
    late_us: AtomicU64,
}

impl Pump {
    pub fn stop(&self) {
        self.stop.store(true, Relaxed);
    }

    /// The worst lateness of a tick that fed media since the last call.
    pub fn lateness(&self) -> Duration {
        Duration::from_micros(self.late_us.swap(0, Relaxed))
    }

    pub fn run(&self, publishers: &Publishers, profile: VideoProfile) {
        let samples = tone_frame();
        let tone = AudioFrame {
            data: samples.as_slice().into(),
            sample_rate: SAMPLE_RATE,
            num_channels: 1,
            samples_per_channel: FRAME_SAMPLES as u32,
        };
        let video_period = Duration::from_secs(1) / profile.fps.max(1);
        let mut ring = Vec::new();
        let mut frame = 0usize;
        let mut next_audio = Instant::now();
        let mut next_video = next_audio;
        while !self.stop.load(Relaxed) {
            let next = next_audio.min(next_video);
            let now = Instant::now();
            if now < next {
                std::thread::sleep(next - now);
                continue;
            }
            let publishers = publishers.snapshot();
            if !publishers.is_empty() {
                self.late_us.fetch_max((now - next).as_micros() as u64, Relaxed);
            }
            if now >= next_audio {
                for (_, audio, _) in &publishers {
                    if let Err(e) = block_on(audio.capture_frame(&tone)) {
                        eprintln!("audio capture: {e}");
                    }
                }
                next_audio = skip_past(next_audio, AUDIO_FRAME, now);
            }
            if now >= next_video {
                if !publishers.is_empty() {
                    if ring.is_empty() {
                        ring = frame_ring(profile.width, profile.height);
                    }
                    let buffer = &ring[frame % RING_FRAMES];
                    let video_frame = VideoFrame::new(VideoRotation::VideoRotation0, buffer);
                    for (_, _, video) in &publishers {
                        video.capture_frame(&video_frame);
                    }
                    frame += 1;
                }
                next_video = skip_past(next_video, video_period, now);
            }
        }
    }
}

// a late tick drops the ticks it missed instead of bursting them
fn skip_past(mut next: Instant, period: Duration, now: Instant) -> Instant {
    while next <= now {
        next += period;
    }
    next
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
