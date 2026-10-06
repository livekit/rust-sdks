mod health;
mod participant;
mod run;
mod worker;

use std::{path::PathBuf, time::Duration};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use load_tester::{
    config,
    record::{now_ms, WorkerInit},
    report,
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
    /// Re-score the JSONL file of an earlier run; --config and --set re-judge it with other
    /// [slo], [limits] and [scoring] values, and the file's other sections are ignored.
    Report(ReportArgs),
    #[command(hide = true)]
    Worker(WorkerArgs),
}

#[derive(Args)]
struct ConfigArgs {
    /// TOML config file; load-test.example.toml lists every key with its default.
    #[arg(long = "config", value_name = "FILE")]
    path: Option<PathBuf>,
    /// Set one config key to a TOML value, applied after the file. Repeatable:
    /// --set steps=[10,20,40] --set video.codec=av1 --set slo.audio_mos_floor=3.8
    #[arg(long, value_name = "KEY=VALUE")]
    set: Vec<String>,
}

impl ConfigArgs {
    fn file(&self) -> anyhow::Result<Option<toml::Table>> {
        self.path.as_deref().map(config::read).transpose()
    }
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    config: ConfigArgs,
    /// Server URL (ws:// or wss://); beats the config file and --set.
    #[arg(long, env = "LIVEKIT_URL")]
    url: Option<String>,
    #[arg(long, env = "LIVEKIT_API_KEY", hide_env_values = true)]
    api_key: String,
    #[arg(long, env = "LIVEKIT_API_SECRET", hide_env_values = true)]
    api_secret: String,
    /// JSONL output file. Default: load-test-<unix seconds>.jsonl
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct ReportArgs {
    file: PathBuf,
    #[command(flatten)]
    config: ConfigArgs,
}

#[derive(Args)]
struct WorkerArgs {
    #[arg(long)]
    init: String,
}

impl RunArgs {
    fn into_config(self) -> anyhow::Result<RunConfig> {
        let settings = config::run_settings(self.config.file()?, &self.config.set, self.url)?;
        Ok(RunConfig {
            api_key: self.api_key,
            api_secret: self.api_secret,
            out: self
                .out
                .unwrap_or_else(|| PathBuf::from(format!("load-test-{}.jsonl", now_ms() / 1000))),
            settings,
        })
    }
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Run(args) => {
            let cfg = args.into_config()?;
            Builder::new_current_thread().enable_all().build()?.block_on(run::run(cfg))
        }
        Cmd::Report(args) => {
            let file = args.config.file()?;
            let report =
                report::read_file(&args.file, |s| config::rejudge(s, file, &args.config.set))?;
            let path = args.config.path.iter().map(|p| p.display().to_string());
            let overrides: Vec<String> = path.chain(args.config.set.iter().cloned()).collect();
            if !overrides.is_empty() {
                println!("judged with {} over the run's recorded criteria\n", overrides.join(", "));
            }
            print!("{report}");
            Ok(())
        }
        Cmd::Worker(args) => {
            let init: WorkerInit = serde_json::from_str(&args.init).context("--init")?;
            Builder::new_multi_thread().enable_all().build()?.block_on(worker::run(init))
        }
    }
}
