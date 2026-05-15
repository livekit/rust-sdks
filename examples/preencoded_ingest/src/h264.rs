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

//! Streaming Annex-B H.264 parser + access-unit assembler.
//!
//! Pushes raw bytes through [`AnnexBParser`] to extract individual NALUs,
//! then groups them into access units (frames) via [`FrameAssembler`].
//! Frames seen before the first parameter-set-bearing IDR are dropped --
//! decoders cannot start without an IDR + SPS/PPS.

const NALU_SLICE: u8 = 1;
const NALU_IDR: u8 = 5;
const NALU_SPS: u8 = 7;
const NALU_PPS: u8 = 8;

fn nalu_type_from(nalu_with_start_code: &[u8]) -> u8 {
    let offset = match start_code_len(nalu_with_start_code) {
        Some(n) => n,
        None => return 0,
    };
    if offset >= nalu_with_start_code.len() {
        return 0;
    }
    // H.264: 1-byte NAL header, type in the lower 5 bits.
    nalu_with_start_code[offset] & 0x1F
}

fn start_code_len(buf: &[u8]) -> Option<usize> {
    if buf.len() > 3 && buf[0] == 0 && buf[1] == 0 && buf[2] == 0 && buf[3] == 1 {
        Some(4)
    } else if buf.len() > 2 && buf[0] == 0 && buf[1] == 0 && buf[2] == 1 {
        Some(3)
    } else {
        None
    }
}

fn is_vcl(nalu_type: u8) -> bool {
    matches!(nalu_type, NALU_SLICE..=NALU_IDR)
}

fn is_keyframe(nalu_type: u8) -> bool {
    nalu_type == NALU_IDR
}

fn is_parameter_set(nalu_type: u8) -> bool {
    matches!(nalu_type, NALU_SPS | NALU_PPS)
}

/// One assembled H.264 access unit (i.e. a complete frame) ready to push
/// into the encoded video source.
#[derive(Debug)]
pub struct H264Frame {
    /// Concatenated Annex-B NALUs that make up this frame.
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub has_parameter_sets: bool,
    pub nalu_count: usize,
}

/// Splits a streaming Annex-B byte feed into discrete NALUs.
///
/// Internally buffers a partial NALU between `push()` calls so callers can
/// feed arbitrary chunk sizes (e.g. raw socket reads).
pub struct AnnexBParser {
    buffer: Vec<u8>,
}

impl AnnexBParser {
    pub fn new() -> Self {
        Self { buffer: Vec::with_capacity(256 * 1024) }
    }

    /// Append `data` to the internal buffer and return any newly completed
    /// NALUs as `(nalu_type, nalu_with_start_code)` pairs.
    pub fn push(&mut self, data: &[u8]) -> Vec<(u8, Vec<u8>)> {
        self.buffer.extend_from_slice(data);
        let mut nalus = Vec::new();
        let mut start: Option<usize> = None;

        let mut i = 0;
        while i < self.buffer.len() {
            let is_4byte = i + 3 < self.buffer.len()
                && self.buffer[i] == 0
                && self.buffer[i + 1] == 0
                && self.buffer[i + 2] == 0
                && self.buffer[i + 3] == 1;
            let is_3byte = !is_4byte
                && i + 2 < self.buffer.len()
                && self.buffer[i] == 0
                && self.buffer[i + 1] == 0
                && self.buffer[i + 2] == 1;

            if is_4byte || is_3byte {
                if let Some(prev_start) = start {
                    let nalu_data = self.buffer[prev_start..i].to_vec();
                    if !nalu_data.is_empty() {
                        let nalu_type = nalu_type_from(&nalu_data);
                        nalus.push((nalu_type, nalu_data));
                    }
                }
                start = Some(i);
                i += if is_4byte { 4 } else { 3 };
            } else {
                i += 1;
            }
        }

        // Keep the last (still-incomplete) NALU as the new buffer head so
        // the next push can stitch the rest of it together.
        if let Some(prev_start) = start {
            self.buffer = self.buffer[prev_start..].to_vec();
        }

        nalus
    }
}

/// Groups individual NALUs into complete H.264 access units.
///
/// Caches the most-recent SPS/PPS so it can prepend them to keyframes that
/// were emitted without inline parameter sets (matches the behaviour
/// of WebRTC's H.264 RTP packetizer).
pub struct FrameAssembler {
    pending_nalus: Vec<(u8, Vec<u8>)>,
    seen_keyframe: bool,
    cached_sps: Option<Vec<u8>>,
    cached_pps: Option<Vec<u8>>,
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self {
            pending_nalus: Vec::new(),
            seen_keyframe: false,
            cached_sps: None,
            cached_pps: None,
        }
    }

    /// Feed parsed NALUs and return any completed frames.
    ///
    /// The returned `dropped` counter tallies frames that were skipped
    /// because we have not yet seen a keyframe + parameter sets.
    pub fn push_nalus(&mut self, nalus: Vec<(u8, Vec<u8>)>) -> (Vec<H264Frame>, u64) {
        let mut frames = Vec::new();
        let mut dropped: u64 = 0;

        for (nalu_type, nalu_data) in nalus {
            self.cache_parameter_set(nalu_type, &nalu_data);

            let nalu_is_vcl = is_vcl(nalu_type);

            if nalu_is_vcl && !self.pending_nalus.is_empty() {
                let has_prev_vcl = self.pending_nalus.iter().any(|(t, _)| is_vcl(*t));
                // H.264 single-slice assumption: any new VCL NALU after a
                // VCL NALU starts a new access unit.
                if has_prev_vcl {
                    let mut frame = self.flush_frame_before_next_au();
                    if frame.is_keyframe && !frame.has_parameter_sets {
                        self.prepend_cached_parameter_sets(&mut frame);
                    }
                    if frame.is_keyframe && frame.has_parameter_sets {
                        self.seen_keyframe = true;
                    }
                    if self.seen_keyframe {
                        frames.push(frame);
                    } else {
                        dropped += 1;
                    }
                }
            }

            self.pending_nalus.push((nalu_type, nalu_data));
        }

        (frames, dropped)
    }

    /// Flush whatever remains in the buffer as one final frame -- only
    /// returns `Some` once the first keyframe has been observed.
    pub fn flush_remaining(&mut self) -> Option<H264Frame> {
        if self.pending_nalus.is_empty() {
            return None;
        }
        let nalus = std::mem::take(&mut self.pending_nalus);
        if self.seen_keyframe {
            Some(Self::build_frame(&nalus))
        } else {
            None
        }
    }

    fn cache_parameter_set(&mut self, nalu_type: u8, nalu_data: &[u8]) {
        match nalu_type {
            NALU_SPS => self.cached_sps = Some(nalu_data.to_vec()),
            NALU_PPS => self.cached_pps = Some(nalu_data.to_vec()),
            _ => {}
        }
    }

    fn prepend_cached_parameter_sets(&self, frame: &mut H264Frame) {
        let mut prefix = Vec::new();
        let mut extra = 0usize;
        if let Some(sps) = &self.cached_sps {
            prefix.extend_from_slice(sps);
            extra += 1;
        }
        if let Some(pps) = &self.cached_pps {
            prefix.extend_from_slice(pps);
            extra += 1;
        }
        if !prefix.is_empty() {
            prefix.extend_from_slice(&frame.data);
            frame.data = prefix;
            frame.has_parameter_sets = true;
            frame.nalu_count += extra;
        }
    }

    fn flush_frame_before_next_au(&mut self) -> H264Frame {
        let all = std::mem::take(&mut self.pending_nalus);
        // Trailing non-VCL NALUs belong to the *next* access unit, not this
        // one -- carry them over so they prefix the next frame.
        let last_vcl = all.iter().rposition(|(t, _)| is_vcl(*t));
        let split_at = match last_vcl {
            Some(idx) => idx + 1,
            None => all.len(),
        };
        let (frame_nalus, carry_over) = all.split_at(split_at);
        self.pending_nalus = carry_over.to_vec();
        Self::build_frame(frame_nalus)
    }

    fn build_frame(nalus: &[(u8, Vec<u8>)]) -> H264Frame {
        let mut data = Vec::new();
        let mut keyframe = false;
        let mut has_param_sets = false;
        for (nalu_type, nalu_data) in nalus {
            data.extend_from_slice(nalu_data);
            if is_keyframe(*nalu_type) {
                keyframe = true;
            }
            if is_parameter_set(*nalu_type) {
                has_param_sets = true;
            }
        }
        H264Frame {
            data,
            is_keyframe: keyframe,
            has_parameter_sets: has_param_sets,
            nalu_count: nalus.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nalu(start_code: &[u8], header: u8, body: &[u8]) -> Vec<u8> {
        let mut v = start_code.to_vec();
        v.push(header);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn parser_extracts_nalus_across_chunks() {
        let mut parser = AnnexBParser::new();
        let chunk1 = nalu(&[0, 0, 0, 1], 0x67, &[0xAA]);
        let mut chunk2 = nalu(&[0, 0, 0, 1], 0x68, &[0xBB]);
        chunk2.extend_from_slice(&nalu(&[0, 0, 1], 0x65, &[0xCC, 0xDD]));

        let nalus_a = parser.push(&chunk1);
        // First push sees only the SPS NALU header but cannot yet flush it
        // (might be followed by more bytes). It still flushes what it has.
        assert!(nalus_a.is_empty());

        let nalus_b = parser.push(&chunk2);
        // After the second chunk we should see the SPS, then the PPS.
        // The IDR NALU is still pending until the next start code arrives.
        assert_eq!(nalus_b.len(), 2);
        assert_eq!(nalus_b[0].0, NALU_SPS);
        assert_eq!(nalus_b[1].0, NALU_PPS);

        // Pushing another start code should flush the trailing IDR.
        let trailing = parser.push(&[0, 0, 0, 1]);
        assert_eq!(trailing.len(), 1);
        assert_eq!(trailing[0].0, NALU_IDR);
    }

    #[test]
    fn assembler_drops_until_first_keyframe_with_param_sets() {
        let mut assembler = FrameAssembler::new();
        // Send one orphan delta slice -- should be dropped.
        let (frames, dropped) = assembler.push_nalus(vec![
            (NALU_SLICE, nalu(&[0, 0, 0, 1], 0x41, &[0xFF])),
            (NALU_SLICE, nalu(&[0, 0, 0, 1], 0x41, &[0xFE])),
        ]);
        assert!(frames.is_empty());
        assert_eq!(dropped, 1);

        // Now feed an SPS, PPS, IDR sequence followed by another delta to
        // trigger the AU boundary.
        let (frames, _) = assembler.push_nalus(vec![
            (NALU_SPS, nalu(&[0, 0, 0, 1], 0x67, &[0x11])),
            (NALU_PPS, nalu(&[0, 0, 0, 1], 0x68, &[0x22])),
            (NALU_IDR, nalu(&[0, 0, 0, 1], 0x65, &[0x33])),
            (NALU_SLICE, nalu(&[0, 0, 0, 1], 0x41, &[0x44])),
        ]);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].is_keyframe);
        assert!(frames[0].has_parameter_sets);
    }
}
