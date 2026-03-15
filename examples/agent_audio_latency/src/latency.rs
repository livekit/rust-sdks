use log::info;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TurnLatencyBenchConfig {
    pub enabled: bool,
    pub user_speech_threshold_dbfs: f32,
    pub user_silence_hold: Duration,
    pub speaker_speech_threshold_dbfs: f32,
    pub speaker_confirm_duration: Duration,
    pub min_user_speech_duration: Duration,
    pub speaker_gap_tolerance: Duration,
    pub wait_for_speaker_timeout: Duration,
}

impl Default for TurnLatencyBenchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            user_speech_threshold_dbfs: -42.0,
            user_silence_hold: Duration::from_millis(250),
            speaker_speech_threshold_dbfs: -38.0,
            speaker_confirm_duration: Duration::from_millis(300),
            min_user_speech_duration: Duration::from_millis(350),
            speaker_gap_tolerance: Duration::from_millis(120),
            wait_for_speaker_timeout: Duration::from_secs(8),
        }
    }
}

#[derive(Debug)]
pub struct TurnLatencyBench {
    config: TurnLatencyBenchConfig,
    user_is_speaking: bool,
    user_speech_accum: Duration,
    user_silence_accum: Duration,
    waiting_for_speaker: bool,
    user_speech_end_at: Option<Instant>,
    speaker_loud_accum: Duration,
    speaker_quiet_accum: Duration,
    speaker_segment_start: Option<Instant>,
    latencies: Vec<Duration>,
}

impl TurnLatencyBench {
    pub fn new(config: TurnLatencyBenchConfig) -> Self {
        Self {
            config,
            user_is_speaking: false,
            user_speech_accum: Duration::ZERO,
            user_silence_accum: Duration::ZERO,
            waiting_for_speaker: false,
            user_speech_end_at: None,
            speaker_loud_accum: Duration::ZERO,
            speaker_quiet_accum: Duration::ZERO,
            speaker_segment_start: None,
            latencies: Vec::new(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn observe_user_audio(&mut self, samples: &[i16], sample_rate: u32) {
        if !self.config.enabled || samples.is_empty() {
            return;
        }

        let level_dbfs = rms_dbfs(samples);
        let chunk_duration = chunk_duration(samples.len(), sample_rate);
        let now = Instant::now();

        if level_dbfs >= self.config.user_speech_threshold_dbfs {
            if self.waiting_for_speaker {
                // Cancel the pending turn if the user starts speaking again before the
                // participant responds. This avoids printing repeated false "speech ended"
                // events while the user is still in the same turn.
                self.waiting_for_speaker = false;
                self.user_speech_end_at = None;
                self.reset_speaker_tracking();
            }

            self.user_is_speaking = true;
            self.user_speech_accum += chunk_duration;
            self.user_silence_accum = Duration::ZERO;
            return;
        }

        if !self.user_is_speaking {
            return;
        }

        self.user_silence_accum += chunk_duration;
        if self.user_silence_accum < self.config.user_silence_hold {
            return;
        }

        if self.user_speech_accum < self.config.min_user_speech_duration {
            self.user_is_speaking = false;
            self.user_speech_accum = Duration::ZERO;
            self.user_silence_accum = Duration::ZERO;
            return;
        }

        self.user_is_speaking = false;
        self.user_speech_accum = Duration::ZERO;
        self.user_silence_accum = Duration::ZERO;
        self.waiting_for_speaker = true;
        self.user_speech_end_at = Some(now);
        self.reset_speaker_tracking();
        info!("benchmark: detected end of user speech");
    }

    pub fn observe_speaker_audio(&mut self, samples: &[i16], sample_rate: u32) {
        if !self.config.enabled || !self.waiting_for_speaker || samples.is_empty() {
            return;
        }

        let level_dbfs = rms_dbfs(samples);
        let chunk_duration = chunk_duration(samples.len(), sample_rate);
        let now = Instant::now();
        let chunk_start = now.checked_sub(chunk_duration).unwrap_or(now);

        if let Some(user_end) = self.user_speech_end_at {
            if now.duration_since(user_end) > self.config.wait_for_speaker_timeout {
                self.waiting_for_speaker = false;
                self.user_speech_end_at = None;
                self.reset_speaker_tracking();
                return;
            }
        }

        if level_dbfs < self.config.speaker_speech_threshold_dbfs {
            if self.speaker_segment_start.is_some() {
                self.speaker_quiet_accum += chunk_duration;
                if self.speaker_quiet_accum > self.config.speaker_gap_tolerance {
                    self.reset_speaker_tracking();
                }
            }
            return;
        }

        if self.speaker_segment_start.is_none() {
            self.speaker_segment_start = Some(chunk_start);
        }
        self.speaker_quiet_accum = Duration::ZERO;
        self.speaker_loud_accum += chunk_duration;

        if self.speaker_loud_accum < self.config.speaker_confirm_duration {
            return;
        }

        let Some(user_end) = self.user_speech_end_at.take() else {
            return;
        };
        let speaker_start = self.speaker_segment_start.unwrap_or(chunk_start);
        let latency = speaker_start.saturating_duration_since(user_end);
        self.latencies.push(latency);
        self.waiting_for_speaker = false;
        self.reset_speaker_tracking();

        info!(
            "benchmark: detected start of speaker audio after user speech end, latency={:.1} ms | {}",
            latency.as_secs_f64() * 1000.0,
            self.summary()
        );
    }

    fn reset_speaker_tracking(&mut self) {
        self.speaker_loud_accum = Duration::ZERO;
        self.speaker_quiet_accum = Duration::ZERO;
        self.speaker_segment_start = None;
    }

    fn summary(&self) -> String {
        if self.latencies.is_empty() {
            return "n=0".to_string();
        }

        let mut sorted = self.latencies.clone();
        sorted.sort_unstable();
        let min = sorted[0];
        let max = sorted[sorted.len() - 1];
        let avg = self.latencies.iter().map(Duration::as_secs_f64).sum::<f64>()
            / self.latencies.len() as f64;
        let p50 = percentile(&sorted, 0.50);
        let p95 = percentile(&sorted, 0.95);

        format!(
            "n={} avg={:.1}ms p50={:.1}ms p95={:.1}ms min={:.1}ms max={:.1}ms",
            self.latencies.len(),
            avg * 1000.0,
            p50.as_secs_f64() * 1000.0,
            p95.as_secs_f64() * 1000.0,
            min.as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0
        )
    }
}

fn chunk_duration(num_samples: usize, sample_rate: u32) -> Duration {
    Duration::from_secs_f64(num_samples as f64 / sample_rate as f64)
}

fn rms_dbfs(samples: &[i16]) -> f32 {
    let mean_square = samples
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>()
        / samples.len() as f32;

    if mean_square <= 0.0 {
        -96.0
    } else {
        10.0 * mean_square.log10()
    }
}

fn percentile(sorted: &[Duration], q: f64) -> Duration {
    let index = ((sorted.len().saturating_sub(1)) as f64 * q).round() as usize;
    sorted[index]
}
