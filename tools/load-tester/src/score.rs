use crate::record::{AudioWindow, MediaWindow, VideoWindow, WindowRecord};

pub use sfu::mos;

mod sfu {
    pub const AUDIO_CONCEALMENT_WEIGHT: f32 = 8.0;
    pub const VIDEO_LOSS_WEIGHT: f32 = 10.0;
    pub const LAYER_STEP_PENALTY: f32 = 35.0;

    pub fn delay_effect(d_ms: f32) -> f32 {
        if d_ms <= 160.0 {
            d_ms / 40.0
        } else {
            (d_ms - 120.0) / 10.0
        }
    }

    pub fn mos(score: f32) -> f32 {
        let s = score;
        (1.0 + 0.035 * s + 0.000007 * s * (s - 60.0) * (100.0 - s)).clamp(1.0, 4.5)
    }
}

const FPS_STEP_PENALTY: f32 = sfu::LAYER_STEP_PENALTY;
const FREEZE_WEIGHT: f32 = 4.0;
const MIN_LIVE_S: f32 = 0.5;

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

pub fn score(w: &WindowRecord) -> Score {
    match &w.media {
        MediaWindow::Audio(a) => audio(a, w.rtt_ms),
        MediaWindow::Video(v) => video(v, w.rtt_ms, w.dur_ms),
    }
}

pub fn stalled_share(v: &VideoWindow, dur_ms: u32) -> f32 {
    if dur_ms == 0 {
        return 0.0;
    }
    (v.stalled_ms as f32 / dur_ms as f32).min(1.0)
}

fn audio(a: &AudioWindow, rtt_ms: f32) -> Score {
    if a.samples == 0 {
        return Score::new(0.0, Reason::Concealment);
    }
    let concealed_pct = pct(a.concealed, a.samples);
    let delay_ms = rtt_ms / 2.0 + mean_ms(a.jb_delay_s, a.jb_emitted);
    packet(
        concealed_pct * sfu::AUDIO_CONCEALMENT_WEIGHT,
        sfu::delay_effect(delay_ms),
        Reason::Concealment,
    )
}

fn video(v: &VideoWindow, rtt_ms: f32, dur_ms: u32) -> Score {
    let lost = v.packets_lost.max(0) as u64;
    let loss_pct = pct(lost, v.packets_received + lost);
    let delay_ms = rtt_ms / 2.0 + mean_ms(v.jb_delay_s, v.jb_emitted);

    let mut worst =
        packet(loss_pct * sfu::VIDEO_LOSS_WEIGHT, sfu::delay_effect(delay_ms), Reason::Loss);
    let mut consider = |s: Score| {
        if s.value < worst.value {
            worst = s;
        }
    };

    let stalled = stalled_share(v, dur_ms);
    consider(Score::new(100.0 - stalled * 100.0 * FREEZE_WEIGHT, Reason::Freeze));

    let live_s = dur_ms.saturating_sub(v.stalled_ms) as f32 / 1000.0;
    if live_s >= MIN_LIVE_S && v.layer_fps > 0.0 {
        let fps = v.frames_decoded as f32 / live_s;
        consider(Score::new(100.0 - FPS_STEP_PENALTY * halvings(v.layer_fps, fps), Reason::Fps));
    }

    consider(Score::new(100.0 - sfu::LAYER_STEP_PENALTY * v.layers_below as f32, Reason::Layer));

    worst
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
        assert!(near(sfu::delay_effect(160.0), 4.0));
        assert!(near(sfu::delay_effect(200.0), 8.0));
    }
}
