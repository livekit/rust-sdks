// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0

//! Pumps PCM frames into a native RTC audio source.

use super::PcmAudioSource;
use crate::pump::{spawn_pump, PumpError, PumpExit, PumpStats, PumpStop, RunningPump};
use livekit::{
    options::TrackPublishOptions,
    webrtc::{
        audio_frame::AudioFrame,
        audio_source::{native::NativeAudioSource, AudioSourceOptions, RtcAudioSource},
    },
};
use std::{borrow::Cow, fmt, io};

/// Pumps fixed-format PCM audio into a [`NativeAudioSource`].
pub struct AudioPump<S: PcmAudioSource> {
    source: S,
    rtc_source: NativeAudioSource,
    stop: PumpStop,
}

impl<S: PcmAudioSource> AudioPump<S> {
    /// Creates a pump with a bounded 100 ms RTC input queue.
    pub fn new(source: S) -> Self {
        Self::with_queue_size(source, 100)
    }

    /// Creates a pump with the requested RTC input queue duration.
    ///
    /// `queue_size_ms` must be a multiple of 10. A bounded queue applies
    /// backpressure to the producer rather than accumulating audio.
    pub fn with_queue_size(source: S, queue_size_ms: u32) -> Self {
        let format = source.format();
        let rtc_source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            format.sample_rate,
            format.num_channels,
            queue_size_ms,
        );
        Self { source, rtc_source, stop: PumpStop::new() }
    }

    /// Returns the RTC source used to create a local audio track.
    pub fn rtc_source(&self) -> RtcAudioSource {
        RtcAudioSource::Native(self.rtc_source.clone())
    }

    /// Returns recommended publish options for PCM audio.
    pub fn publish_options(&self) -> TrackPublishOptions {
        TrackPublishOptions::default()
    }

    /// Returns the immutable stream format.
    pub fn format(&self) -> super::AudioFormat {
        self.source.format()
    }

    /// Returns the underlying source.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Returns a cancellation handle.
    pub fn stop_handle(&self) -> PumpStop {
        self.stop.clone()
    }

    /// Runs capture on the calling thread until stop, EOS, or failure.
    pub fn run(mut self) -> Result<PumpStats, PumpError> {
        let format = self.source.format();
        let mut stats = PumpStats { exit: PumpExit::EndOfStream, ..PumpStats::default() };

        loop {
            if self.stop.is_stopped() {
                stats.exit = PumpExit::Stopped;
                break;
            }
            let Some(frame) = self.source.next_frame(&self.stop)? else {
                stats.exit =
                    if self.stop.is_stopped() { PumpExit::Stopped } else { PumpExit::EndOfStream };
                break;
            };
            if frame.data.len() != format.samples_per_frame() {
                return Err(PumpError::Source(crate::error::SourceError::new(InvalidAudioFrame {
                    expected: format.samples_per_frame(),
                    actual: frame.data.len(),
                })));
            }

            let rtc_frame = AudioFrame {
                data: Cow::Borrowed(frame.data.as_ref()),
                sample_rate: format.sample_rate,
                num_channels: format.num_channels,
                samples_per_channel: format.samples_per_channel,
            };
            pollster::block_on(self.rtc_source.capture_frame(&rtc_frame))
                .map_err(|_| PumpError::CaptureFailed)?;

            stats.frames_captured += 1;
            stats.samples_captured += u64::from(format.samples_per_channel);
            stats.bytes_captured += (frame.data.len() * std::mem::size_of::<i16>()) as u64;
            stats.first_timestamp_ns.get_or_insert(frame.timestamp_ns);
            stats.last_timestamp_ns = Some(frame.timestamp_ns);
        }
        Ok(stats)
    }

    /// Runs capture on a dedicated thread.
    pub fn spawn(self) -> io::Result<RunningPump>
    where
        S: 'static,
    {
        let stop = self.stop_handle();
        spawn_pump(stop, move || self.run())
    }
}

impl<S: PcmAudioSource> fmt::Debug for AudioPump<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioPump")
            .field("format", &self.source.format())
            .field("stop", &self.stop)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("audio source returned {actual} samples; expected exactly {expected}")]
struct InvalidAudioFrame {
    expected: usize,
    actual: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFormat, PcmAudioFrame};
    use std::collections::VecDeque;

    struct FakeSource(VecDeque<PcmAudioFrame<'static>>);

    impl PcmAudioSource for FakeSource {
        fn format(&self) -> AudioFormat {
            AudioFormat::default()
        }

        fn next_frame(
            &mut self,
            _stop: &PumpStop,
        ) -> Result<Option<PcmAudioFrame<'static>>, crate::error::SourceError> {
            Ok(self.0.pop_front())
        }
    }

    #[tokio::test]
    async fn pump_reports_pcm_stats() {
        let frames = [0, 10_000_000, 20_000_000]
            .map(|timestamp_ns| PcmAudioFrame { data: vec![1; 480].into(), timestamp_ns });
        let stats = AudioPump::with_queue_size(FakeSource(frames.into()), 0).run().unwrap();
        assert_eq!(stats.frames_captured, 3);
        assert_eq!(stats.samples_captured, 1_440);
        assert_eq!(stats.bytes_captured, 2_880);
        assert_eq!(stats.first_timestamp_ns, Some(0));
        assert_eq!(stats.last_timestamp_ns, Some(20_000_000));
        assert_eq!(stats.exit, PumpExit::EndOfStream);
    }
}
