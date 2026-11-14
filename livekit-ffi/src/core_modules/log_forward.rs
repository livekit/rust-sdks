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

use std::sync::atomic::{AtomicUsize, Ordering};

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
/// Also turns on the telemetry copy of the core's own warnings and errors (see
/// `telemetry_copy`), whatever `level` is: `level` filters only what is forwarded here. `Off`
/// forwards nothing to the platform's console; the telemetry copy stays on. Don't pass the
/// forwarded entries on to `telemetry_log`.
///
#[uniffi::export]
fn log_forward_bootstrap(level: LevelFilter) {
    let logger = LOGGER.get_or_init(Logger::new);
    _ = log::set_logger(logger); // Returns an error if already set (ignore)
    logger.forward.store(level as usize, Ordering::Relaxed);
    // Never stricter than `Warn`: the telemetry copy needs the core's warnings even when the
    // platform forwards only errors.
    log::set_max_level(level.max(LevelFilter::Warn));
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
    /// The platform's level (a `LevelFilter` as `usize`): what is forwarded to it. The global
    /// `log` max level may be looser, for the telemetry copy.
    forward: AtomicUsize,
}

impl Logger {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self { tx, rx: rx.into(), forward: AtomicUsize::new(LevelFilter::Trace as usize) }
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &Record) {
        // Telemetry gets its copy first, built from the borrowed record and only when a pipeline
        // is installed at the call; the console entry below is exactly what it always was. A
        // record telemetry never copies does not even look the pipeline up.
        if copied(record) {
            livekit_telemetry::global::log_with(|| telemetry_copy(record));
        }
        // `Level` and `LevelFilter` share their numbering (`Error` = 1 … `Trace` = 5).
        if record.level() as usize > self.forward.load(Ordering::Relaxed) {
            return;
        }
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

/// The core's own warnings and errors — `livekit*` targets at `Warn` or `Error` — as telemetry
/// log records, so no platform has to route them there itself. Works once the platform has
/// installed this forwarder (`log_forward_bootstrap`): Rust allows one logger per process, and
/// without it the core's `log` records go nowhere. Platforms must therefore not also feed these
/// forwarded entries into `telemetry_log`, or they would be counted twice.
///
/// Never the telemetry crate's own records (`livekit_telemetry*`): a record about a failed
/// upload must not produce another upload. The pipeline applies the same guard again.
fn telemetry_copy(record: &Record) -> Option<livekit_telemetry::LogRecord> {
    if !copied(record) {
        return None;
    }
    let target = record.target();
    Some(livekit_telemetry::LogRecord {
        severity: record.level().into(),
        source: livekit_telemetry::LogSource::Ffi,
        body: mask_credentials(&record.args().to_string()),
        logger: Some(target.to_owned()),
        // `log` knows no function name; the module path is the target, already in `logger`.
        function: None,
        file: record.file().map(str::to_owned),
        line: record.line(),
        timestamp_ns: None,
        span_id: None,
    })
}

/// Whether telemetry copies `record`: a core warning or error, never the telemetry crate's own.
fn copied(record: &Record) -> bool {
    let target = record.target();
    record.level() <= Level::Warn
        && target.starts_with("livekit")
        && !target.starts_with("livekit_telemetry")
}

/// `body` with credentials replaced: the value after `Bearer`, or after a `token` key (`token=`,
/// `access_token=`, `"token": "…"`, quoted or not, any spacing), becomes `<redacted>`, and every
/// compact JWT — three base64url segments whose header and payload decode to JSON objects,
/// whatever their formatting or the punctuation around them — becomes `<jwt>`. A participant
/// token quoted in an error must not leave the device.
pub(crate) fn mask_credentials(body: &str) -> String {
    let body = mask_credential_values(body);
    let jwt_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    let mut out = String::with_capacity(body.len());
    let mut rest = body.as_str();
    while let Some(start) = rest.find(jwt_char) {
        out.push_str(&rest[..start]);
        let run = &rest[start..];
        let run = &run[..run.find(|c| !jwt_char(c)).unwrap_or(run.len())];
        mask_run(run, &mut out);
        rest = &rest[start + run.len()..];
    }
    out.push_str(rest);
    out
}

/// Push `run` (base64url characters and dots) with every `header.payload.signature` inside it —
/// the signature may be empty, dots or a word glued on with `-`/`_` may surround it — as `<jwt>`.
fn mask_run(run: &str, out: &mut String) {
    let segments: Vec<&str> = run.split('.').collect();
    let mut i = 0;
    while i < segments.len() {
        if i > 0 {
            out.push('.');
        }
        let token = segments.get(i + 2).and_then(|_| {
            json_object(segments[i + 1]).then_some(())?;
            header_start(segments[i])
        });
        let Some(at) = token else {
            out.push_str(segments[i]);
            i += 1;
            continue;
        };
        out.push_str(&segments[i][..at]);
        out.push_str("<jwt>");
        i += 3;
    }
}

/// Where a JWT header starts in `segment`: at its start, or after a `-`/`_` joining a word to it.
fn header_start(segment: &str) -> Option<usize> {
    std::iter::once(0)
        .chain(segment.match_indices(['-', '_']).map(|(i, _)| i + 1))
        .find(|&at| json_object(&segment[at..]))
}

/// A base64url segment whose decoded bytes start, after whitespace, with `{`.
fn json_object(segment: &str) -> bool {
    let sextet = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'-' => 62,
        _ => 63, // `_`: the caller only passes base64url characters
    };
    let (mut acc, mut bits) = (0u32, 0);
    for c in segment.bytes() {
        // Keep the 12 bits that can still hold an undecoded byte.
        acc = (acc << 6 | u32::from(sextet(c))) & 0xFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            let byte = (acc >> bits) as u8;
            if !byte.is_ascii_whitespace() {
                return byte == b'{';
            }
        }
    }
    false
}

/// The value after a credential marker (matched case-insensitively) as `<redacted>`: after
/// `bearer` and whitespace, or after `bearer`/`token`, an optional closing quote, `=` or `:`, with any
/// whitespace and an optional opening quote in between. A quoted value ends at its closing quote,
/// any other at a delimiter. One pass over `body`: a marker is looked for at each byte, never by
/// searching the rest of the text for every marker again, so a body full of one marker and
/// none of the other (`token expired`, over and over) costs the same as any other.
fn mask_credential_values(body: &str) -> String {
    // Same byte offsets as `body`: ASCII lowercasing never changes a length.
    let lower = body.to_ascii_lowercase();
    let lower = lower.as_bytes();
    let mut out = String::with_capacity(body.len());
    let (mut at, mut from) = (0, 0);
    while from < lower.len() {
        let rest = &lower[from..];
        let len = if rest.starts_with(b"bearer") {
            "bearer".len()
        } else if rest.starts_with(b"token") {
            "token".len()
        } else {
            from += 1;
            continue;
        };
        // Right after an ASCII marker: a char boundary of `body` as well.
        from += len;
        let Some(value) = credential_value(&body[from..], len == "bearer".len()) else { continue };
        let (start, end) = (from + value.start, from + value.end);
        out.push_str(&body[at..start]);
        out.push_str("<redacted>");
        at = end;
        from = end;
    }
    out.push_str(&body[at..]);
    out
}

/// The non-empty value in `rest`, which follows a `bearer` (`bearer`) or `token` key, as a byte
/// range. Only a bearer value may follow its key after whitespace alone: `token expired` is prose.
fn credential_value(rest: &str, bearer: bool) -> Option<std::ops::Range<usize>> {
    let trimmed = |s: &str| s.len() - s.trim_start().len();
    let skip = if bearer && rest.starts_with(char::is_whitespace) {
        trimmed(rest)
    } else {
        let quote = usize::from(rest.starts_with(['"', '\'']));
        let after = &rest[quote..];
        let gap = trimmed(after);
        if !after[gap..].starts_with(['=', ':']) {
            return None;
        }
        quote + gap + 1 + trimmed(&after[gap + 1..])
    };
    let value = &rest[skip..];
    let (start, len) = match value.chars().next() {
        Some(q @ ('"' | '\'')) => (skip + 1, value[1..].find(q).unwrap_or(value.len() - 1)),
        _ => (
            skip,
            value
                .find(|c: char| {
                    c.is_whitespace() || matches!(c, '&' | '"' | '\'' | ',' | ';' | ')' | '<' | '>')
                })
                .unwrap_or(value.len()),
        ),
    };
    (len > 0).then_some(start..start + len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record<'a>(level: Level, target: &'a str, args: std::fmt::Arguments<'a>) -> Record<'a> {
        Record::builder()
            .level(level)
            .target(target)
            .module_path(Some(target))
            .args(args)
            .file(Some("f.rs"))
            .line(Some(7))
            .build()
    }

    /// The core's own warnings and errors reach telemetry; nothing else does, and never the
    /// telemetry crate's own (no feedback loop).
    #[test]
    fn only_the_cores_own_warnings_and_errors_are_copied() {
        let copied = |level, target| telemetry_copy(&record(level, target, format_args!("boom")));
        let copy = copied(Level::Warn, "livekit::rtc_engine").expect("a core warning");
        assert_eq!(copy.severity, livekit_telemetry::Severity::Warn);
        assert_eq!(copy.source, livekit_telemetry::LogSource::Ffi);
        assert_eq!(
            (copy.body.as_str(), copy.logger.as_deref()),
            ("boom", Some("livekit::rtc_engine"))
        );
        assert_eq!((copy.file.as_deref(), copy.line), (Some("f.rs"), Some(7)));
        assert_eq!(copy.function, None, "the module path is already the logger");
        assert!(copied(Level::Error, "livekit_datatrack").is_some());
        assert!(copied(Level::Info, "livekit::room").is_none(), "info stays on the console");
        assert!(copied(Level::Error, "livekit_telemetry::exporter").is_none(), "no feedback loop");
        assert!(copied(Level::Error, "hyper::client").is_none(), "not the core's");
        assert!(copied(Level::Error, "my_app").is_none());
    }

    /// A token quoted in a core error never reaches telemetry; the console still gets the line.
    #[test]
    fn tokens_in_copied_records_are_masked() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJib2IifQ.c2lnbmF0dXJl-_x";
        let copy = telemetry_copy(&record(
            Level::Error,
            "livekit::signal_client",
            format_args!("connect failed: wss://x/rtc?access_token={jwt}&auto=1 ({jwt})"),
        ))
        .expect("copied");
        assert_eq!(copy.body, "connect failed: wss://x/rtc?access_token=<redacted>&auto=1 (<jwt>)");
        // A valid token need not start `eyJ` — a header `{ "alg":…` and a payload with leading
        // whitespace encode differently — and a transport error may quote the bearer value itself.
        let spaced = "eyAiYWxnIjoiSFMyNTYiLCJ0eXAiOiJKV1QifQ\
                      .CiAgeyJzdWIiOiJib2IiLCJ2aWRlbyI6eyJyb29tIjoiciJ9fQ.c2lnbmF0dXJlLWJ5dGVz";
        let copy = telemetry_copy(&record(
            Level::Warn,
            "livekit_signaling",
            format_args!(
                "signal transport error: HTTP 401 for request with Authorization: Bearer opaque-\
                 credential; token {spaced}. retrying"
            ),
        ))
        .expect("copied");
        assert_eq!(
            copy.body,
            "signal transport error: HTTP 401 for request with Authorization: Bearer <redacted>; \
             token <jwt>. retrying"
        );
        assert_eq!(mask_credentials(r#"{"token":"abc.def"}"#), r#"{"token":"<redacted>"}"#);
        // A JWT inside a punctuation-delimited run, and credential values that are quoted or
        // spaced.
        for (logged, masked) in [
            (format!("token (...{jwt})"), "token (...<jwt>)".to_owned()),
            (format!("token (...{spaced})"), "token (...<jwt>)".to_owned()),
            (format!("id=lk_{jwt}.x"), "id=lk_<jwt>.x".to_owned()),
            (r#"token="opaque-secret""#.into(), r#"token="<redacted>""#.into()),
            (r#"{"token": "opaque-secret"}"#.into(), r#"{"token": "<redacted>"}"#.into()),
            (
                r#"{"token" : 'opaque secret', "k": 1}"#.into(),
                r#"{"token" : '<redacted>', "k": 1}"#.into(),
            ),
            (
                "Authorization: Bearer  opaque-secret".into(),
                "Authorization: Bearer  <redacted>".into(),
            ),
            ("ACCESS_TOKEN = opaque-secret&x".into(), "ACCESS_TOKEN = <redacted>&x".into()),
        ] {
            assert_eq!(mask_credentials(&logged), masked, "{logged}");
        }
        let unsigned = "eyAiYWxnIjoiSFMyNTYiLCJ0eXAiOiJKV1QifQ.CiAgeyJzdWIiOiJib2IifQ.";
        assert_eq!(mask_credentials(unsigned), "<jwt>", "an unsigned token is masked too");
        for kept in [
            "eyJ",
            "eyJabc.",
            "eyJabc..sig",
            "see eyJ-not-a-token.",
            "plain",
            "wss://p.livekit.cloud/rtc",
            "v1.2.3 of livekit_api.client.rs",
            "token expired, tokens: 3, token_type=x",
            "(...eyJ)",
        ] {
            assert_eq!(mask_credentials(kept), kept);
        }
    }

    /// Masking is one pass over the body: 1.2 MB of one marker and none of the other, which once
    /// searched the rest of the text again at every occurrence (seconds), costs milliseconds.
    #[test]
    fn masking_a_body_full_of_one_marker_is_linear() {
        let body = "token expired ".repeat(90_000);
        let started = std::time::Instant::now();
        assert_eq!(mask_credentials(&body), body, "prose is kept");
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(5), "{took:?} for {} bytes", body.len());
        let body = "Bearer x ".repeat(100_000);
        assert_eq!(mask_credentials(&body), "Bearer <redacted> ".repeat(100_000));
    }

    /// A platform forwarding only errors (`OSLogger(minLevel: .error)`) still gets the core's
    /// warnings into telemetry, and its console still sees only errors.
    #[test]
    fn the_platforms_level_filters_the_console_not_the_telemetry_copy() {
        log_forward_bootstrap(LevelFilter::Error);
        assert_eq!(log::max_level(), LevelFilter::Warn, "warnings still reach the logger");
        let logger = Logger::new();
        logger.forward.store(LevelFilter::Error as usize, Ordering::Relaxed);
        let warning = record(Level::Warn, "livekit::room", format_args!("careful"));
        assert!(telemetry_copy(&warning).is_some(), "telemetry copies the warning");
        logger.log(&warning);
        logger.log(&record(Level::Error, "livekit::room", format_args!("boom")));
        let mut rx = logger.rx.try_lock().expect("rx");
        assert_eq!(rx.try_recv().expect("forwarded").level, Level::Error, "the error is forwarded");
        assert!(rx.try_recv().is_err(), "the warning is not: the console is unchanged");
        drop(rx);
        log_forward_bootstrap(LevelFilter::Debug);
        assert_eq!(log::max_level(), LevelFilter::Debug, "a looser level is kept as is");
    }

    /// With telemetry off, a core warning costs the telemetry copy nothing: its message is not
    /// even formatted.
    #[test]
    fn telemetry_off_builds_no_copy() {
        let _off = crate::telemetry::PROCESS_PIPELINE.lock().unwrap_or_else(|e| e.into_inner());
        assert!(livekit_telemetry::global::stats().is_none(), "precondition: telemetry is off");
        /// Counts how often it is formatted.
        struct Counted<'a>(&'a AtomicUsize);
        impl std::fmt::Display for Counted<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fetch_add(1, Ordering::Relaxed);
                f.write_str("careful")
            }
        }
        let formatted = AtomicUsize::new(0);
        let logger = Logger::new();
        logger.forward.store(LevelFilter::Error as usize, Ordering::Relaxed);
        logger.log(&record(Level::Warn, "livekit::room", format_args!("{}", Counted(&formatted))));
        assert_eq!(formatted.load(Ordering::Relaxed), 0, "nothing was built for telemetry");
    }

    /// Run the ignored test `name` alone in a child process: `None` when it did not end within
    /// 20 s (it is killed then), else whether it passed, with its output.
    fn in_child(name: &str) -> Option<(bool, String)> {
        use std::io::Read;
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", name, "--ignored", "--nocapture", "--test-threads=1"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("child");
        let mut stdout = child.stdout.take().expect("stdout");
        let (read, output) = std::sync::mpsc::channel();
        // Ends when the child's stdout closes: when it exits, or is killed.
        let reader = std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stdout.read_to_string(&mut text);
            let _ = read.send(text.clone());
            text
        });
        let ended = output.recv_timeout(std::time::Duration::from_secs(20)).is_ok();
        if !ended {
            let _ = child.kill();
        }
        let status = child.wait().expect("child status");
        let text = reader.join().expect("reader");
        ended.then_some((status.success(), text))
    }

    /// An instrument whose destructor logs `target` warnings, then the uninstall that frees it,
    /// with this forwarder as the process logger.
    fn uninstall_an_instrument_that_logs_as_it_is_freed(target: &'static str) {
        use livekit_telemetry::global::{self, TelemetryInstrument};
        /// Logs a warning when it is freed.
        struct Logs(&'static str);
        impl Drop for Logs {
            fn drop(&mut self) {
                log::warn!(target: self.0, "instrument released, token=secret");
            }
        }
        impl TelemetryInstrument for Logs {
            fn start(&self) {}
            fn stop(&self) {}
        }
        /// Never answers.
        struct Never;
        #[async_trait::async_trait]
        impl livekit_telemetry::TelemetryTransport for Never {
            async fn send(
                &self,
                _request: livekit_telemetry::ExportRequest,
            ) -> Result<livekit_telemetry::ExportResponse, livekit_telemetry::ExportError>
            {
                std::future::pending().await
            }
        }
        log_forward_bootstrap(LevelFilter::Trace);
        let transport = std::sync::Arc::new(Never);
        let freed = std::sync::Arc::downgrade(&transport);
        let (telemetry, exporter) = livekit_telemetry::Telemetry::new(
            livekit_telemetry::TelemetryConfig::default(),
            transport,
        );
        // The installed list holds the instrument's last reference.
        assert!(global::install(telemetry, vec![std::sync::Arc::new(Logs(target))]).is_none());
        let uninstalled = global::uninstall();
        println!("CHILD uninstall returned");
        let entries: Vec<LogForwardEntry> = {
            let mut rx = LOGGER.get().expect("bootstrapped").rx.try_lock().expect("rx");
            std::iter::from_fn(|| rx.try_recv().ok()).collect()
        };
        let console = entries.iter().find(|e| e.target == target).expect("the console entry");
        assert_eq!(
            console.message, "instrument released, token=secret",
            "unchanged on the console"
        );
        drop((uninstalled, exporter));
        assert!(freed.upgrade().is_none(), "the uninstalled pipeline is gone");
        println!("CHILD ok");
    }

    #[test]
    #[ignore = "run alone in a child process by an_instrument_that_logs_as_it_is_freed_never_hangs_the_uninstall"]
    fn child_app_warning_from_an_instrument_destructor() {
        uninstall_an_instrument_that_logs_as_it_is_freed("host::instrument");
    }

    #[test]
    #[ignore = "run alone in a child process by an_instrument_that_logs_as_it_is_freed_never_hangs_the_uninstall"]
    fn child_core_warning_from_an_instrument_destructor() {
        uninstall_an_instrument_that_logs_as_it_is_freed("livekit::instrument");
    }

    /// An instrument freed by the uninstall can log, an app warning or a core one: the uninstall
    /// returns, the console gets the line, and nothing keeps the pipeline. Each case runs in its
    /// own process, so a hang (the process-wide lock held for good) is killed, not inherited.
    #[test]
    fn an_instrument_that_logs_as_it_is_freed_never_hangs_the_uninstall() {
        for case in ["app", "core"] {
            let name = format!(
                "core_modules::log_forward::tests::child_{case}_warning_from_an_instrument_destructor"
            );
            let outcome = in_child(&name);
            let Some((passed, output)) = outcome else {
                panic!("{case} warning: the uninstall hung (killed after 20 s)");
            };
            assert!(passed && output.contains("CHILD ok"), "{case} warning: {output}");
        }
    }

    /// The console entry is unchanged by the copy.
    #[test]
    fn the_console_entry_is_what_it_always_was() {
        let logger = Logger::new();
        logger.log(&record(Level::Warn, "livekit::room", format_args!("hello {}", 1)));
        let entry = logger.rx.try_lock().expect("rx").try_recv().expect("forwarded");
        assert_eq!((entry.level, entry.target.as_str()), (Level::Warn, "livekit::room"));
        assert_eq!(
            (entry.message.as_str(), entry.file.as_deref(), entry.line),
            ("hello 1", Some("f.rs"), Some(7))
        );
    }
}
