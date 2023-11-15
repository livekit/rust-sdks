use crate::proto;
use crate::server::FfiServer;
use log::{self, Log};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

enum LogMsg {
    Log(proto::LogRecord),
    Flush(oneshot::Sender<()>),
}

pub struct FfiLogger {
    server: &'static FfiServer,
    log_tx: mpsc::UnboundedSender<LogMsg>,
}

impl FfiLogger {
    pub fn new(server: &'static FfiServer, max_batch_size: u32) -> Self {
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        server
            .async_runtime
            .spawn(log_task(server, max_batch_size, log_rx));
        FfiLogger { server, log_tx }
    }
}

async fn log_task(
    server: &'static FfiServer,
    max_batch_size: u32,
    mut rx: mpsc::UnboundedReceiver<LogMsg>,
) {
    async fn flush(server: &'static FfiServer, batch: &mut Vec<proto::LogRecord>) {
        let _ = server
            .send_event(proto::ffi_event::Message::Logs(proto::LogBatch {
                records: batch.clone(), // Avoid clone here?
            }))
            .await;
        batch.clear();
    }

    let mut batch = Vec::with_capacity(max_batch_size as usize);
    let mut interval = tokio::time::interval(FLUSH_INTERVAL);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                if let Some(msg) = msg {

                    match msg {
                        LogMsg::Log(record) => {
                            batch.push(record);
                        }
                        LogMsg::Flush(tx) => {
                            flush(server, &mut batch).await;
                            let _ = tx.send(());
                        }
                    }

                } else {
                    flush(server, &mut batch).await;
                    break; // FfiLogger dropped
                }
            },
            _ = interval.tick() => {
                flush(server, &mut batch).await;
            }
        }
    }
}

impl Log for FfiLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        true // The ffi client decides what to log (FfiLogger is just forwarding)
    }

    fn log(&self, record: &log::Record) {
        self.log_tx.send(LogMsg::Log(record.into())).unwrap();
    }

    fn flush(&self) {
        let (tx, mut rx) = oneshot::channel();
        self.log_tx.send(LogMsg::Flush(tx)).unwrap();
        let _ = self.server.async_runtime.block_on(rx); // should we block?
    }
}

impl From<&log::Record<'_>> for proto::LogRecord {
    fn from(record: &log::Record) -> Self {
        proto::LogRecord {
            level: proto::LogLevel::from(record.level()).into(),
            target: record.target().to_string(),
            module_path: record.module_path().map(|s| s.to_string()),
            file: record.file().map(|s| s.to_string()),
            line: record.line(),
            message: record.args().to_string(), // Display trait
        }
    }
}

impl From<log::Level> for proto::LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => Self::LogError,
            log::Level::Warn => Self::LogWarn,
            log::Level::Info => Self::LogInfo,
            log::Level::Debug => Self::LogDebug,
            log::Level::Trace => Self::LogTrace,
        }
    }
}
