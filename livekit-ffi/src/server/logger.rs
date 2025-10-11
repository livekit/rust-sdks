use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use env_logger;
use log::{self, Log};
use tokio::sync::{mpsc, oneshot};

use crate::{proto, FFI_SERVER};

pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
pub const BATCH_SIZE: usize = 32;

/// Logger that forward logs to the FfiClient when capture_logs is enabled
/// Otherwise fallback to the env_logger
pub struct FfiLogger {
    async_runtime: tokio::runtime::Handle,
    log_tx: mpsc::UnboundedSender<LogMsg>,
    capture_logs: AtomicBool,
    env_logger: env_logger::Logger,
}

enum LogMsg {
    Log(proto::LogRecord),
    Flush(oneshot::Sender<()>),
}

impl FfiLogger {
    pub fn new(async_runtime: tokio::runtime::Handle) -> Self {
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        async_runtime.spawn(log_forward_task(log_rx));

        let env_logger = env_logger::Builder::from_default_env().build();
        FfiLogger {
            async_runtime,
            log_tx,
            capture_logs: AtomicBool::new(false), // Always false by default to ensure the server
            // is always initialized when using capture_logs
            env_logger,
        }
    }
}

impl FfiLogger {
    pub fn capture_logs(&self) -> bool {
        self.capture_logs.load(Ordering::Acquire)
    }

    pub fn set_capture_logs(&self, capture: bool) {
        self.capture_logs.store(capture, Ordering::Release);
    }
}

impl Log for FfiLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        if !self.capture_logs() {
            return self.env_logger.enabled(metadata);
        }

        true // The ffi client decides what to log (FfiLogger is just forwarding)
    }

    fn log(&self, record: &log::Record) {
        if !self.capture_logs() {
            return self.env_logger.log(record);
        }

        self.log_tx.send(LogMsg::Log(record.into())).unwrap();
    }

    fn flush(&self) {
        if !self.capture_logs() {
            return self.env_logger.flush();
        }

        let (tx, rx) = oneshot::channel();
        self.log_tx.send(LogMsg::Flush(tx)).unwrap();
        let _ = self.async_runtime.block_on(rx); // should we block?
    }
}

async fn log_forward_task(mut rx: mpsc::UnboundedReceiver<LogMsg>) {
    async fn flush(batch: &mut Vec<proto::LogRecord>) {
        if batch.is_empty() {
            return;
        }
        // It is safe to use FFI_SERVER here, if we receive logs when capture_logs is enabled,
        // it means the server has already been initialized

        let _ = FFI_SERVER.send_event(
            proto::LogBatch {
            records: batch.clone(), // Avoid clone here?
        }
            .into(),
        );
        batch.clear();
    }

    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut interval = tokio::time::interval(FLUSH_INTERVAL);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                if msg.is_none() {
                    break;
                }

                match msg.unwrap() {
                    LogMsg::Log(record) => {
                        batch.push(record);
                    }
                    LogMsg::Flush(tx) => {
                        flush(&mut batch).await;
                        let _ = tx.send(());
                    }
                }
            },
            _ = interval.tick() => {
                flush(&mut batch).await;
            }
        }

        flush(&mut batch).await;
    }

    println!("log forwarding task stopped"); // Shouldn't happen (logger is leaked)
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
