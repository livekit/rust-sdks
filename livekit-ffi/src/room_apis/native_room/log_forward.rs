// Copyright 2026 LiveKit, Inc.
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

//! Log forwarding for the typed surface.
//!
//! The exports have the names and shapes of the `core-modules` log exports,
//! so a host SDK reads logs from both surfaces with the same code. This
//! module does not install a logger. It observes the records of the logger
//! that the FFI server owns, so the protobuf log capture is unchanged.

use std::{
    collections::VecDeque,
    sync::atomic::{AtomicUsize, Ordering},
};

use log::{Level, LevelFilter};
use parking_lot::Mutex;
use tokio::sync::Notify;

use super::events::wait_for;

/// Largest number of entries that wait for the consumer.
// ponytail: fixed bound, the oldest entry goes first. Make it an argument if a host needs more.
const MAX_ENTRIES: usize = 1024;

static FILTER: AtomicUsize = AtomicUsize::new(LevelFilter::Off as usize);
static ENTRIES: Mutex<VecDeque<LogForwardEntry>> = Mutex::new(VecDeque::new());
static CHANGED: Notify = Notify::const_new();

/// Returns `true` when a record of `level` is forwarded.
pub(crate) fn enabled(level: Level) -> bool {
    level as usize <= FILTER.load(Ordering::Relaxed)
}

/// Keeps a copy of `record` for the consumer. Never waits and never logs.
pub(crate) fn observe(record: &log::Record) {
    if !enabled(record.level()) {
        return;
    }
    let entry = LogForwardEntry {
        level: record.level(),
        target: record.target().to_owned(),
        file: record.file().map(str::to_owned),
        line: record.line(),
        message: record.args().to_string(),
    };
    let mut entries = ENTRIES.lock();
    if entries.len() >= MAX_ENTRIES {
        entries.pop_front();
    }
    entries.push_back(entry);
    drop(entries);
    CHANGED.notify_waiters();
}

/// Starts log forwarding, or changes its level.
///
/// At most 1024 entries wait for the consumer. When the consumer is slower
/// than the log, the oldest entries are dropped.
#[uniffi::export]
pub(crate) fn log_forward_bootstrap(level: LevelFilter) {
    // The server installs the process logger on first use.
    lazy_static::initialize(&crate::FFI_SERVER);
    FILTER.store(level as usize, Ordering::Relaxed);
}

/// Waits for the next forwarded log entry.
///
/// Call it repeatedly. The logger lives as long as the process, so this
/// surface never returns `None`.
#[uniffi::export]
pub(crate) async fn log_forward_receive() -> Option<LogForwardEntry> {
    Some(wait_for(&CHANGED, || ENTRIES.lock().pop_front()).await)
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

/// One forwarded log record.
#[derive(uniffi::Record)]
pub struct LogForwardEntry {
    pub(crate) level: Level,
    target: String,
    file: Option<String>,
    line: Option<u32>,
    // The name matches the `core-modules` record. A record field named
    // `message` compiles in Kotlin; only an error variant field does not.
    pub(crate) message: String,
}
