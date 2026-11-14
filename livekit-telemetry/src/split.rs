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

//! Lossless wire splitting of the one-resource, one-scope OTLP shape we write.
//! Protobuf field order is arbitrary. Three passes count records, measure both halves, then
//! copy raw fields into gzip writers with corrected envelope lengths. Unknown fields survive.

use std::io::{Read, Write};

use flate2::{bufread::GzDecoder, write::GzEncoder, Compression};

/// Whole decoded input materialization, not a heap, output-buffer or RSS bound.
pub(crate) const DECODE_BUDGET: usize = 8 * 1024 * 1024;
const SCRATCH: usize = 32 * 1024;

#[cfg(test)]
thread_local! {
    // Observe actual materialized input bytes and the reader used, not heap/RSS allocations.
    static MATERIALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static STREAMING_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
pub(crate) type Halves = [(Vec<u8>, u64); 2];

/// Only a genuinely single-record supported batch may take the existing oversized-drop path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Failure {
    #[error("single-record batch")]
    SingleRecord,
    #[error("unsupported OTLP envelope")]
    Unsupported,
    #[error("malformed protobuf or gzip")]
    Malformed,
    #[error("consent withdrawn during split")]
    Cancelled,
}

impl From<std::io::Error> for Failure {
    fn from(_: std::io::Error) -> Self {
        Self::Malformed
    }
}

/// Split a complete gzip member, retaining no decoded whole input larger than the budget.
/// Outputs are private temporaries until the caller transactionally replaces the original.
pub(crate) fn recover(body: &[u8], cancelled: impl Fn() -> bool) -> Result<Halves, Failure> {
    #[cfg(test)]
    {
        MATERIALIZED.set(0);
        STREAMING_PASSES.set(0);
    }
    let mut decoder = GzDecoder::new(body);
    let mut decoded = Vec::new();
    let mut scratch = [0; SCRATCH];
    while decoded.len() < DECODE_BUDGET {
        if cancelled() {
            return Err(Failure::Cancelled);
        }
        let take = scratch.len().min(DECODE_BUDGET - decoded.len());
        let n = decoder.read(&mut scratch[..take])?;
        if n == 0 {
            break;
        }
        decoded.extend_from_slice(&scratch[..n]);
    }
    #[cfg(test)]
    MATERIALIZED.set(decoded.len());
    let mut probe = [0];
    let large = decoder.read(&mut probe)? != 0;
    if !large {
        if !decoder.get_ref().is_empty() {
            return Err(Failure::Malformed);
        }
        split(Input::Decoded(&decoded), &cancelled)
    } else {
        // Drop the prefix before opening the streaming passes. The extra probe byte is on-stack.
        drop((decoded, decoder));
        split(Input::Gzip(body), &cancelled)
    }
}

/// Re-readable input: one bounded decoded slice, or a fresh gzip decoder per pass.
#[derive(Clone, Copy)]
enum Input<'a> {
    Decoded(&'a [u8]),
    Gzip(&'a [u8]),
}

/// Original envelope sizes, including the raw key and length prefix widths.
#[derive(Default, Clone, Copy)]
struct Container {
    length: u64,
    key_bytes: u64,
    length_bytes: u64,
}

/// Constant-size framing summary; no per-record offsets or metadata payloads are retained.
#[derive(Default)]
struct Shape {
    containers: [Container; 2],
    seen: [u64; 2],
    records: u64,
    record_bytes: [u64; 2],
}

impl Shape {
    /// Recalculate resource/scope lengths after each half omits the other records.
    fn lengths(&self) -> [[u64; 2]; 2] {
        let [resource, scope] = self.containers;
        let scope_base = scope.length - self.record_bytes.iter().sum::<u64>();
        let scopes = self.record_bytes.map(|n| scope_base + n);
        let resource_base = resource.length - scope.key_bytes - scope.length_bytes - scope.length;
        let resources = scopes.map(|n| resource_base + scope.key_bytes + varint(n).len as u64 + n);
        [resources, scopes]
    }
}

/// Write complete compressed children only after both framing-measurement passes succeed.
fn split(input: Input<'_>, cancelled: &dyn Fn() -> bool) -> Result<Halves, Failure> {
    let mut encoders = std::array::from_fn(|_| GzEncoder::new(Vec::new(), Compression::fast()));
    let counts = split_into(input, &mut encoders, cancelled)?;
    let [first, second] = encoders;
    Ok([(first.finish()?, counts[0]), (second.finish()?, counts[1])])
}

/// Split a newly encoded batch without constructing a second whole protobuf object.
/// The caller already owns the encoded input; only child output buffers are allocated here.
pub(crate) fn encoded(input: &[u8]) -> Result<Halves, Failure> {
    let mut outputs = [Vec::new(), Vec::new()];
    let counts = split_into(Input::Decoded(input), &mut outputs, &|| false)?;
    let [first, second] = outputs;
    Ok([(first, counts[0]), (second, counts[1])])
}

/// Count, measure, then copy; each pass validates the complete input framing.
fn split_into<W: Write>(
    input: Input<'_>,
    outputs: &mut [W; 2],
    cancelled: &dyn Fn() -> bool,
) -> Result<[u64; 2], Failure> {
    let shape = pass::<std::io::Sink>(input, u64::MAX, None, [[0; 2]; 2], cancelled)?;
    if shape.records == 1 {
        return Err(Failure::SingleRecord);
    }
    if shape.records == 0 {
        return Err(Failure::Unsupported);
    }
    let midpoint = shape.records / 2;
    let measured = pass::<std::io::Sink>(input, midpoint, None, [[0; 2]; 2], cancelled)?;
    pass(input, midpoint, Some(outputs), measured.lengths(), cancelled)?;
    if cancelled() {
        return Err(Failure::Cancelled);
    }
    Ok([midpoint, shape.records - midpoint])
}

/// Open one independent read pass, validating the gzip footer before returning.
fn pass<W: Write>(
    input: Input<'_>,
    midpoint: u64,
    output: Option<&mut [W; 2]>,
    lengths: [[u64; 2]; 2],
    cancelled: &dyn Fn() -> bool,
) -> Result<Shape, Failure> {
    match input {
        Input::Decoded(mut bytes) => walk(&mut bytes, midpoint, output, lengths, cancelled),
        Input::Gzip(bytes) => {
            #[cfg(test)]
            STREAMING_PASSES.set(STREAMING_PASSES.get() + 1);
            let mut decoder = GzDecoder::new(bytes);
            let shape = walk(&mut decoder, midpoint, output, lengths, cancelled)?;
            // Reaching EOF validates the footer/CRC. Extra members or trailing bytes are not our shape.
            if !decoder.get_ref().is_empty() {
                return Err(Failure::Malformed);
            }
            Ok(shape)
        }
    }
}

/// Walk the supported envelope using one fixed scratch buffer.
fn walk<W: Write>(
    reader: &mut dyn Read,
    midpoint: u64,
    output: Option<&mut [W; 2]>,
    lengths: [[u64; 2]; 2],
    cancelled: &dyn Fn() -> bool,
) -> Result<Shape, Failure> {
    let mut walker = Walker {
        reader,
        position: 0,
        scratch: [0; SCRATCH],
        output,
        lengths,
        cancelled,
        midpoint,
        shape: Shape::default(),
    };
    walker.message(None, 0, 3)?;
    if walker.shape.seen != [1, 1] {
        return Err(Failure::Unsupported);
    }
    Ok(walker.shape)
}

/// A varint and its exact original bytes (including legal noncanonical encodings).
struct Raw {
    bytes: [u8; 10],
    len: usize,
    value: u64,
}
/// Encode a replacement container length without allocating.
fn varint(mut value: u64) -> Raw {
    let mut raw = Raw { bytes: [0; 10], len: 0, value };
    loop {
        let more = value >= 128;
        raw.bytes[raw.len] = (value as u8 & 127) | if more { 128 } else { 0 };
        raw.len += 1;
        value >>= 7;
        if !more {
            return raw;
        }
    }
}

/// Copy bytes only to the selected child outputs (mask 3 means both).
fn emit<W: Write>(output: &mut Option<&mut [W; 2]>, mask: u8, bytes: &[u8]) -> Result<(), Failure> {
    if let Some(output) = output.as_deref_mut() {
        for (i, writer) in output.iter_mut().enumerate() {
            if mask & (1 << i) != 0 {
                writer.write_all(bytes)?;
            }
        }
    }
    Ok(())
}

/// A pass owns its reader and output borrows; it stores no callbacks or ownership back-edges.
struct Walker<'a, 'b, W: Write> {
    reader: &'a mut dyn Read,
    position: u64,
    scratch: [u8; SCRATCH],
    output: Option<&'b mut [W; 2]>,
    lengths: [[u64; 2]; 2],
    cancelled: &'a dyn Fn() -> bool,
    midpoint: u64,
    shape: Shape,
}

impl<W: Write> Walker<'_, '_, W> {
    /// Read a complete varint within its enclosing message, distinguishing top-level EOF.
    fn raw(&mut self, end: Option<u64>) -> Result<Option<Raw>, Failure> {
        if (self.cancelled)() {
            return Err(Failure::Cancelled);
        }
        if end == Some(self.position) {
            return Ok(None);
        }
        let mut raw = Raw { bytes: [0; 10], len: 0, value: 0 };
        for i in 0..10 {
            if end.is_some_and(|end| self.position >= end) {
                return Err(Failure::Malformed);
            }
            let n = self.reader.read(&mut raw.bytes[i..i + 1])?;
            if n == 0 {
                return if i == 0 && end.is_none() { Ok(None) } else { Err(Failure::Malformed) };
            }
            self.position = self.position.checked_add(1).ok_or(Failure::Malformed)?;
            raw.len += 1;
            let byte = raw.bytes[i];
            if i == 9 && byte > 1 {
                return Err(Failure::Malformed);
            }
            raw.value |= u64::from(byte & 127) << (i * 7);
            if byte < 128 {
                return Ok(Some(raw));
            }
        }
        Err(Failure::Malformed)
    }

    /// Validate a payload length before consuming any of it.
    fn bound(&self, length: u64, end: Option<u64>) -> Result<u64, Failure> {
        let at = self.position.checked_add(length).ok_or(Failure::Malformed)?;
        if end.is_some_and(|end| at > end) {
            return Err(Failure::Malformed);
        }
        Ok(at)
    }

    /// Copy a field payload in fixed chunks, observing consent between chunks.
    fn copy(&mut self, mut length: u64, end: Option<u64>, mask: u8) -> Result<(), Failure> {
        self.bound(length, end)?;
        while length != 0 {
            if (self.cancelled)() {
                return Err(Failure::Cancelled);
            }
            let n = length.min(SCRATCH as u64) as usize;
            self.reader.read_exact(&mut self.scratch[..n])?;
            self.position += n as u64;
            emit(&mut self.output, mask, &self.scratch[..n])?;
            length -= n as u64;
        }
        Ok(())
    }

    /// Traverse request/resource/scope/record levels; only envelope lengths are rewritten.
    fn message(&mut self, end: Option<u64>, level: usize, mask: u8) -> Result<(), Failure> {
        loop {
            let start = self.position;
            let Some(key) = self.raw(end)? else {
                return Ok(());
            };
            let number = key.value >> 3;
            if number == 0 || number > 0x1fff_ffff {
                return Err(Failure::Malformed);
            }
            let wire = key.value & 7;
            if wire != 2 || level > 2 || number != if level == 0 { 1 } else { 2 } {
                emit(&mut self.output, mask, &key.bytes[..key.len])?;
                self.field(number, wire, end, mask, 0)?;
                continue;
            }
            let length = self.raw(end)?.ok_or(Failure::Malformed)?;
            let child_end = self.bound(length.value, end)?;
            if level < 2 {
                self.shape.seen[level] += 1;
                if self.shape.seen[level] > 1 {
                    return Err(Failure::Unsupported);
                }
                self.shape.containers[level] = Container {
                    length: length.value,
                    key_bytes: key.len as u64,
                    length_bytes: length.len as u64,
                };
                for i in 0..2 {
                    let encoded = varint(self.lengths[level][i]);
                    emit(&mut self.output, 1 << i, &key.bytes[..key.len])?;
                    emit(&mut self.output, 1 << i, &encoded.bytes[..encoded.len])?;
                }
                self.message(Some(child_end), level + 1, mask)?;
            } else {
                let half = usize::from(self.shape.records >= self.midpoint);
                self.shape.records += 1;
                let target = 1 << half;
                emit(&mut self.output, target, &key.bytes[..key.len])?;
                emit(&mut self.output, target, &length.bytes[..length.len])?;
                // Validate record wire framing without materializing the record's fields.
                self.message(Some(child_end), 3, target)?;
                self.shape.record_bytes[half] += self.position - start;
            }
        }
    }

    /// Preserve an opaque field, validating its wire framing and matching legacy group ends.
    fn field(
        &mut self,
        number: u64,
        wire: u64,
        end: Option<u64>,
        mask: u8,
        depth: u32,
    ) -> Result<(), Failure> {
        match wire {
            0 => {
                let raw = self.raw(end)?.ok_or(Failure::Malformed)?;
                emit(&mut self.output, mask, &raw.bytes[..raw.len])
            }
            1 => self.copy(8, end, mask),
            2 => {
                let raw = self.raw(end)?.ok_or(Failure::Malformed)?;
                emit(&mut self.output, mask, &raw.bytes[..raw.len])?;
                self.copy(raw.value, end, mask)
            }
            3 => {
                // Unsupported deeply nested legacy groups are retained, never called a record.
                if depth >= 64 {
                    return Err(Failure::Unsupported);
                }
                loop {
                    let key = self.raw(end)?.ok_or(Failure::Malformed)?;
                    let child = key.value >> 3;
                    if child == 0 || child > 0x1fff_ffff {
                        return Err(Failure::Malformed);
                    }
                    emit(&mut self.output, mask, &key.bytes[..key.len])?;
                    if key.value & 7 == 4 {
                        return if child == number { Ok(()) } else { Err(Failure::Malformed) };
                    }
                    self.field(child, key.value & 7, end, mask, depth + 1)?;
                }
            }
            5 => self.copy(4, end, mask),
            _ => Err(Failure::Malformed),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use super::*;

    pub(crate) fn field(number: u64, payload: &[u8]) -> Vec<u8> {
        let key = varint((number << 3) | 2);
        let length = varint(payload.len() as u64);
        [&key.bytes[..key.len], &length.bytes[..length.len], payload].concat()
    }

    /// Real raw unknown fields: all six legal wire types, including a legacy group.
    fn unknowns() -> Vec<u8> {
        // Fields 90 varint (noncanonical value 1), 91 fixed64, 92 bytes, 93 group, 94 fixed32.
        let mut bytes = vec![0xd0, 0x05, 0x81, 0x00, 0xd9, 0x05];
        bytes.extend_from_slice(&[7; 8]);
        bytes.extend(field(92, b"unknown-envelope"));
        bytes.extend_from_slice(&[0xeb, 0x05, 0x08, 0x01, 0xec, 0x05, 0xf5, 0x05, 1, 2, 3, 4]);
        bytes
    }

    pub(crate) fn record(bytes: usize, traces: bool, seed: u8, random: bool) -> Vec<u8> {
        let mut state = u64::from(seed) + 1;
        let text: Vec<u8> = (0..bytes)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                if random {
                    b' ' + (state % 95) as u8
                } else {
                    b'a' + seed
                }
            })
            .collect();
        // LogRecord.severity_text / Span.name, both valid UTF-8. Include record-level unknowns.
        [field(if traces { 5 } else { 3 }, &text), unknowns()].concat()
    }

    pub(crate) fn fixture(records: &[Vec<u8>]) -> Vec<u8> {
        // Deliberately noncanonical field order, with schema/resource/scope metadata after data.
        let mut scope = unknowns();
        for record in records {
            scope.extend(field(2, record));
        }
        scope.extend(field(3, b"scope-schema"));
        scope.extend(field(1, &field(1, b"sdk-scope")));
        let resource = [
            field(3, b"resource-schema"),
            field(2, &scope),
            field(
                1,
                &field(
                    1,
                    &[field(1, b"resource.key"), field(2, &field(1, b"resource-value"))].concat(),
                ),
            ),
            unknowns(),
        ]
        .concat();
        [unknowns(), field(1, &resource), field(70, b"top-level-extension")].concat()
    }

    pub(crate) fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut zip = GzEncoder::new(Vec::new(), Compression::fast());
        zip.write_all(bytes).expect("fixture gzip");
        zip.finish().expect("fixture finish")
    }

    pub(crate) fn decoded(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        GzDecoder::new(bytes).read_to_end(&mut out).expect("complete gzip");
        out
    }

    #[test]
    fn metadata_unknown_fields_order_and_records_survive_raw_split_for_logs_and_spans() {
        for traces in [false, true] {
            let records: Vec<_> = (0..5).map(|n| record(100, traces, n, false)).collect();
            let original = fixture(&records);
            use crate::proto::opentelemetry::proto::collector::{
                logs::v1::ExportLogsServiceRequest, trace::v1::ExportTraceServiceRequest,
            };
            use prost::Message;
            if traces {
                ExportTraceServiceRequest::decode(original.as_slice())
                    .expect("valid spans fixture");
            } else {
                ExportLogsServiceRequest::decode(original.as_slice()).expect("valid logs fixture");
            }
            let halves = recover(&gzip(&original), || false).expect("split");
            assert_eq!([halves[0].1, halves[1].1], [2, 3]);
            assert_eq!(decoded(&halves[0].0), fixture(&records[..2]));
            assert_eq!(decoded(&halves[1].0), fixture(&records[2..]));
        }
    }

    #[test]
    fn interleaved_metadata_and_large_unknown_envelope_fields_survive() {
        let records: Vec<_> = (0..4).map(|n| record(30, false, n, false)).collect();
        let interleaved = |from: usize, to: usize| {
            let mut scope = Vec::new();
            for (i, record) in records.iter().enumerate() {
                if (from..to).contains(&i) {
                    scope.extend(field(2, record));
                }
                scope.extend(field(99, &[i as u8]));
            }
            scope.extend(field(3, b"scope-schema-after-records"));
            field(1, &field(2, &scope))
        };
        let halves = recover(&gzip(&interleaved(0, 4)), || false).expect("interleaved");
        assert_eq!(decoded(&halves[0].0), interleaved(0, 2));
        assert_eq!(decoded(&halves[1].0), interleaved(2, 4));

        let unknown = field(71, &vec![b'z'; DECODE_BUDGET + 1]);
        let original = [unknown.clone(), fixture(&records)].concat();
        let halves = recover(&gzip(&original), || false).expect("large opaque metadata");
        assert_eq!(MATERIALIZED.get(), DECODE_BUDGET);
        assert_eq!(STREAMING_PASSES.get(), 3);
        assert_eq!(decoded(&halves[0].0), [unknown.clone(), fixture(&records[..2])].concat());
        assert_eq!(decoded(&halves[1].0), [unknown, fixture(&records[2..])].concat());
        // Each decoded child exceeds the input threshold: it is deliberately not an output cap.
    }

    #[test]
    fn decoded_input_budget_minus_exact_plus_one_and_large_recovery() {
        for target in [DECODE_BUDGET - 1, DECODE_BUDGET, DECODE_BUDGET + 1] {
            let mut payload = target - 1000;
            let (bytes, records) = loop {
                let records = vec![record(payload, false, 0, false), record(1, false, 1, false)];
                let bytes = fixture(&records);
                if bytes.len() == target {
                    break (bytes, records);
                }
                payload = (payload as isize + target as isize - bytes.len() as isize) as usize;
            };
            let result = recover(&gzip(&bytes), || false).expect("boundary recovery");
            assert_eq!(MATERIALIZED.get(), target.min(DECODE_BUDGET));
            assert_eq!(STREAMING_PASSES.get(), if target > DECODE_BUDGET { 3 } else { 0 });
            let plain = encoded(&bytes).expect("new-batch split without whole object decode");
            assert_eq!(plain[0].0, fixture(&records[..1]));
            assert_eq!(plain[1].0, fixture(&records[1..]));
            assert_eq!(decoded(&result[0].0), fixture(&records[..1]));
            assert_eq!(decoded(&result[1].0), fixture(&records[1..]));
        }
        for traces in [false, true] {
            for random in [false, true] {
                let records: Vec<_> =
                    (0..4).map(|n| record(DECODE_BUDGET / 3, traces, n, random)).collect();
                let bytes = fixture(&records);
                assert!(bytes.len() > DECODE_BUDGET);
                let zipped = gzip(&bytes);
                let halves = recover(&zipped, || false).expect("streaming supported envelope");
                assert_eq!(decoded(&halves[0].0), fixture(&records[..2]));
                assert_eq!(decoded(&halves[1].0), fixture(&records[2..]));
                eprintln!("streaming traces={traces} random={random} decoded={} compressed={} output={}+{}", bytes.len(), zipped.len(), halves[0].0.len(), halves[1].0.len());
            }
        }
    }

    #[test]
    fn unsupported_malformed_single_and_cancelled_are_distinct_and_never_partial_success() {
        let records = vec![record(30, false, 0, false), record(30, false, 1, false)];
        let valid = fixture(&records);
        assert_eq!(recover(&gzip(&fixture(&records[..1])), || false), Err(Failure::SingleRecord));
        assert_eq!(recover(&gzip(&fixture(&[])), || false), Err(Failure::Unsupported));
        let multi_resources = [
            field(1, &field(2, &field(2, &records[0]))),
            field(1, &field(2, &field(2, &records[1]))),
        ]
        .concat();
        let multi_scopes = field(
            1,
            &[field(2, &field(2, &records[0])), field(2, &field(2, &records[1]))].concat(),
        );
        for bytes in [multi_resources, multi_scopes] {
            assert_eq!(recover(&gzip(&bytes), || false), Err(Failure::Unsupported));
        }
        for bytes in [
            vec![0x0a, 0xff, 0xff],
            vec![0x0a, 0x02, 0x12, 0xff],
            vec![0x00],
            vec![0x0f],
            vec![0x80],
            vec![0x0b, 0x14],
        ] {
            assert_eq!(recover(&gzip(&bytes), || false), Err(Failure::Malformed));
        }
        let mut corrupt = gzip(&valid);
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert_eq!(recover(&corrupt, || false), Err(Failure::Malformed));
        assert_eq!(recover(&gzip(&valid)[..10], || false), Err(Failure::Malformed));
        let mut trailing = gzip(&valid);
        trailing.extend(gzip(&valid));
        assert_eq!(recover(&trailing, || false), Err(Failure::Malformed));
        let calls = Cell::new(0);
        let zipped = gzip(&valid);
        let count = || {
            calls.set(calls.get() + 1);
            false
        };
        pass::<std::io::Sink>(Input::Gzip(&zipped), u64::MAX, None, [[0; 2]; 2], &count)
            .expect("count");
        pass::<std::io::Sink>(Input::Gzip(&zipped), 1, None, [[0; 2]; 2], &count).expect("measure");
        let during_output = calls.get() + 10;
        calls.set(0);
        // The measured first two passes finish; the third has begun writing metadata.
        let result = split(Input::Gzip(&zipped), &|| {
            calls.set(calls.get() + 1);
            calls.get() > during_output
        });
        assert_eq!(result, Err(Failure::Cancelled));
    }
}
