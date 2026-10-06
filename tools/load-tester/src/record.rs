use std::{
    ops::Range,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{
    media::{AudioProfile, VideoProfile},
    score::Scoring,
};

pub type UnixMs = u64;

pub const NULL_DECODER: &str = "NullVideoDecoder";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub u32);

impl ParticipantId {
    pub fn identity(self) -> String {
        format!("lt-{:05}", self.0)
    }

    pub fn parse(identity: &str) -> Option<Self> {
        identity.strip_prefix("lt-")?.parse().ok().map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slo {
    pub max_join_failures: u32,
    pub join_p95_ms: u32,
    pub ttff_p95_ms: u32,
    pub audio_mos_floor: f32,
    pub video_stall_ceiling: f32,
    pub max_disconnects: u32,
    pub max_missing_subscriptions: u32,
    pub video_score_floor: Option<f32>,
    pub delay_ceiling_ms: Option<f32>,
}

impl Default for Slo {
    fn default() -> Self {
        Self {
            max_join_failures: 0,
            join_p95_ms: 5_000,
            ttff_p95_ms: 3_000,
            audio_mos_floor: 3.5,
            video_stall_ceiling: 0.05,
            max_disconnects: 0,
            max_missing_subscriptions: 0,
            video_score_floor: None,
            delay_ceiling_ms: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TesterLimits {
    pub max_lag_ms: f32,
    pub max_thread_util: f32,
    pub max_cpu_share: f32,
    pub max_cpu_limited_layer_share: f32,
}

impl Default for TesterLimits {
    fn default() -> Self {
        Self {
            max_lag_ms: 100.0,
            max_thread_util: 0.85,
            max_cpu_share: 0.85,
            max_cpu_limited_layer_share: 0.10,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub url: String,
    pub room: String,
    pub publishers: u32,
    pub steps: Vec<u32>,
    pub video: VideoProfile,
    pub tiles: u32,
    pub tile_height: Option<u32>,
    pub null_video_decoder: bool,
    pub workers: u16,
    pub join_rate: f32,
    pub settle_s: u32,
    pub hold_s: u32,
    pub slo: Slo,
    pub limits: TesterLimits,
    pub audio: AudioProfile,
    pub scoring: Scoring,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            url: String::new(),
            room: "load-test".into(),
            publishers: 4,
            steps: (1..=10).map(|i| i * 10).collect(),
            video: VideoProfile::default(),
            tiles: 9,
            tile_height: None,
            null_video_decoder: false,
            workers: default_workers(),
            join_rate: 5.0,
            settle_s: 10,
            hold_s: 30,
            slo: Slo::default(),
            limits: TesterLimits::default(),
            audio: AudioProfile::default(),
            scoring: Scoring::default(),
        }
    }
}

fn default_workers() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 4).clamp(1, u16::MAX as usize) as u16
}

impl Settings {
    pub fn tile_height(&self) -> u32 {
        self.tile_height.unwrap_or(self.video.height)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.url.is_empty(),
            "url is not set: pass --url, export LIVEKIT_URL, or set url in the config file"
        );
        let increasing = self.steps.windows(2).all(|w| w[0] < w[1]);
        anyhow::ensure!(
            self.steps.first().is_some_and(|&first| first > 0) && increasing,
            "steps must be positive and strictly increasing"
        );
        anyhow::ensure!(
            self.publishers <= self.steps[0],
            "publishers {} must fit in the first step ({})",
            self.publishers,
            self.steps[0]
        );
        let positive = [
            ("workers", u32::from(self.workers)),
            ("video.width", self.video.width),
            ("video.height", self.video.height),
            ("video.fps", self.video.fps),
            ("tile_height", self.tile_height()),
        ];
        for (key, v) in positive {
            anyhow::ensure!(v > 0, "{key} must be positive");
        }
        anyhow::ensure!(self.join_rate > 0.0, "join_rate must be positive, got {}", self.join_rate);
        self.scoring.validate()?;
        match non_finite(&toml::Table::try_from(self)?) {
            Some(key) => anyhow::bail!("{key} must be a finite number"),
            None => Ok(()),
        }
    }

    pub fn is_publisher(&self, id: ParticipantId) -> bool {
        id.0 < self.publishers
    }

    pub fn wants(&self, sub: ParticipantId, publ: ParticipantId, kind: MediaKind) -> bool {
        sub != publ && self.is_publisher(publ) && (kind == MediaKind::Audio || publ.0 < self.tiles)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Record {
    Run(RunHeader),
    Step(StepRecord),
    Join(JoinRecord),
    Subscription(SubscriptionRecord),
    Window(WindowRecord),
    Uplink(UplinkRecord),
    Health(HealthRecord),
    Event(EventRecord),
    WorkerExit(WorkerExitRecord),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunHeader {
    pub version: String,
    pub started: UnixMs,
    pub cores: u32,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StepRecord {
    pub index: u32,
    pub participants: u32,
    pub first_new: u32,
    pub measure_start: UnixMs,
    pub measure_end: UnixMs,
    pub aborted: bool,
}

impl StepRecord {
    pub fn measures(&self, at: UnixMs, dur_ms: u32) -> bool {
        at.saturating_sub(dur_ms as u64) >= self.measure_start && at <= self.measure_end
    }

    pub fn new_ids(&self) -> Range<u32> {
        self.first_new..self.participants
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JoinRecord {
    pub at: UnixMs,
    pub id: ParticipantId,
    pub outcome: JoinOutcome,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JoinOutcome {
    Joined { connect_ms: u32 },
    Failed { error: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub at: UnixMs,
    pub sub: ParticipantId,
    pub publ: ParticipantId,
    pub track: String,
    pub kind: MediaKind,
    pub ttff_ms: u32,
    pub decoder: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowRecord {
    pub at: UnixMs,
    pub dur_ms: u32,
    pub sub: ParticipantId,
    pub publ: ParticipantId,
    pub track: String,
    pub rtt_ms: f32,
    pub media: MediaWindow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaWindow {
    Audio(AudioWindow),
    Video(VideoWindow),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioWindow {
    pub packets_received: u64,
    pub packets_lost: i64,
    pub samples: u64,
    pub concealed: u64,
    pub concealment_events: u64,
    pub jb_delay_s: f64,
    pub jb_emitted: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VideoWindow {
    pub packets_received: u64,
    pub packets_lost: i64,
    pub bytes: u64,
    pub frames_decoded: u32,
    pub frames_dropped: u32,
    pub freeze_count: u32,
    pub stalled_ms: u32,
    pub height: u32,
    pub expected_height: u32,
    pub layer_fps: f32,
    pub layers_below: u32,
    pub jb_delay_s: f64,
    pub jb_emitted: u64,
    pub nacks: u32,
    pub plis: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UplinkRecord {
    pub at: UnixMs,
    pub dur_ms: u32,
    pub id: ParticipantId,
    pub kind: MediaKind,
    pub layers: Vec<UplinkLayer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UplinkLayer {
    pub rid: String,
    pub bitrate_bps: u32,
    pub fps: f32,
    pub height: u32,
    pub limitation: Limitation,
    pub remote_loss: f32,
    pub rtt_ms: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Limitation {
    None,
    Cpu,
    Bandwidth,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthRecord {
    pub at: UnixMs,
    pub dur_ms: u32,
    pub worker: u16,
    pub cpu_cores: f32,
    pub lag_max_ms: f32,
    pub hottest_thread: Option<ThreadLoad>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadLoad {
    pub name: String,
    pub util: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventRecord {
    pub at: UnixMs,
    pub id: ParticipantId,
    pub event: ParticipantEvent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParticipantEvent {
    Reconnecting,
    Disconnected { reason: String },
    ServerQuality { about: ParticipantId, quality: ServerQuality },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerExitRecord {
    pub at: UnixMs,
    pub worker: u16,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkerInit {
    pub worker: u16,
    pub settings: Settings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JoinCommand {
    pub id: ParticipantId,
    pub token: String,
}

fn non_finite(table: &toml::Table) -> Option<String> {
    table.iter().find_map(|(key, value)| match value {
        toml::Value::Float(f) if !f.is_finite() => Some(key.clone()),
        toml::Value::Table(inner) => non_finite(inner).map(|path| format!("{key}.{path}")),
        _ => None,
    })
}

pub fn now_ms() -> UnixMs {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as UnixMs)
}
