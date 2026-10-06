use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::BufReader,
    path::Path,
};

use anyhow::{bail, Context};

use crate::{
    record::{
        HealthRecord, JoinOutcome, Limitation, MediaKind, MediaWindow, ParticipantEvent,
        ParticipantId, Record, RunHeader, ServerQuality, Settings, Slo, StepRecord, TesterLimits,
        ThreadLoad, WorkerExitRecord, NULL_DECODER,
    },
    score::{self, Reason, Score},
};

const WORST_LISTED: usize = 5;

enum Tail {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rollup {
    pub n: usize,
    pub p50: f32,
    pub tail: f32,
    pub worst: f32,
}

impl Rollup {
    fn of(values: &mut [f32], bad: Tail) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        values.sort_by(f32::total_cmp);
        let n = values.len();
        let (tail, worst) = match bad {
            Tail::High => (values[nearest_rank(n, 95)], values[n - 1]),
            Tail::Low => (values[nearest_rank(n, 5)], values[0]),
        };
        Some(Self { n, p50: values[nearest_rank(n, 50)], tail, worst })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreRollup {
    pub score: Rollup,
    pub dominant: Option<Reason>,
}

impl ScoreRollup {
    fn of(scores: &[Score]) -> Option<Self> {
        let mut values: Vec<f32> = scores.iter().map(|s| s.value).collect();
        let score = Rollup::of(&mut values, Tail::Low)?;
        let mut deficits: BTreeMap<Reason, f32> = BTreeMap::new();
        for s in scores {
            *deficits.entry(s.reason).or_default() += s.deficit();
        }
        let dominant = deficits
            .into_iter()
            .filter(|(_, d)| *d > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(r, _)| r);
        Some(Self { score, dominant })
    }
}

#[derive(Debug)]
pub struct ParticipantRollup {
    pub id: ParticipantId,
    pub audio: Option<ScoreRollup>,
    pub video: Option<ScoreRollup>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkerHealth {
    pub samples: u32,
    pub mean_cpu_cores: f32,
    pub lag_max_ms: f32,
    pub hottest_thread: Option<ThreadLoad>,
}

impl WorkerHealth {
    fn add(&mut self, h: &HealthRecord) {
        self.samples += 1;
        self.mean_cpu_cores += (h.cpu_cores - self.mean_cpu_cores) / self.samples as f32;
        self.lag_max_ms = self.lag_max_ms.max(h.lag_max_ms);
        if let Some(t) = &h.hottest_thread {
            if self.hottest_thread.as_ref().is_none_or(|hot| t.util > hot.util) {
                self.hottest_thread = Some(t.clone());
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct StepReport {
    pub step: StepRecord,
    pub joins_attempted: u32,
    pub join_failures: u32,
    pub disconnects: u32,
    pub missing_subscriptions: u32,
    pub join_ms: Option<Rollup>,
    pub ttff_ms: Option<Rollup>,
    pub audio: Option<ScoreRollup>,
    pub video: Option<ScoreRollup>,
    pub stalled_share: Option<Rollup>,
    pub one_way_delay_ms: Option<Rollup>,
    pub participants: Vec<ParticipantRollup>,
    pub server_poor: u32,
    pub server_lost: u32,
    pub decoder_mismatches: u32,
    pub workers: BTreeMap<u16, WorkerHealth>,
    pub cpu_share: Option<f32>,
    pub cpu_limited_layer_share: Option<f32>,
    pub worker_exits: Vec<WorkerExitRecord>,
    pub verdict: Verdict,
}

#[derive(Debug, Default)]
pub enum Verdict {
    #[default]
    Pass,
    Fail(Vec<String>),
    Invalid(Vec<String>),
}

#[derive(Debug, PartialEq)]
pub enum Capacity {
    SfuLimited { ok: Option<u32>, failed_at: u32 },
    TesterLimited { ok: Option<u32>, invalid_at: u32 },
    NotReached { ok: u32 },
}

#[derive(Debug)]
pub struct RunReport {
    pub header: RunHeader,
    pub steps: Vec<StepReport>,
    pub capacity: Option<Capacity>,
}

pub struct Ledger {
    header: RunHeader,
    roster: BTreeSet<ParticipantId>,
    pending: Vec<Record>,
    steps: Vec<StepReport>,
}

#[derive(Default)]
struct Scores {
    audio: Vec<Score>,
    video: Vec<Score>,
}

impl Ledger {
    pub fn new(header: RunHeader) -> Self {
        Self { header, roster: BTreeSet::new(), pending: Vec::new(), steps: Vec::new() }
    }

    pub fn push(&mut self, record: Record) -> Option<&StepReport> {
        if let Record::Step(step) = record {
            let report = self.evaluate(step);
            self.pending.clear();
            self.steps.push(report);
            return self.steps.last();
        }
        match &record {
            Record::Join(j) if matches!(j.outcome, JoinOutcome::Joined { .. }) => {
                self.roster.insert(j.id);
            }
            Record::Event(e) if matches!(e.event, ParticipantEvent::Disconnected { .. }) => {
                self.roster.remove(&e.id);
            }
            _ => {}
        }
        self.pending.push(record);
        None
    }

    pub fn finish(self) -> RunReport {
        RunReport { capacity: capacity(&self.steps), header: self.header, steps: self.steps }
    }

    fn evaluate(&self, step: StepRecord) -> StepReport {
        let settings = &self.header.settings;
        let mut report =
            StepReport { joins_attempted: step.new_ids().len() as u32, ..Default::default() };
        let mut joined = BTreeSet::new();
        let mut join_ms = Vec::new();
        let mut ttff_ms = Vec::new();
        let mut server_quality = BTreeMap::new();
        let mut scores: BTreeMap<ParticipantId, Scores> = BTreeMap::new();
        let mut stalled = Vec::new();
        let mut delays = Vec::new();
        let mut delivered = BTreeSet::new();
        let (mut cpu_limited, mut uplink_layers) = (0u32, 0u32);

        for record in &self.pending {
            match record {
                Record::Join(j) if step.new_ids().contains(&j.id.0) => {
                    if let JoinOutcome::Joined { connect_ms } = j.outcome {
                        joined.insert(j.id);
                        join_ms.push(connect_ms as f32);
                    }
                }
                Record::Subscription(s) => {
                    ttff_ms.push(s.ttff_ms as f32);
                    let wrong_decoder = settings.null_video_decoder
                        && s.kind == MediaKind::Video
                        && s.decoder.as_deref() != Some(NULL_DECODER);
                    report.decoder_mismatches += u32::from(wrong_decoder);
                }
                Record::Event(e) => match &e.event {
                    ParticipantEvent::Reconnecting | ParticipantEvent::Disconnected { .. } => {
                        report.disconnects += 1
                    }
                    ParticipantEvent::ServerQuality { about, quality } => {
                        server_quality.insert(*about, *quality);
                    }
                },
                Record::Window(w) if step.measures(w.at, w.dur_ms) => {
                    let scored = settings.scoring.score(w);
                    delays.push(score::one_way_delay_ms(w));
                    let of_sub = scores.entry(w.sub).or_default();
                    match &w.media {
                        MediaWindow::Audio(_) => {
                            delivered.insert((w.sub, w.publ, MediaKind::Audio));
                            of_sub.audio.push(scored);
                        }
                        MediaWindow::Video(v) => {
                            delivered.insert((w.sub, w.publ, MediaKind::Video));
                            of_sub.video.push(scored);
                            stalled.push(score::stalled_share(v, w.dur_ms));
                        }
                    }
                }
                Record::Uplink(u)
                    if u.kind == MediaKind::Video && step.measures(u.at, u.dur_ms) =>
                {
                    uplink_layers += u.layers.len() as u32;
                    cpu_limited +=
                        u.layers.iter().filter(|l| l.limitation == Limitation::Cpu).count() as u32;
                }
                Record::Health(h) => report.workers.entry(h.worker).or_default().add(h),
                Record::WorkerExit(w) => report.worker_exits.push(w.clone()),
                _ => {}
            }
        }

        report.join_failures = report.joins_attempted.saturating_sub(joined.len() as u32);
        for &sub in &self.roster {
            for &publ in self.roster.range(..ParticipantId(settings.publishers)) {
                for kind in [MediaKind::Audio, MediaKind::Video] {
                    if settings.wants(sub, publ, kind) && !delivered.contains(&(sub, publ, kind)) {
                        report.missing_subscriptions += 1;
                    }
                }
            }
        }

        let all_audio: Vec<Score> = scores.values().flat_map(|s| &s.audio).copied().collect();
        let all_video: Vec<Score> = scores.values().flat_map(|s| &s.video).copied().collect();
        report.audio = ScoreRollup::of(&all_audio);
        report.video = ScoreRollup::of(&all_video);
        report.participants = scores
            .iter()
            .map(|(&id, s)| ParticipantRollup {
                id,
                audio: ScoreRollup::of(&s.audio),
                video: ScoreRollup::of(&s.video),
            })
            .collect();
        report.join_ms = Rollup::of(&mut join_ms, Tail::High);
        report.ttff_ms = Rollup::of(&mut ttff_ms, Tail::High);
        report.stalled_share = Rollup::of(&mut stalled, Tail::High);
        report.one_way_delay_ms = Rollup::of(&mut delays, Tail::High);
        report.server_poor = count_quality(&server_quality, ServerQuality::Poor);
        report.server_lost = count_quality(&server_quality, ServerQuality::Lost);
        let cores = self.header.cores.max(1) as f32;
        report.cpu_share = (!report.workers.is_empty())
            .then(|| report.workers.values().map(|w| w.mean_cpu_cores).sum::<f32>() / cores);
        report.cpu_limited_layer_share =
            (uplink_layers > 0).then(|| cpu_limited as f32 / uplink_layers as f32);
        report.step = step;

        let issues = tester_issues(&report, &settings.limits);
        report.verdict = if !issues.is_empty() {
            Verdict::Invalid(issues)
        } else {
            let found = breaches(&report, &settings.slo);
            if found.is_empty() {
                Verdict::Pass
            } else {
                Verdict::Fail(found)
            }
        };
        report
    }
}

fn count_quality(latest: &BTreeMap<ParticipantId, ServerQuality>, wanted: ServerQuality) -> u32 {
    latest.values().filter(|q| **q == wanted).count() as u32
}

fn breaches(report: &StepReport, slo: &Slo) -> Vec<String> {
    let mut found = Vec::new();
    if report.join_failures > slo.max_join_failures {
        found.push(format!(
            "{} join failures > {} allowed",
            report.join_failures, slo.max_join_failures
        ));
    }
    if report.disconnects > slo.max_disconnects {
        found.push(format!("{} disconnects > {} allowed", report.disconnects, slo.max_disconnects));
    }
    if report.missing_subscriptions > slo.max_missing_subscriptions {
        found.push(format!(
            "{} subscriptions never delivered media > {} allowed",
            report.missing_subscriptions, slo.max_missing_subscriptions
        ));
    }
    if let Some(tail) = report.join_ms.map(|r| r.tail).filter(|t| *t > slo.join_p95_ms as f32) {
        found.push(format!("join p95 {} > {}", secs(tail), secs(slo.join_p95_ms as f32)));
    }
    if let Some(tail) = report.ttff_ms.map(|r| r.tail).filter(|t| *t > slo.ttff_p95_ms as f32) {
        found.push(format!("ttff p95 {} > {}", secs(tail), secs(slo.ttff_p95_ms as f32)));
    }
    if let Some(mos) =
        report.audio.as_ref().map(|a| score::mos(a.score.tail)).filter(|m| *m < slo.audio_mos_floor)
    {
        found.push(format!("audio MOS p95 {mos:.2} < {:.2}", slo.audio_mos_floor));
    }
    if let (Some(floor), Some(video)) = (slo.video_score_floor, &report.video) {
        if video.score.tail < floor {
            found.push(format!("video score p95 {:.0} < {floor:.0}", video.score.tail));
        }
    }
    if let Some(tail) =
        report.stalled_share.map(|r| r.tail).filter(|t| *t > slo.video_stall_ceiling)
    {
        found.push(format!(
            "video stalled p95 {:.1}% > {:.1}%",
            tail * 100.0,
            slo.video_stall_ceiling * 100.0
        ));
    }
    if let (Some(ceiling), Some(delay)) = (slo.delay_ceiling_ms, report.one_way_delay_ms) {
        if delay.tail > ceiling {
            found.push(format!("one-way delay p95 {:.0}ms > {ceiling:.0}ms", delay.tail));
        }
    }
    found
}

fn tester_issues(report: &StepReport, limits: &TesterLimits) -> Vec<String> {
    let mut issues = Vec::new();
    if report.step.aborted {
        issues.push("run aborted mid-step".to_string());
    }
    for w in &report.worker_exits {
        issues.push(format!("worker {} exited ({})", w.worker, w.status));
    }
    for (worker, h) in &report.workers {
        if h.lag_max_ms > limits.max_lag_ms {
            issues.push(format!("tester lag {:.0}ms on worker {worker}", h.lag_max_ms));
        }
        if let Some(t) = h.hottest_thread.as_ref().filter(|t| t.util > limits.max_thread_util) {
            issues.push(format!("{} {:.0}% on worker {worker}", t.name, t.util * 100.0));
        }
    }
    if let Some(share) = report.cpu_share.filter(|s| *s > limits.max_cpu_share) {
        issues.push(format!("tester CPU {:.0}% of the machine", share * 100.0));
    }
    if let Some(share) =
        report.cpu_limited_layer_share.filter(|s| *s > limits.max_cpu_limited_layer_share)
    {
        issues.push(format!("{:.0}% of publisher layer-windows CPU-limited", share * 100.0));
    }
    issues
}

pub fn capacity(steps: &[StepReport]) -> Option<Capacity> {
    if steps.is_empty() {
        return None;
    }
    let mut ok = None;
    for s in steps {
        match s.verdict {
            Verdict::Pass => ok = Some(s.step.participants),
            Verdict::Fail(_) => {
                return Some(Capacity::SfuLimited { ok, failed_at: s.step.participants })
            }
            Verdict::Invalid(_) => {
                return Some(Capacity::TesterLimited { ok, invalid_at: s.step.participants })
            }
        }
    }
    Some(Capacity::NotReached { ok: ok.unwrap_or(0) })
}

fn nearest_rank(sorted_len: usize, pct: usize) -> usize {
    (sorted_len * pct).div_ceil(100).max(1) - 1
}

fn secs(ms: f32) -> String {
    format!("{:.2}s", ms / 1000.0)
}

fn reason_suffix(r: &ScoreRollup) -> String {
    r.dominant.map_or(String::new(), |reason| format!(" ({})", reason.as_str()))
}

impl StepReport {
    fn hottest_thread(&self) -> Option<(u16, &ThreadLoad)> {
        self.workers
            .iter()
            .filter_map(|(&w, h)| h.hottest_thread.as_ref().map(|t| (w, t)))
            .max_by(|a, b| a.1.util.total_cmp(&b.1.util))
    }

    fn notes(&self) -> Option<String> {
        match &self.verdict {
            Verdict::Pass => None,
            Verdict::Fail(items) | Verdict::Invalid(items) => {
                Some(format!("       {}", items.join("; ")))
            }
        }
    }

    fn tester_cell(&self) -> String {
        if let Some((worker, t)) = self.hottest_thread() {
            return format!("{} {:.0}% (w{worker})", t.name, t.util * 100.0);
        }
        self.cpu_share.map_or("-".into(), |s| format!("cpu {:.0}%", s * 100.0))
    }
}

fn row(cells: [&str; 9]) -> String {
    let [step, people, verdict, join, ttff, audio, video, freeze, tester] = cells;
    format!(
        "{step:<5} {people:<7} {verdict:<8} {join:>8} {ttff:>8}  {audio:<30} {video:<28} {freeze:>10}  {tester}"
    )
}

pub fn step_header() -> String {
    row([
        "step",
        "people",
        "verdict",
        "join p95",
        "ttff p95",
        "audio MOS p50/p95/worst",
        "video score p50/p95/worst",
        "stall p95",
        "hottest thread",
    ])
}

pub fn step_line(step: &StepReport) -> String {
    let verdict = match step.verdict {
        Verdict::Pass => "pass",
        Verdict::Fail(_) => "FAIL",
        Verdict::Invalid(_) => "INVALID",
    };
    let audio = step.audio.as_ref().map_or("-".into(), |a| {
        format!(
            "{:.2}/{:.2}/{:.2}{}",
            score::mos(a.score.p50),
            score::mos(a.score.tail),
            score::mos(a.score.worst),
            reason_suffix(a)
        )
    });
    let video = step.video.as_ref().map_or("-".into(), |v| {
        format!("{:.0}/{:.0}/{:.0}{}", v.score.p50, v.score.tail, v.score.worst, reason_suffix(v))
    });
    let stall = step.stalled_share.map_or("-".into(), |r| format!("{:.1}%", r.tail * 100.0));
    let join = step.join_ms.map_or("-".into(), |r| secs(r.tail));
    let ttff = step.ttff_ms.map_or("-".into(), |r| secs(r.tail));
    let mut line = row([
        &step.step.index.to_string(),
        &step.step.participants.to_string(),
        verdict,
        &join,
        &ttff,
        &audio,
        &video,
        &stall,
        &step.tester_cell(),
    ]);
    if let Some(notes) = step.notes() {
        line.push('\n');
        line.push_str(&notes);
    }
    line
}

impl RunReport {
    fn reference(&self) -> Option<&StepReport> {
        self.steps.iter().find(|s| !matches!(s.verdict, Verdict::Pass)).or(self.steps.last())
    }

    fn capacity_sentence(&self) -> Option<String> {
        let capacity = self.capacity.as_ref()?;
        Some(match capacity {
            Capacity::SfuLimited { ok: Some(ok), failed_at } => format!(
                "Capacity: {ok} participants. Step {failed_at} breached SLOs; the tester stayed healthy."
            ),
            Capacity::SfuLimited { ok: None, failed_at } => format!(
                "Capacity: fewer than {failed_at} participants. The first step breached SLOs; the tester stayed healthy."
            ),
            Capacity::TesterLimited { ok, invalid_at } => {
                let floor = ok.map_or("no participants verified".to_string(), |n| {
                    format!("at least {n} participants")
                });
                format!(
                    "Capacity: {floor}. Step {invalid_at} was invalid because the tester, not the SFU, hit a limit. Rerun with more --workers."
                )
            }
            Capacity::NotReached { ok } => {
                format!("Capacity: not reached. Every step passed, up to {ok} participants.")
            }
        })
    }
}

fn worst_line(step: &StepReport) -> Option<String> {
    let mut worst: Vec<(f32, String)> = Vec::new();
    for p in &step.participants {
        let id = p.id.identity();
        if let Some(a) = &p.audio {
            let mos = score::mos(a.score.tail);
            worst.push((a.score.tail, format!("{id} audio MOS p95 {mos:.2}{}", reason_suffix(a))));
        }
        if let Some(v) = &p.video {
            worst.push((
                v.score.tail,
                format!("{id} video score p95 {:.0}{}", v.score.tail, reason_suffix(v)),
            ));
        }
    }
    if worst.is_empty() {
        return None;
    }
    worst.sort_by(|a, b| a.0.total_cmp(&b.0));
    let items: Vec<String> = worst.into_iter().take(WORST_LISTED).map(|(_, t)| t).collect();
    Some(format!("Worst at {}: {}", step.step.participants, items.join(", ")))
}

impl fmt::Display for RunReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = &self.header.settings;
        writeln!(
            f,
            "workers {} ({} video decoder)  publishers {} x {} {} {}  tiles {} @ {}p  hold {}s",
            s.workers,
            if s.null_video_decoder { "null" } else { "real" },
            s.publishers,
            s.video,
            s.video.codec,
            s.video.layering(),
            s.tiles,
            s.tile_height(),
            s.hold_s
        )?;
        writeln!(f)?;
        writeln!(f, "{}", step_header())?;
        for step in &self.steps {
            writeln!(f, "{}", step_line(step))?;
        }
        writeln!(f)?;

        if let Some(sentence) = self.capacity_sentence() {
            writeln!(f, "{sentence}")?;
        }
        if let Some(step) = self.reference() {
            if let Some(line) = worst_line(step) {
                writeln!(f, "{line}")?;
            }
            if step.server_poor + step.server_lost > 0 {
                writeln!(
                    f,
                    "SFU-reported quality at {}: {} POOR, {} LOST.",
                    step.step.participants, step.server_poor, step.server_lost
                )?;
            }
        }
        let decoder_mismatches: u32 = self.steps.iter().map(|s| s.decoder_mismatches).sum();
        if decoder_mismatches > 0 {
            writeln!(
                f,
                "Warning: {decoder_mismatches} video subscriptions did not run {NULL_DECODER} although it was requested."
            )?;
        }
        Ok(())
    }
}

/// Re-scores a run's JSONL file after `rejudge` adjusts the settings recorded in its header.
pub fn read_file(
    path: &Path,
    rejudge: impl FnOnce(&mut Settings) -> anyhow::Result<()>,
) -> anyhow::Result<RunReport> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut records =
        serde_json::Deserializer::from_reader(BufReader::new(file)).into_iter::<Record>();
    let mut header = match records.next() {
        Some(Ok(Record::Run(header))) => header,
        Some(Ok(_)) | None => bail!("{}: line 1 must be the run header", path.display()),
        Some(Err(e)) => bail!("{}:{}: {e}", path.display(), e.line()),
    };

    rejudge(&mut header.settings)?;
    let mut ledger = Ledger::new(header);
    for record in records {
        match record {
            Ok(record) => {
                ledger.push(record);
            }
            Err(e) if e.is_eof() => break,
            Err(e) => bail!("{}:{}: {e}", path.display(), e.line()),
        }
    }

    Ok(ledger.finish())
}
