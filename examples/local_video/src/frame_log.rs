use anyhow::{bail, Result};
use std::{
    fmt,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

/// Inclusive frame-ID bounds for per-frame CSV logging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FrameLogRange {
    start: Option<u32>,
    end: Option<u32>,
}

impl FrameLogRange {
    /// Validates optional inclusive frame-ID bounds.
    pub(crate) fn new(start: Option<u32>, end: Option<u32>) -> Result<Self> {
        if let (Some(start), Some(end)) = (start, end) {
            if start > end {
                bail!("--log-start-frame-id ({start}) must not exceed --log-end-frame-id ({end})");
            }
        }
        Ok(Self { start, end })
    }

    /// Returns whether a frame ID falls within the configured inclusive bounds.
    pub(crate) fn contains(self, frame_id: u32) -> bool {
        self.start.is_none_or(|start| frame_id >= start)
            && self.end.is_none_or(|end| frame_id <= end)
    }

    /// Returns the frame ID immediately before an explicit start bound, when representable.
    pub(crate) fn previous_to_start(self) -> Option<u32> {
        self.start.and_then(|start| start.checked_sub(1))
    }

    /// Returns whether this frame ID is the configured inclusive end bound.
    pub(crate) fn reaches_end(self, frame_id: u32) -> bool {
        self.end == Some(frame_id)
    }
}

/// Creates a buffered CSV file, including missing parent directories, and writes its header.
pub(crate) fn create_csv(path: &Path, header: &str) -> std::io::Result<BufWriter<File>> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(writer, "{header}")?;
    writer.flush()?;
    Ok(writer)
}

/// Displays an optional CSV cell without adding quoting or placeholder text.
pub(crate) struct CsvOption<T>(pub(crate) Option<T>);

impl<T: fmt::Display> fmt::Display for CsvOption<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = &self.0 {
            value.fmt(formatter)
        } else {
            Ok(())
        }
    }
}

/// Displays a timestamp delta in milliseconds when both endpoints are available and ordered.
pub(crate) struct CsvLatency(Option<u64>);

impl CsvLatency {
    /// Builds a latency cell from optional microsecond timestamps.
    pub(crate) fn between(start_timestamp_us: Option<u64>, end_timestamp_us: Option<u64>) -> Self {
        Self(match (start_timestamp_us, end_timestamp_us) {
            (Some(start), Some(end)) => end.checked_sub(start),
            _ => None,
        })
    }
}

impl fmt::Display for CsvLatency {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(latency_us) = self.0 {
            write!(formatter, "{:.3}", latency_us as f64 / 1_000.0)
        } else {
            Ok(())
        }
    }
}

/// Displays an optional floating-point CSV cell with millisecond precision.
pub(crate) struct CsvFloat(pub(crate) Option<f64>);

impl fmt::Display for CsvFloat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = self.0 {
            write!(formatter, "{value:.3}")
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_log_range_is_inclusive() {
        let range = FrameLogRange::new(Some(10), Some(20)).expect("range should be valid");
        assert!(!range.contains(9));
        assert!(range.contains(10));
        assert!(range.contains(20));
        assert!(!range.contains(21));
        assert_eq!(range.previous_to_start(), Some(9));
        assert!(range.reaches_end(20));
        assert!(!range.reaches_end(19));
    }

    #[test]
    fn frame_log_range_rejects_reversed_bounds() {
        assert!(FrameLogRange::new(Some(20), Some(10)).is_err());
    }
}
