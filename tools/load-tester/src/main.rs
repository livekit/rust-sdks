mod health;
mod participant;
mod run;
mod worker;

use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use load_tester::{
    media::{Codec, VideoProfile},
    record::{now_ms, Settings, Slo, TesterLimits, WorkerInit},
    report::{self, SloOverrides},
};
use tokio::runtime::Builder;

use crate::run::RunConfig;

const WINDOW: Duration = Duration::from_secs(5);
const CHANNEL_CAPACITY: usize = 4096;

#[derive(Parser)]
#[command(
    about = "Ramp participants into a LiveKit room and score the audio and video they receive"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ramp participants in steps and report the largest step where every SLO held.
    Run(Box<RunArgs>),
    /// Re-score the JSONL file of an earlier run, optionally with different SLOs.
    Report(ReportArgs),
    #[command(hide = true)]
    Worker(WorkerArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Server URL (ws:// or wss://).
    #[arg(long, env = "LIVEKIT_URL")]
    url: String,
    #[arg(long, env = "LIVEKIT_API_KEY", hide_env_values = true)]
    api_key: String,
    #[arg(long, env = "LIVEKIT_API_SECRET", hide_env_values = true)]
    api_secret: String,
    #[arg(long, default_value = "load-test")]
    room: String,
    /// Participants 0..N publish a mic and a camera; they join first.
    #[arg(long, default_value_t = 4)]
    publishers: u32,
    /// Participants per step, strictly increasing.
    #[arg(long, value_delimiter = ',', default_value = "10,20,30,40,50,60,70,80,90,100")]
    steps: Vec<u32>,
    /// Camera width.
    #[arg(long, default_value_t = 1280, value_parser = clap::value_parser!(u32).range(1..))]
    width: u32,
    /// Camera height.
    #[arg(long, default_value_t = 720, value_parser = clap::value_parser!(u32).range(1..))]
    height: u32,
    /// Camera frame rate.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..))]
    fps: u32,
    #[arg(long, value_enum, default_value_t = Codec::Vp8)]
    codec: Codec,
    /// Each participant watches video from the first N publishers.
    #[arg(long, default_value_t = 9)]
    tiles: u32,
    /// Height the subscriber asks the SFU for. Default: the camera height.
    #[arg(long)]
    tile_height: Option<u32>,
    /// Skip video decoding in workers so one machine holds more subscribers.
    #[arg(long)]
    null_decoder: bool,
    /// Worker processes, each with its own WebRTC runtime. Default: cores / 4, at least 1.
    #[arg(long, default_value_t = default_workers(), value_parser = clap::value_parser!(u16).range(1..))]
    workers: u16,
    /// Joins per second.
    #[arg(long, default_value_t = 5.0)]
    join_rate: f32,
    /// Seconds between the last join of a step and the start of measurement.
    #[arg(long, default_value_t = 10)]
    settle_s: u32,
    /// Seconds each step is measured.
    #[arg(long, default_value_t = 30)]
    hold_s: u32,
    /// JSONL output file. Default: load-test-<unix seconds>.jsonl
    #[arg(long)]
    out: Option<PathBuf>,
    #[command(flatten)]
    slo: SloOverrides,
}

#[derive(Args)]
struct ReportArgs {
    file: PathBuf,
    #[command(flatten)]
    slo: SloOverrides,
}

#[derive(Args)]
struct WorkerArgs {
    #[arg(long)]
    init: String,
}

impl RunArgs {
    fn into_config(self) -> anyhow::Result<RunConfig> {
        let increasing = self.steps.windows(2).all(|w| w[0] < w[1]);
        anyhow::ensure!(
            self.steps.first().is_some_and(|&first| first > 0) && increasing,
            "--steps must be positive and strictly increasing"
        );
        anyhow::ensure!(
            self.publishers <= self.steps[0],
            "--publishers {} must fit in the first step ({})",
            self.publishers,
            self.steps[0]
        );
        anyhow::ensure!(self.join_rate > 0.0, "--join-rate must be positive");
        let video = VideoProfile {
            width: self.width,
            height: self.height,
            fps: self.fps,
            codec: self.codec,
        };
        Ok(RunConfig {
            api_key: self.api_key,
            api_secret: self.api_secret,
            out: self
                .out
                .unwrap_or_else(|| PathBuf::from(format!("load-test-{}.jsonl", now_ms() / 1000))),
            settings: Settings {
                url: self.url,
                room: self.room,
                publishers: self.publishers,
                steps: self.steps,
                video,
                tiles: self.tiles,
                tile_height: self.tile_height.unwrap_or(video.height),
                null_video_decoder: self.null_decoder,
                workers: self.workers,
                join_rate: self.join_rate,
                settle_s: self.settle_s,
                hold_s: self.hold_s,
                slo: self.slo.apply(Slo::default()),
                limits: TesterLimits::default(),
            },
        })
    }
}

fn default_workers() -> u16 {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cores / 4).clamp(1, u16::MAX as usize) as u16
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(args) => {
            let cfg = args.into_config()?;
            Builder::new_current_thread().enable_all().build()?.block_on(run::run(cfg))
        }
        Cmd::Report(args) => {
            print!("{}", report::read_file(&args.file, &args.slo)?);
            Ok(())
        }
        Cmd::Worker(args) => {
            let init: WorkerInit = serde_json::from_str(&args.init).context("--init")?;
            Builder::new_multi_thread().enable_all().build()?.block_on(worker::run(init))
        }
    }
}
