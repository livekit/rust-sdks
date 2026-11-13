// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.

use crate::audio::AudioFormat;
use futures_util::StreamExt;
use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use livekit::webrtc::{
    audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions},
    audio_track::RtcAudioTrack,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use thiserror::Error;

const APPSRC_NAME: &str = "lk_audio_appsrc";

/// Configuration for mixed PCM playout into GStreamer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GStreamerAudioPlayoutConfig {
    /// Launch description containing `appsrc name=lk_audio_appsrc`.
    pub pipeline: String,
    /// Mixer and appsrc sample rate.
    pub sample_rate: u32,
    /// Mixer and appsrc channel count.
    pub num_channels: u32,
    /// Mixer frame duration in milliseconds.
    pub frame_duration_ms: u32,
}

impl Default for GStreamerAudioPlayoutConfig {
    fn default() -> Self {
        Self {
            pipeline: String::new(),
            sample_rate: 48_000,
            num_channels: 1,
            frame_duration_ms: 10,
        }
    }
}

/// Terminal statistics from a GStreamer audio playout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GStreamerAudioPlayoutStats {
    /// Mixed frames accepted by appsrc.
    pub frames_pushed: u64,
    /// Track frames dropped because the bounded mixer input was full.
    pub frames_dropped: u64,
    /// Interleaved PCM bytes accepted by appsrc.
    pub bytes_pushed: u64,
}

/// Running mixed playout into `lk_audio_appsrc`.
pub struct GStreamerAudioPlayout {
    stop: tokio::sync::watch::Sender<bool>,
    control: GStreamerAudioPlayoutControl,
    task: Option<
        tokio::task::JoinHandle<Result<GStreamerAudioPlayoutStats, GStreamerAudioPlayoutError>>,
    >,
}

/// Cloneable command handle for a running [`GStreamerAudioPlayout`].
#[derive(Clone)]
pub struct GStreamerAudioPlayoutControl {
    commands: tokio::sync::mpsc::Sender<PlayoutCommand>,
}

impl GStreamerAudioPlayout {
    /// Starts playout, optionally with initial RTC audio tracks.
    ///
    /// An empty track set is valid: the pipeline remains running without
    /// pushing audio until [`GStreamerAudioPlayout::attach_track`] succeeds.
    /// Duplicate initial track IDs are rejected.
    pub fn start(
        config: GStreamerAudioPlayoutConfig,
        tracks: impl IntoIterator<Item = RtcAudioTrack>,
    ) -> Result<Self, GStreamerAudioPlayoutError> {
        gst::init().map_err(|error| GStreamerAudioPlayoutError::Pipeline(error.to_string()))?;
        let format = AudioFormat::with_frame_duration(
            config.sample_rate,
            config.num_channels,
            config.frame_duration_ms,
        )
        .ok_or(GStreamerAudioPlayoutError::InvalidFormat)?;
        let tracks = tracks.into_iter().collect::<Vec<_>>();
        let mut track_ids = HashSet::with_capacity(tracks.len());
        for track in &tracks {
            let track_id = track.id();
            if !track_ids.insert(track_id.clone()) {
                return Err(GStreamerAudioPlayoutError::DuplicateTrack(track_id));
            }
        }

        let pipeline = gst::parse::launch(&config.pipeline)
            .map_err(|error| GStreamerAudioPlayoutError::Pipeline(error.to_string()))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| GStreamerAudioPlayoutError::NotAPipeline)?;
        let appsrc = pipeline
            .by_name(APPSRC_NAME)
            .ok_or(GStreamerAudioPlayoutError::MissingAppSrc)?
            .downcast::<gst_app::AppSrc>()
            .map_err(|_| GStreamerAudioPlayoutError::NotAnAppSrc)?;
        let caps: gst::Caps = format!(
            "audio/x-raw,format=S16LE,layout=interleaved,rate={},channels={}",
            format.sample_rate, format.num_channels
        )
        .parse()
        .map_err(|error| GStreamerAudioPlayoutError::Pipeline(format!("invalid caps: {error}")))?;
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);
        appsrc.set_is_live(true);
        appsrc.set_property("block", false);
        appsrc.set_max_bytes((format.samples_per_frame() * 2 * 4) as u64);
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|error| GStreamerAudioPlayoutError::Pipeline(error.to_string()))?;

        let (stop, stop_rx) = tokio::sync::watch::channel(false);
        let (commands, command_rx) = tokio::sync::mpsc::channel(32);
        let pipeline = PlayingAudioPipeline { pipeline, appsrc };
        let task = tokio::spawn(run_playout(pipeline, format, tracks, stop_rx, command_rx));
        Ok(Self { stop, control: GStreamerAudioPlayoutControl { commands }, task: Some(task) })
    }

    /// Returns a cloneable handle for attaching and detaching tracks.
    pub fn control(&self) -> GStreamerAudioPlayoutControl {
        self.control.clone()
    }

    /// Attaches a track and returns its RTC track ID.
    ///
    /// Attaching an ID that is currently attached returns
    /// [`GStreamerAudioPlayoutError::DuplicateTrack`] and leaves the existing
    /// reader unchanged.
    pub async fn attach_track(
        &self,
        track: RtcAudioTrack,
    ) -> Result<String, GStreamerAudioPlayoutError> {
        self.control.attach_track(track).await
    }

    /// Detaches a track without stopping the pipeline.
    pub async fn detach_track(&self, track_id: &str) -> Result<bool, GStreamerAudioPlayoutError> {
        self.control.detach_track(track_id).await
    }

    /// Signals EOS and waits for every track reader and the pipeline to stop.
    pub async fn stop(mut self) -> Result<GStreamerAudioPlayoutStats, GStreamerAudioPlayoutError> {
        let _ = self.stop.send(true);
        let task = self.task.take().expect("playout task is joined once");
        task.await.map_err(|error| GStreamerAudioPlayoutError::Task(error.to_string()))?
    }
}

impl GStreamerAudioPlayoutControl {
    /// Attaches a track and returns its RTC track ID.
    ///
    /// Duplicate IDs are rejected without replacing the existing reader.
    pub async fn attach_track(
        &self,
        track: RtcAudioTrack,
    ) -> Result<String, GStreamerAudioPlayoutError> {
        let (response, received) = tokio::sync::oneshot::channel();
        self.commands
            .send(PlayoutCommand::Attach { track, response })
            .await
            .map_err(|_| GStreamerAudioPlayoutError::TaskStopped)?;
        received.await.map_err(|_| GStreamerAudioPlayoutError::TaskStopped)?
    }

    /// Detaches a track without stopping the pipeline.
    ///
    /// Returns `false` when the ID was not attached. On success, the reader is
    /// cancelled and joined before this method returns.
    pub async fn detach_track(&self, track_id: &str) -> Result<bool, GStreamerAudioPlayoutError> {
        let (response, received) = tokio::sync::oneshot::channel();
        self.commands
            .send(PlayoutCommand::Detach { track_id: track_id.to_owned(), response })
            .await
            .map_err(|_| GStreamerAudioPlayoutError::TaskStopped)?;
        received.await.map_err(|_| GStreamerAudioPlayoutError::TaskStopped)
    }
}

enum PlayoutCommand {
    Attach {
        track: RtcAudioTrack,
        response: tokio::sync::oneshot::Sender<Result<String, GStreamerAudioPlayoutError>>,
    },
    Detach {
        track_id: String,
        response: tokio::sync::oneshot::Sender<bool>,
    },
}

struct TrackFrame {
    track_id: String,
    generation: u64,
    data: Vec<i16>,
}

enum ReaderEvent {
    Frame(TrackFrame),
    Ended { track_id: String, generation: u64 },
}

#[derive(Default)]
struct MixerState {
    next_generation: u64,
    tracks: HashMap<String, MixerTrack>,
}

struct MixerTrack {
    generation: u64,
    pending: Option<Vec<i16>>,
}

impl MixerState {
    fn attach(&mut self, track_id: &str) -> Result<u64, GStreamerAudioPlayoutError> {
        if self.tracks.contains_key(track_id) {
            return Err(GStreamerAudioPlayoutError::DuplicateTrack(track_id.to_owned()));
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        self.tracks.insert(track_id.to_owned(), MixerTrack { generation, pending: None });
        Ok(generation)
    }

    fn detach(&mut self, track_id: &str) -> bool {
        self.tracks.remove(track_id).is_some()
    }

    fn end(&mut self, track_id: &str, generation: u64) -> bool {
        if self.tracks.get(track_id).is_some_and(|track| track.generation == generation) {
            self.tracks.remove(track_id);
            true
        } else {
            false
        }
    }

    fn accept(&mut self, frame: TrackFrame) {
        let Some(track) = self.tracks.get_mut(&frame.track_id) else {
            return;
        };
        if track.generation == frame.generation {
            track.pending = Some(frame.data);
        }
    }

    fn mix(&mut self, samples_per_frame: usize) -> (Option<Vec<i16>>, u64) {
        mix_pending_frames(
            self.tracks.values_mut().map(|track| &mut track.pending),
            samples_per_frame,
        )
    }
}

impl Drop for GStreamerAudioPlayout {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        // Dropping the join handle detaches the actor; the stop signal lets
        // it cancel and join every reader before releasing the pipeline.
        self.task.take();
    }
}

async fn run_playout(
    pipeline: PlayingAudioPipeline,
    format: AudioFormat,
    tracks: Vec<RtcAudioTrack>,
    mut stop: tokio::sync::watch::Receiver<bool>,
    mut commands: tokio::sync::mpsc::Receiver<PlayoutCommand>,
) -> Result<GStreamerAudioPlayoutStats, GStreamerAudioPlayoutError> {
    let dropped = Arc::new(AtomicU64::new(0));
    let (frame_tx, mut frames) = tokio::sync::mpsc::channel::<ReaderEvent>(64);
    let mut mixer = MixerState::default();
    let mut readers = HashMap::new();
    for track in tracks {
        attach_reader(track, format, &mut mixer, &mut readers, &frame_tx, &dropped)?;
    }

    let mut interval =
        tokio::time::interval(std::time::Duration::from_nanos(format.frame_duration_ns()));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut stats = GStreamerAudioPlayoutStats::default();
    let started = std::time::Instant::now();

    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                match command {
                    PlayoutCommand::Attach { track, response } => {
                        let result = attach_reader(
                            track,
                            format,
                            &mut mixer,
                            &mut readers,
                            &frame_tx,
                            &dropped,
                        );
                        let _ = response.send(result);
                    }
                    PlayoutCommand::Detach { track_id, response } => {
                        let detached = mixer.detach(&track_id);
                        if let Some(reader) = readers.remove(&track_id) {
                            reader.abort();
                            let _ = reader.await;
                        }
                        let _ = response.send(detached);
                    }
                }
            }
            event = frames.recv() => {
                let Some(event) = event else { break };
                match event {
                    ReaderEvent::Frame(frame) => mixer.accept(frame),
                    ReaderEvent::Ended { track_id, generation } => {
                        if mixer.end(&track_id, generation) {
                            if let Some(reader) = readers.remove(&track_id) {
                                let _ = reader.await;
                            }
                        }
                    }
                }
            }
            _ = interval.tick() => {
                let buffer_timestamp_ns =
                    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                let (samples, rejected) = mixer.mix(format.samples_per_frame());
                stats.frames_dropped += rejected;
                let Some(samples) = samples else {
                    continue;
                };
                let mut buffer = gst::Buffer::with_size(samples.len() * 2)
                    .map_err(|error| GStreamerAudioPlayoutError::Pipeline(error.to_string()))?;
                {
                    let buffer = buffer.get_mut().expect("new GStreamer buffer is uniquely owned");
                    buffer.set_pts(gst::ClockTime::from_nseconds(buffer_timestamp_ns));
                    buffer.set_duration(gst::ClockTime::from_nseconds(format.frame_duration_ns()));
                    let mut map = buffer
                        .map_writable()
                        .map_err(|error| GStreamerAudioPlayoutError::Pipeline(error.to_string()))?;
                    for (bytes, sample) in map.as_mut_slice().chunks_exact_mut(2).zip(samples) {
                        bytes.copy_from_slice(&sample.to_le_bytes());
                    }
                }
                pipeline.appsrc
                    .push_buffer(buffer)
                    .map_err(|error| GStreamerAudioPlayoutError::Flow(error.to_string()))?;
                stats.frames_pushed += 1;
                stats.bytes_pushed += (format.samples_per_frame() * 2) as u64;
            }
        }
    }

    for (_, reader) in readers {
        reader.abort();
        let _ = reader.await;
    }
    stats.frames_dropped += dropped.load(Ordering::Relaxed);
    Ok(stats)
}

fn attach_reader(
    track: RtcAudioTrack,
    format: AudioFormat,
    mixer: &mut MixerState,
    readers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
    frames: &tokio::sync::mpsc::Sender<ReaderEvent>,
    dropped: &Arc<AtomicU64>,
) -> Result<String, GStreamerAudioPlayoutError> {
    let track_id = track.id();
    let generation = mixer.attach(&track_id)?;
    let frames = frames.clone();
    let dropped = Arc::clone(dropped);
    let reader_track_id = track_id.clone();
    let mut reader = NativeAudioStream::with_options(
        track,
        format.sample_rate as i32,
        format.num_channels as i32,
        NativeAudioStreamOptions { queue_size_frames: Some(2) },
    );
    let task = tokio::spawn(async move {
        while let Some(frame) = reader.next().await {
            let frame = ReaderEvent::Frame(TrackFrame {
                track_id: reader_track_id.clone(),
                generation,
                data: frame.data.into_owned(),
            });
            if frames.try_send(frame).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        let _ = frames.send(ReaderEvent::Ended { track_id: reader_track_id, generation }).await;
    });
    readers.insert(track_id.clone(), task);
    Ok(track_id)
}

struct PlayingAudioPipeline {
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
}

impl Drop for PlayingAudioPipeline {
    fn drop(&mut self) {
        let _ = self.appsrc.end_of_stream();
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn mix_pending_frames<'a>(
    pending: impl IntoIterator<Item = &'a mut Option<Vec<i16>>>,
    samples_per_frame: usize,
) -> (Option<Vec<i16>>, u64) {
    let mut mixed = vec![0i64; samples_per_frame];
    let mut contributors = 0i64;
    let mut rejected = 0;
    for frame in pending {
        let Some(frame) = frame.take() else { continue };
        if frame.len() != mixed.len() {
            rejected += 1;
            continue;
        }
        contributors += 1;
        for (output, sample) in mixed.iter_mut().zip(frame) {
            *output += i64::from(sample);
        }
    }
    if contributors == 0 {
        return (None, rejected);
    }
    let samples = mixed
        .into_iter()
        .map(|sample| {
            (sample / contributors).clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
        })
        .collect();
    (Some(samples), rejected)
}

/// Error starting or running GStreamer audio playout.
#[derive(Debug, Error)]
pub enum GStreamerAudioPlayoutError {
    /// The requested PCM format is invalid.
    #[error("sample rate, channel count, and frame duration must form whole non-zero frames")]
    InvalidFormat,
    /// A track with this RTC ID is already attached.
    #[error("audio track '{0}' is already attached")]
    DuplicateTrack(String),
    /// The launch description did not create a pipeline.
    #[error("GStreamer description did not create a pipeline")]
    NotAPipeline,
    /// The required appsrc is missing.
    #[error("pipeline must contain `appsrc name={APPSRC_NAME}`")]
    MissingAppSrc,
    /// The named element is not an appsrc.
    #[error("pipeline element `{APPSRC_NAME}` is not an appsrc")]
    NotAnAppSrc,
    /// GStreamer setup failed.
    #[error("GStreamer audio pipeline error: {0}")]
    Pipeline(String),
    /// appsrc rejected a mixed frame.
    #[error("GStreamer appsrc rejected audio: {0}")]
    Flow(String),
    /// The playout task failed.
    #[error("audio playout task failed: {0}")]
    Task(String),
    /// The playout task stopped before applying a lifecycle command.
    #[error("audio playout has stopped")]
    TaskStopped,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gstreamer_app::AppSink;

    #[test]
    fn mixer_averages_tracks_and_rejects_wrong_frame_sizes() {
        let mut pending = [Some(vec![10_000, -10_000]), Some(vec![20_000, 10_000]), Some(vec![1])];
        let (mixed, rejected) = mix_pending_frames(&mut pending, 2);
        assert_eq!(mixed.unwrap(), vec![15_000, 0]);
        assert_eq!(rejected, 1);
        assert!(pending.iter().all(Option::is_none));
    }

    #[test]
    fn mixer_attach_detach_rejects_duplicates_and_stale_frames() {
        let mut mixer = MixerState::default();
        let first_generation = mixer.attach("track").unwrap();
        assert!(matches!(
            mixer.attach("track"),
            Err(GStreamerAudioPlayoutError::DuplicateTrack(id)) if id == "track"
        ));
        mixer.accept(TrackFrame {
            track_id: "track".into(),
            generation: first_generation,
            data: vec![100, 200],
        });
        assert_eq!(mixer.mix(2).0.unwrap(), vec![100, 200]);
        assert!(mixer.detach("track"));
        assert!(!mixer.detach("track"));

        let second_generation = mixer.attach("track").unwrap();
        mixer.accept(TrackFrame {
            track_id: "track".into(),
            generation: first_generation,
            data: vec![300, 400],
        });
        assert!(mixer.mix(2).0.is_none(), "late detached-reader frame must be ignored");
        mixer.accept(TrackFrame {
            track_id: "track".into(),
            generation: second_generation,
            data: vec![500, 600],
        });
        assert_eq!(mixer.mix(2).0.unwrap(), vec![500, 600]);
        assert!(!mixer.end("track", first_generation));
        assert!(mixer.end("track", second_generation));
        assert!(mixer.attach("track").is_ok());
    }

    #[tokio::test]
    async fn playout_starts_and_stops_without_tracks() {
        let playout = GStreamerAudioPlayout::start(
            GStreamerAudioPlayoutConfig {
                pipeline: "appsrc name=lk_audio_appsrc ! fakesink sync=false".into(),
                ..Default::default()
            },
            [],
        )
        .unwrap();
        let stats = playout.stop().await.unwrap();
        assert_eq!(stats, GStreamerAudioPlayoutStats::default());
    }

    #[test]
    fn synthetic_appsrc_pipeline_accepts_s16le_audio_and_eos() {
        gst::init().unwrap();
        let pipeline =
            gst::parse::launch("appsrc name=lk_audio_appsrc ! appsink name=verify sync=false")
                .unwrap()
                .downcast::<gst::Pipeline>()
                .unwrap();
        let appsrc = pipeline.by_name(APPSRC_NAME).unwrap().downcast::<gst_app::AppSrc>().unwrap();
        let appsink = pipeline.by_name("verify").unwrap().downcast::<AppSink>().unwrap();
        let caps: gst::Caps =
            "audio/x-raw,format=S16LE,layout=interleaved,rate=48000,channels=1".parse().unwrap();
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);
        pipeline.set_state(gst::State::Playing).unwrap();

        let mut buffer = gst::Buffer::with_size(960).unwrap();
        buffer.get_mut().unwrap().set_duration(gst::ClockTime::from_mseconds(10));
        appsrc.push_buffer(buffer).unwrap();
        let sample = appsink.try_pull_sample(gst::ClockTime::from_seconds(1)).unwrap();
        assert_eq!(sample.buffer().unwrap().size(), 960);

        appsrc.end_of_stream().unwrap();
        assert!(appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)).is_none());
        assert!(appsink.is_eos());
        pipeline.set_state(gst::State::Null).unwrap();
    }
}
