use std::{
    ops::Range,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::media::VideoProfile;

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
pub struct Slo {
    pub max_join_failures: u32,
    pub join_p95_ms: u32,
    pub ttff_p95_ms: u32,
    pub audio_mos_floor: f32,
    pub video_stall_ceiling: f32,
}

impl Default for Slo {
    fn default() -> Self {
        Self {
            max_join_failures: 0,
            join_p95_ms: 5_000,
            ttff_p95_ms: 3_000,
            audio_mos_floor: 3.5,
            video_stall_ceiling: 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub url: String,
    pub room: String,
    pub publishers: u32,
    pub steps: Vec<u32>,
    pub video: VideoProfile,
    pub tiles: u32,
    pub tile_height: u32,
    pub null_video_decoder: bool,
    pub workers: u16,
    pub join_rate: f32,
    pub settle_s: u32,
    pub hold_s: u32,
    pub slo: Slo,
    pub limits: TesterLimits,
}

impl Settings {
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

pub fn now_ms() -> UnixMs {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as UnixMs)
}
