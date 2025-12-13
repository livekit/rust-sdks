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

use log::{Level, LevelFilter, Log, Record};
use once_cell::sync::OnceCell;
use tokio::sync::{mpsc, Mutex};

/// Global logger instance.
static LOGGER: OnceCell<Logger> = OnceCell::new();

/// Bootstraps log forwarding.
///
/// Generally, you will invoke this once early in program execution. However,
/// subsequent invocations are allowed to change the log level.
///
#[uniffi::export]
fn log_forward_bootstrap(level: LevelFilter) {
    let logger = LOGGER.get_or_init(|| Logger::new());
    _ = log::set_logger(logger); // Returns an error if already set (ignore)
    log::set_max_level(level);
}

/// Asynchronously receives a forwarded log entry.
///
/// Invoke repeatedly to receive log entries as they are produced
/// until `None` is returned, indicating forwarding has ended. Clients will
/// likely want to bridge this to the languages's equivalent of an asynchronous stream.
///
#[uniffi::export]
async fn log_forward_receive() -> Option<LogForwardEntry> {
    let logger = LOGGER.get().expect("Log forwarding not bootstrapped");
    logger.rx.try_lock().ok()?.recv().await
}

#[uniffi::remote(Enum)]
#[uniffi(name = "LogForwardFilter")]
pub enum LevelFilter {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[uniffi::remote(Enum)]
#[uniffi(name = "LogForwardLevel")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(uniffi::Record)]
pub struct LogForwardEntry {
    level: Level,
    target: String,
    file: Option<String>,
    line: Option<u32>,
    message: String,
}
// TODO: can we expose static strings?

struct Logger {
    tx: mpsc::UnboundedSender<LogForwardEntry>,
    rx: Mutex<mpsc::UnboundedReceiver<LogForwardEntry>>,
}

impl Logger {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { tx, rx: rx.into() }
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        let record = LogForwardEntry {
            level: record.metadata().level(),
            target: record.target().to_string(),
            file: record.file().map(|s| s.to_string()),
            line: record.line(),
            message: record.args().to_string(),
        };
        // TODO: expose module path and key-value pairs
        self.tx.send(record).unwrap();
    }
    fn flush(&self) {}
}
