use std::{
    collections::BTreeSet,
    fs::File,
    io::{BufWriter, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::Context;
use livekit_token::{AccessToken, VideoGrants};
use load_tester::{
    record::{
        now_ms, JoinCommand, ParticipantId, Record, RunHeader, Settings, StepRecord,
        WorkerExitRecord, WorkerInit,
    },
    report::{self, Ledger, RunReport, StepReport, Verdict},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    signal::unix::{signal, Signal, SignalKind},
    sync::mpsc,
    task::JoinSet,
    time::Instant,
};

use crate::CHANNEL_CAPACITY;

const JOIN_BACKSTOP: Duration = Duration::from_secs(60);
const PIPELINE_SLACK: Duration = Duration::from_secs(2);
const STOP_DEADLINE: Duration = Duration::from_secs(20);

pub struct RunConfig {
    pub api_key: String,
    pub api_secret: String,
    pub settings: Settings,
    pub out: PathBuf,
}

impl RunConfig {
    fn token(&self, id: ParticipantId) -> anyhow::Result<String> {
        let grants = VideoGrants {
            room_join: true,
            room: self.settings.room.clone(),
            can_publish: Some(self.settings.is_publisher(id)),
            can_subscribe: Some(true),
            ..Default::default()
        };
        Ok(AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&id.identity())
            .with_grants(grants)
            .to_jwt()?)
    }
}

pub async fn run(cfg: RunConfig) -> anyhow::Result<()> {
    let header = RunHeader {
        version: env!("CARGO_PKG_VERSION").into(),
        started: now_ms(),
        cores: std::thread::available_parallelism()?.get() as u32,
        settings: cfg.settings.clone(),
    };
    let mut recorder = Recorder::create(&cfg.out, header)?;
    println!("writing {}", cfg.out.display());
    let mut workers = Workers::spawn(&cfg.settings)?;
    println!("{}", report::step_header());

    let settle = secs(cfg.settings.settle_s);
    let hold = secs(cfg.settings.hold_s);
    let mut joined = 0;
    for (i, &target) in cfg.settings.steps.iter().enumerate() {
        let mut measure_start = None;
        if workers.join(&cfg, joined..target, &mut recorder).await? && !workers.aborting {
            workers.forward(Instant::now() + settle, exited, &mut recorder).await?;
            if !workers.aborting {
                measure_start = Some(now_ms());
                workers.forward(Instant::now() + hold, exited, &mut recorder).await?;
            }
        }
        let measure_end = now_ms();
        workers.forward(Instant::now() + PIPELINE_SLACK, exited, &mut recorder).await?;
        let step = StepRecord {
            index: i as u32 + 1,
            participants: target,
            first_new: joined,
            measure_start: measure_start.unwrap_or(measure_end),
            measure_end,
            aborted: measure_start.is_none() || workers.aborting,
        };
        joined = target;
        let report = recorder.step(step)?;
        println!("{}", report::step_line(report));
        if !matches!(report.verdict, Verdict::Pass) {
            break;
        }
    }

    workers.stop(&mut recorder).await?;
    println!();
    print!("{}", recorder.finish()?);
    Ok(())
}

fn secs(s: u32) -> Duration {
    Duration::from_secs(s.into())
}

fn exited(record: &Record) -> bool {
    matches!(record, Record::WorkerExit(_))
}

struct Recorder {
    file: BufWriter<File>,
    ledger: Ledger,
}

impl Recorder {
    fn create(path: &Path, header: RunHeader) -> anyhow::Result<Self> {
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let ledger = Ledger::new(header.clone());
        let mut recorder = Self { file: BufWriter::new(file), ledger };
        recorder.write(&Record::Run(header))?;
        Ok(recorder)
    }

    fn push(&mut self, record: Record) -> anyhow::Result<()> {
        self.write(&record)?;
        self.ledger.push(record);
        Ok(())
    }

    fn step(&mut self, step: StepRecord) -> anyhow::Result<&StepReport> {
        let record = Record::Step(step);
        self.write(&record)?;
        self.file.flush()?;
        self.ledger.push(record).context("a step record yields a report")
    }

    fn write(&mut self, record: &Record) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.file, record)?;
        self.file.write_all(b"\n")?;
        Ok(())
    }

    fn finish(mut self) -> anyhow::Result<RunReport> {
        self.file.flush()?;
        Ok(self.ledger.finish())
    }
}

struct Workers {
    stdins: Vec<ChildStdin>,
    readers: JoinSet<()>,
    records: mpsc::Receiver<Record>,
    ctrl_c: Signal,
    aborting: bool,
}

impl Workers {
    fn spawn(settings: &Settings) -> anyhow::Result<Self> {
        let exe = std::env::current_exe()?;
        let (tx, records) = mpsc::channel(CHANNEL_CAPACITY);
        let mut readers = JoinSet::new();
        let mut stdins = Vec::new();
        for worker in 0..settings.workers {
            let init = serde_json::to_string(&WorkerInit { worker, settings: settings.clone() })?;
            let mut child = Command::new(&exe)
                .args(["worker", "--init", &init])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .process_group(0)
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("spawning worker {worker}"))?;
            stdins.push(child.stdin.take().context("worker stdin")?);
            let stdout = child.stdout.take().context("worker stdout")?;
            readers.spawn(read_records(worker, child, stdout, tx.clone()));
        }
        let ctrl_c = signal(SignalKind::interrupt())?;
        Ok(Self { stdins, readers, records, ctrl_c, aborting: false })
    }

    /// Sends the new ids at the join rate and returns whether every one of them reported a join.
    async fn join(
        &mut self,
        cfg: &RunConfig,
        ids: Range<u32>,
        recorder: &mut Recorder,
    ) -> anyhow::Result<bool> {
        let mut pending: BTreeSet<ParticipantId> = ids.clone().map(ParticipantId).collect();
        let mut done = |record: &Record| {
            exited(record)
                || matches!(record, Record::Join(j) if pending.remove(&j.id) && pending.is_empty())
        };
        let period = Duration::from_secs_f32(1.0 / cfg.settings.join_rate);
        let mut next = Instant::now();
        for id in ids.map(ParticipantId) {
            self.forward(next, &mut done, recorder).await?;
            if self.aborting {
                break;
            }
            let mut line = serde_json::to_vec(&JoinCommand { id, token: cfg.token(id)? })?;
            line.push(b'\n');
            let worker = id.0 as usize % self.stdins.len();
            if let Err(e) = self.stdins[worker].write_all(&line).await {
                eprintln!("worker {worker}: {e}");
            }
            next += period;
        }
        if !self.aborting {
            self.forward(Instant::now() + JOIN_BACKSTOP, &mut done, recorder).await?;
        }
        Ok(pending.is_empty())
    }

    /// Forwards worker records to the recorder until one satisfies `done`, the deadline passes,
    /// every reader has finished, or Ctrl-C arrives.
    async fn forward(
        &mut self,
        deadline: Instant,
        mut done: impl FnMut(&Record) -> bool,
        recorder: &mut Recorder,
    ) -> anyhow::Result<()> {
        loop {
            let record = tokio::select! {
                _ = tokio::time::sleep_until(deadline) => return Ok(()),
                _ = self.ctrl_c.recv() => {
                    if self.aborting {
                        std::process::exit(130);
                    }
                    self.aborting = true;
                    return Ok(());
                }
                record = self.records.recv() => record,
            };
            let Some(record) = record else { return Ok(()) };
            // a worker that exits before it was told to ends the run
            if exited(&record) && !self.stdins.is_empty() {
                self.aborting = true;
            }
            let finished = done(&record);
            recorder.push(record)?;
            if finished {
                return Ok(());
            }
        }
    }

    async fn stop(mut self, recorder: &mut Recorder) -> anyhow::Result<()> {
        self.stdins.clear();
        self.forward(Instant::now() + STOP_DEADLINE, |_| false, recorder).await?;
        self.readers.shutdown().await;
        Ok(())
    }
}

async fn read_records(
    worker: u16,
    mut child: Child,
    stdout: ChildStdout,
    tx: mpsc::Sender<Record>,
) {
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        match serde_json::from_str::<Record>(&line) {
            Ok(record) => {
                if tx.send(record).await.is_err() {
                    return;
                }
            }
            Err(e) => eprintln!("worker {worker}: skipped a line ({e}): {line}"),
        }
    }
    let status = match child.wait().await {
        Ok(status) => status.to_string(),
        Err(e) => e.to_string(),
    };
    let exit = WorkerExitRecord { at: now_ms(), worker, status };
    let _ = tx.send(Record::WorkerExit(exit)).await;
}
