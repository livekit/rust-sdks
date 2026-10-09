// Copyright 2025 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use env_logger;
use log::{self, Log};
use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, oneshot};

use crate::{proto, FFI_SERVER};

pub const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
pub const BATCH_SIZE: usize = 32;

/// Logger that forwards logs to the FfiClient when capture_logs is enabled.
/// Otherwise falls back to env_logger.
///
/// The global `log` logger is installed once for the process, but `setup()`
/// and `dispose()` refresh the env_logger filter from the current `RUST_LOG`
/// value and restart the capture forward task so repeated initialize/shutdown
/// cycles (for example gtest repeats) observe the latest environment.
pub struct FfiLogger {
    async_runtime: tokio::runtime::Handle,
    log_tx: Mutex<Option<mpsc::UnboundedSender<LogMsg>>>,
    capture_logs: AtomicBool,
    env_logger: RwLock<env_logger::Logger>,
}

enum LogMsg {
    Log(proto::LogRecord),
    Flush(oneshot::Sender<()>),
}

impl FfiLogger {
    pub fn new(async_runtime: tokio::runtime::Handle) -> Self {
        FfiLogger {
            async_runtime,
            log_tx: Mutex::new(None),
            capture_logs: AtomicBool::new(false),
            // Avoid reading RUST_LOG until the client calls setup().
            env_logger: RwLock::new(silent_env_logger()),
        }
    }

    /// Prepare logging for a new initialize cycle.
    pub fn setup(&self, capture_logs: bool) {
        *self.env_logger.write() = env_logger_from_default_env();
        self.capture_logs.store(capture_logs, Ordering::Release);
        self.start_log_forward_task();
    }

    /// Tear down logging for the current initialize cycle.
    pub fn dispose(&self) {
        self.flush_captured_logs();
        self.capture_logs.store(false, Ordering::Release);
        self.stop_log_forward_task();
        self.env_logger.read().flush();
        *self.env_logger.write() = silent_env_logger();
    }

    pub fn capture_logs(&self) -> bool {
        self.capture_logs.load(Ordering::Acquire)
    }
}

impl Log for FfiLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        if self.capture_logs() {
            return true;
        }

        self.env_logger.read().enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if !self.capture_logs() {
            self.env_logger.read().log(record);
            return;
        }

        if let Some(log_tx) = self.log_tx.lock().as_ref() {
            let _ = log_tx.send(LogMsg::Log(record.into()));
        }
    }

    fn flush(&self) {
        if !self.capture_logs() {
            self.env_logger.read().flush();
            return;
        }

        self.flush_captured_logs();
    }
}

impl FfiLogger {
    fn flush_captured_logs(&self) {
        if !self.capture_logs() {
            return;
        }

        let log_tx_guard = self.log_tx.lock();
        let Some(log_tx) = log_tx_guard.as_ref() else {
            return;
        };

        let (tx, rx) = oneshot::channel();
        if log_tx.send(LogMsg::Flush(tx)).is_err() {
            return;
        }
        let _ = self.async_runtime.block_on(rx);
    }

    fn start_log_forward_task(&self) {
        let mut log_tx = self.log_tx.lock();
        if log_tx.is_some() {
            return;
        }

        let (sender, log_rx) = mpsc::unbounded_channel();
        *log_tx = Some(sender);
        self.async_runtime.spawn(log_forward_task(log_rx));
    }

    fn stop_log_forward_task(&self) {
        *self.log_tx.lock() = None;
    }

    #[cfg(test)]
    fn env_logger_enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.env_logger.read().enabled(metadata)
    }
}

fn env_logger_from_default_env() -> env_logger::Logger {
    env_logger::Builder::from_default_env().build()
}

fn silent_env_logger() -> env_logger::Logger {
    env_logger::Builder::new().filter_level(log::LevelFilter::Off).build()
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

#[cfg(test)]
mod tests {
    use super::*;
    use log::Metadata;

    fn metadata(level: log::Level) -> Metadata<'static> {
        Metadata::builder().level(level).target("livekit_ffi").build()
    }

    #[test]
    fn setup_rebuilds_env_logger_from_rust_log() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let logger = FfiLogger::new(runtime.handle().clone());
        let debug_meta = metadata(log::Level::Debug);

        std::env::set_var("RUST_LOG", "livekit_ffi=error");
        logger.setup(false);
        assert!(!logger.env_logger_enabled(&debug_meta));

        logger.dispose();

        std::env::set_var("RUST_LOG", "livekit_ffi=debug");
        logger.setup(false);
        assert!(logger.env_logger_enabled(&debug_meta));

        logger.dispose();
    }

    #[test]
    fn dispose_stops_capture_forward_task() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let logger = FfiLogger::new(runtime.handle().clone());

        logger.setup(true);
        assert!(logger.log_tx.lock().is_some());

        logger.dispose();
        assert!(logger.log_tx.lock().is_none());
        assert!(!logger.capture_logs());
    }
}
