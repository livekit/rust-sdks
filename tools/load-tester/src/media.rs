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
const SVC_MODE: &str = "L3T3_KEY";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    Vp8,
    H264,
    Vp9,
    Av1,
}

impl Codec {
    fn is_svc(self) -> bool {
        matches!(self, Codec::Vp9 | Codec::Av1)
    }
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
#[serde(default)]
pub struct VideoProfile {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
    pub layered: bool,
}

impl Default for VideoProfile {
    fn default() -> Self {
        Self { width: 1280, height: 720, fps: 30, codec: Codec::Vp8, layered: true }
    }
}

impl VideoProfile {
    pub(crate) fn layering(&self) -> &'static str {
        match (self.layered, self.codec.is_svc()) {
            (false, _) => "single layer",
            (true, false) => "simulcast",
            (true, true) => "svc",
        }
    }
}

impl fmt::Display for VideoProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}p{}", self.height, self.fps)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioProfile {
    pub red: bool,
    pub dtx: bool,
}

impl Default for AudioProfile {
    fn default() -> Self {
        let sdk = TrackPublishOptions::default();
        Self { red: sdk.red, dtx: sdk.dtx }
    }
}

pub fn audio_options(profile: &AudioProfile) -> TrackPublishOptions {
    TrackPublishOptions {
        source: TrackSource::Microphone,
        red: profile.red,
        dtx: profile.dtx,
        ..Default::default()
    }
}

pub fn video_options(profile: &VideoProfile) -> TrackPublishOptions {
    let svc = profile.layered && profile.codec.is_svc();
    TrackPublishOptions {
        source: TrackSource::Camera,
        video_codec: profile.codec.into(),
        simulcast: profile.layered,
        scalability_mode: svc.then(|| SVC_MODE.to_string()),
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
        let options = video_options(profile);
        let encodings = compute_video_encodings(profile.width, profile.height, &options);
        let heights = video_layers_from_encodings(profile.width, profile.height, &encodings);
        let capped = |max_framerate: Option<f64>| {
            let max_fps = profile.fps as f32;
            max_framerate.map_or(max_fps, |f| (f as f32).min(max_fps))
        };
        let mut layers: Vec<Layer> = if options.scalability_mode.is_some() {
            // every spatial layer rides in the one encoding, so they all run at its frame rate
            let fps = capped(encodings.first().and_then(|e| e.max_framerate));
            heights.iter().map(|l| Layer { height: l.height, fps }).collect()
        } else {
            heights
                .iter()
                .zip(&encodings)
                .map(|(l, e)| Layer { height: l.height, fps: capped(e.max_framerate) })
                .collect()
        };
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
    VideoProfile { width: height * 16 / 9, height, fps, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svc_codecs_publish_one_encoding_whose_spatial_layers_share_its_frame_rate() {
        for codec in [Codec::Vp9, Codec::Av1] {
            let profile = VideoProfile { codec, ..test_profile(720, 30) };
            assert_eq!(profile.layering(), "svc");
            let options = video_options(&profile);
            assert_eq!(options.scalability_mode.as_deref(), Some(SVC_MODE));
            assert_eq!(compute_video_encodings(1280, 720, &options).len(), 1, "{codec}");
            let ladder = Ladder::of(&profile);
            let heights: Vec<u32> = ladder.0.iter().map(|l| l.height).collect();
            assert_eq!(heights, [720, 360, 180], "{codec}");
            assert!(ladder.0.iter().all(|l| l.fps == 30.0), "{codec}: {:?}", ladder.0);
        }
    }

    #[test]
    fn rungs_are_ladder_steps_not_height_halvings() {
        let ladder = Ladder::of(&test_profile(1080, 30));
        assert_eq!(ladder.layer(0).height, 1080);
        assert_eq!(ladder.rung(360), 1, "1080p publishes 1080/360/180");
        assert_eq!(ladder.rung(180), 2);
    }
}
