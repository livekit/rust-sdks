use serde::{Deserialize, Serialize};

use crate::record::{AudioWindow, MediaWindow, VideoWindow, WindowRecord};

pub use sfu::mos;

mod sfu {
    pub const AUDIO_CONCEALMENT_WEIGHT: f32 = 8.0;
    pub const VIDEO_LOSS_WEIGHT: f32 = 10.0;
    pub const LAYER_STEP_PENALTY: f32 = 35.0;
    pub const DELAY_KNEE_MS: f32 = 160.0;
    pub const DELAY_MS_PER_POINT_BELOW_KNEE: f32 = 40.0;
    pub const DELAY_MS_PER_POINT_ABOVE_KNEE: f32 = 10.0;

    pub fn mos(score: f32) -> f32 {
        let s = score;
        (1.0 + 0.035 * s + 0.000007 * s * (s - 60.0) * (100.0 - s)).clamp(1.0, 4.5)
    }
}

const FPS_STEP_PENALTY: f32 = sfu::LAYER_STEP_PENALTY;
const FREEZE_WEIGHT: f32 = 4.0;
const MIN_LIVE_S: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scoring {
    pub audio_concealment_weight: f32,
    pub video_loss_weight: f32,
    pub layer_step_penalty: f32,
    pub fps_step_penalty: f32,
    pub freeze_weight: f32,
    pub delay_knee_ms: f32,
    pub delay_ms_per_point_below_knee: f32,
    pub delay_ms_per_point_above_knee: f32,
}

impl Default for Scoring {
    fn default() -> Self {
        Self {
            audio_concealment_weight: sfu::AUDIO_CONCEALMENT_WEIGHT,
            video_loss_weight: sfu::VIDEO_LOSS_WEIGHT,
            layer_step_penalty: sfu::LAYER_STEP_PENALTY,
            fps_step_penalty: FPS_STEP_PENALTY,
            freeze_weight: FREEZE_WEIGHT,
            delay_knee_ms: sfu::DELAY_KNEE_MS,
            delay_ms_per_point_below_knee: sfu::DELAY_MS_PER_POINT_BELOW_KNEE,
            delay_ms_per_point_above_knee: sfu::DELAY_MS_PER_POINT_ABOVE_KNEE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    Concealment,
    Loss,
    Delay,
    Freeze,
    Fps,
    Layer,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Concealment => "concealment",
            Reason::Loss => "loss",
            Reason::Delay => "delay",
            Reason::Freeze => "freeze",
            Reason::Fps => "fps",
            Reason::Layer => "layer",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub value: f32,
    pub reason: Reason,
}

impl Score {
    fn new(value: f32, reason: Reason) -> Self {
        Self { value: value.clamp(0.0, 100.0), reason }
    }

    pub fn deficit(self) -> f32 {
        100.0 - self.value
    }
}

pub fn stalled_share(v: &VideoWindow, dur_ms: u32) -> f32 {
    if dur_ms == 0 {
        return 0.0;
    }
    (v.stalled_ms as f32 / dur_ms as f32).min(1.0)
}

pub fn one_way_delay_ms(w: &WindowRecord) -> f32 {
    let (jb_delay_s, jb_emitted) = match &w.media {
        MediaWindow::Audio(a) => (a.jb_delay_s, a.jb_emitted),
        MediaWindow::Video(v) => (v.jb_delay_s, v.jb_emitted),
    };
    w.rtt_ms / 2.0 + mean_ms(jb_delay_s, jb_emitted)
}

impl Scoring {
    pub fn score(&self, w: &WindowRecord) -> Score {
        let delay_penalty = self.delay_effect(one_way_delay_ms(w));
        match &w.media {
            MediaWindow::Audio(a) => self.audio(a, delay_penalty),
            MediaWindow::Video(v) => self.video(v, delay_penalty, w.dur_ms),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let slopes = [
            ("scoring.delay_ms_per_point_below_knee", self.delay_ms_per_point_below_knee),
            ("scoring.delay_ms_per_point_above_knee", self.delay_ms_per_point_above_knee),
        ];
        for (key, v) in slopes {
            anyhow::ensure!(v > 0.0, "{key} must be positive, got {v}");
        }
        Ok(())
    }

    fn audio(&self, a: &AudioWindow, delay_penalty: f32) -> Score {
        if a.samples == 0 {
            return Score::new(0.0, Reason::Concealment);
        }
        let concealed_pct = pct(a.concealed, a.samples);
        packet(concealed_pct * self.audio_concealment_weight, delay_penalty, Reason::Concealment)
    }

    fn video(&self, v: &VideoWindow, delay_penalty: f32, dur_ms: u32) -> Score {
        let lost = v.packets_lost.max(0) as u64;
        let loss_pct = pct(lost, v.packets_received + lost);

        let mut worst = packet(loss_pct * self.video_loss_weight, delay_penalty, Reason::Loss);
        let mut consider = |s: Score| {
            if s.value < worst.value {
                worst = s;
            }
        };

        let stalled = stalled_share(v, dur_ms);
        consider(Score::new(100.0 - stalled * 100.0 * self.freeze_weight, Reason::Freeze));

        let live_s = dur_ms.saturating_sub(v.stalled_ms) as f32 / 1000.0;
        if live_s >= MIN_LIVE_S && v.layer_fps > 0.0 {
            let fps = v.frames_decoded as f32 / live_s;
            let penalty = self.fps_step_penalty * halvings(v.layer_fps, fps);
            consider(Score::new(100.0 - penalty, Reason::Fps));
        }

        let penalty = self.layer_step_penalty * v.layers_below as f32;
        consider(Score::new(100.0 - penalty, Reason::Layer));

        worst
    }

    fn delay_effect(&self, d_ms: f32) -> f32 {
        let at_knee = self.delay_knee_ms / self.delay_ms_per_point_below_knee;
        if d_ms <= self.delay_knee_ms {
            d_ms / self.delay_ms_per_point_below_knee
        } else {
            at_knee + (d_ms - self.delay_knee_ms) / self.delay_ms_per_point_above_knee
        }
    }
}

fn packet(loss_penalty: f32, delay_penalty: f32, loss_reason: Reason) -> Score {
    let reason = if loss_penalty >= delay_penalty { loss_reason } else { Reason::Delay };
    Score::new(100.0 - loss_penalty - delay_penalty, reason)
}

fn halvings(expected: f32, actual: f32) -> f32 {
    if actual <= 0.0 {
        return f32::INFINITY;
    }
    (expected / actual).log2().max(0.0)
}

fn pct(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    part as f32 * 100.0 / whole as f32
}

fn mean_ms(total_s: f64, count: u64) -> f32 {
    if count == 0 {
        return 0.0;
    }
    (total_s * 1000.0 / count as f64) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn scoring_matches_the_sfu_anchor_points() {
        assert!(near(mos(80.0), 4.024), "got {}", mos(80.0));
        assert_eq!(mos(0.0), 1.0);
        assert_eq!(mos(100.0), 4.5);
        let sfu = Scoring::default();
        assert!(near(sfu.delay_effect(160.0), 4.0));
        assert!(near(sfu.delay_effect(200.0), 8.0));
    }
}
