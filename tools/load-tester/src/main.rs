use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use load_tester::report::{self, SloOverrides};

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
    /// Re-score the JSONL file of an earlier run, optionally with different SLOs.
    Report(ReportArgs),
}

#[derive(Args)]
struct ReportArgs {
    file: PathBuf,
    #[command(flatten)]
    slo: SloOverrides,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Report(args) => {
            print!("{}", report::read_file(&args.file, &args.slo)?);
            Ok(())
        }
    }
}
