use crate::proto;
use crate::server::FfiServer;
use crate::FFI_SERVER;
use env_logger;
use log::{self, Log};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
pub const BATCH_SIZE: usize = 16;

/// Logger that forward logs to the FfiClient when capture_logs is enabled
/// Otherwise fallback to the env_logger
pub struct FfiLogger {
    server: &'static FfiServer,
    log_tx: mpsc::UnboundedSender<LogMsg>,
    capture_logs: AtomicBool,
    env_logger: env_logger::Logger,
}

enum LogMsg {
    Log(proto::LogRecord),
    Flush(oneshot::Sender<()>),
}

impl FfiLogger {
    pub fn new(capture_logs: bool) -> Self {
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        FFI_SERVER
            .async_runtime
            .spawn(log_forward_task(&FFI_SERVER, log_rx));

        let env_logger = env_logger::Builder::from_default_env().build();
        FfiLogger {
            server: &FFI_SERVER,
            log_tx,
            capture_logs: AtomicBool::new(capture_logs),
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

        let (tx, mut rx) = oneshot::channel();
        self.log_tx.send(LogMsg::Flush(tx)).unwrap();
        let _ = self.server.async_runtime.block_on(rx); // should we block?
    }
}

async fn log_forward_task(server: &'static FfiServer, mut rx: mpsc::UnboundedReceiver<LogMsg>) {
    async fn flush(server: &'static FfiServer, batch: &mut Vec<proto::LogRecord>) {
        if batch.is_empty() {
            return;
        }
        let _ = server
            .send_event(proto::ffi_event::Message::Logs(proto::LogBatch {
                records: batch.clone(), // Avoid clone here?
            }))
            .await;
        batch.clear();
    }

    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut interval = tokio::time::interval(FLUSH_INTERVAL);

    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                match msg {
                    LogMsg::Log(record) => {
                        batch.push(record);
                    }
                    LogMsg::Flush(tx) => {
                        flush(server, &mut batch).await;
                        let _ = tx.send(());
                    }
                }
           },
            _ = interval.tick() => {
                flush(server, &mut batch).await;
            }
        }

        flush(server, &mut batch).await;
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
