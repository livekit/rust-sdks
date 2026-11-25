//! Camera timestamp validation shared by the video and vector publishers.
use std::time::Duration;

const MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US: u64 = 5_000_000;

/// Accepts a camera-provided Unix timestamp close to the time its buffer was read.
pub(crate) fn validate_backend_capture_timestamp_us(
    capture_timestamp: Duration,
    read_wall_time_us: u64,
) -> Result<u64, &'static str> {
    let capture_timestamp_us =
        u64::try_from(capture_timestamp.as_micros()).map_err(|_| "overflows u64")?;
    if capture_timestamp_us == 0 {
        return Err("is zero");
    }
    if capture_timestamp_us > read_wall_time_us {
        return Err("is in the future");
    }
    if read_wall_time_us - capture_timestamp_us > MAX_BACKEND_CAPTURE_TIMESTAMP_AGE_US {
        return Err("is too old");
    }
    Ok(capture_timestamp_us)
}
