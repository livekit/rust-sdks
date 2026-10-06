use std::sync::Arc;

use anyhow::Context;
use load_tester::{
    media::{Ladder, Publishers, Pump},
    record::{JoinCommand, Record, WorkerInit},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::{mpsc, watch},
    task::JoinSet,
};

use crate::{
    health,
    participant::{Ctx, Participant},
    CHANNEL_CAPACITY,
};

pub async fn run(init: WorkerInit) -> anyhow::Result<()> {
    // any panic ends the worker, and the parent scores the step invalid on its exit
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{info}");
        std::process::exit(101);
    }));
    if init.settings.null_video_decoder {
        livekit::webrtc::enable_null_video_decoder()?;
    }
    let (out, records) = mpsc::channel(CHANNEL_CAPACITY);
    let writer = tokio::spawn(write_stdout(records));
    let (stop, stopped) = watch::channel(false);
    let publishers = Arc::new(Publishers::default());
    let pump = Arc::new(Pump::default());
    let pump_thread = std::thread::Builder::new().name("pump".into()).spawn({
        let (publishers, pump, video) = (publishers.clone(), pump.clone(), init.settings.video);
        move || pump.run(&publishers, video)
    })?;
    let health = tokio::spawn(health::run(init.worker, out.clone(), stopped.clone(), pump.clone()));
    let ctx = Arc::new(Ctx {
        ladder: Ladder::of(&init.settings.video),
        settings: init.settings,
        publishers,
        out,
        stop: stopped,
    });

    let mut participants = JoinSet::new();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let JoinCommand { id, token } = serde_json::from_str(&line).context("join command")?;
        participants.spawn(Participant::run(id, token, ctx.clone()));
    }

    let _ = stop.send(true);
    while participants.join_next().await.is_some() {}
    health.await?;
    pump.stop();
    let _ = pump_thread.join();
    drop(ctx);
    writer.await??;
    Ok(())
}

async fn write_stdout(mut records: mpsc::Receiver<Record>) -> anyhow::Result<()> {
    let mut stdout = BufWriter::new(tokio::io::stdout());
    while let Some(record) = records.recv().await {
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        stdout.write_all(&line).await?;
        if records.is_empty() {
            stdout.flush().await?;
        }
    }
    stdout.flush().await?;
    Ok(())
}
