//! Shared 6-channel codec for the `--attach-user-data` demo.
//!
//! Six channel values are encoded as little-endian `int16` fixed-point, 2 bytes
//! per channel = 12 bytes total, and shipped in the `user_data` frame-metadata
//! trailer field. The full `int16` range maps to `±VALUE_RANGE`, giving
//! ~1/32767 of the range in resolution — well within the ~232-byte trailer
//! budget.
//!
//! Both the `publisher` and `subscriber` binaries include this file via
//! `mod user_data;` so they agree on the wire format.

/// Number of channels carried in the user_data payload.
pub const NUM_CHANNELS: usize = 6;

/// Encoded payload size in bytes (2 bytes per channel).
pub const ENCODED_LEN: usize = NUM_CHANNELS * 2;

/// Value that maps to `i16::MAX`. Channel values are normalized to
/// `±VALUE_RANGE` before quantization.
pub const VALUE_RANGE: f32 = 1.0;

/// Value units per `int16` step.
fn scale() -> f32 {
    VALUE_RANGE / i16::MAX as f32
}

/// Clamp a channel value to the encodable `±VALUE_RANGE` range.
pub fn clamp_value(value: f32) -> f32 {
    value.clamp(-VALUE_RANGE, VALUE_RANGE)
}

/// Encode 6 channel values into 12 little-endian `int16` bytes.
pub fn encode(values: &[f32; NUM_CHANNELS]) -> Vec<u8> {
    let s = scale();
    let mut buf = Vec::with_capacity(ENCODED_LEN);
    for &v in values {
        let q = (v / s).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        buf.extend_from_slice(&q.to_le_bytes());
    }
    buf
}

/// Decode 6 channel values from the `user_data` payload. Returns `None` if the
/// buffer is too short to hold all six values.
pub fn decode(buf: &[u8]) -> Option<[f32; NUM_CHANNELS]> {
    if buf.len() < ENCODED_LEN {
        return None;
    }
    let s = scale();
    let mut out = [0.0f32; NUM_CHANNELS];
    for (i, chunk) in buf.chunks_exact(2).take(NUM_CHANNELS).enumerate() {
        out[i] = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 * s;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_within_quantization_error() {
        let values = [0.0, 0.5, -0.75, 1.0, -0.1, 0.9];
        let decoded = decode(&encode(&values)).unwrap();
        for (v, d) in values.iter().zip(decoded.iter()) {
            assert!((v - d).abs() <= scale(), "got {d}, expected ~{v}");
        }
    }

    #[test]
    fn clamp_keeps_within_range() {
        assert_eq!(clamp_value(100.0), VALUE_RANGE);
        assert_eq!(clamp_value(-100.0), -VALUE_RANGE);
        assert_eq!(clamp_value(0.5), 0.5);
    }

    #[test]
    fn decode_rejects_short_buffer() {
        assert!(decode(&[0u8; ENCODED_LEN - 1]).is_none());
    }
}
