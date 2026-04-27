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

//! Stream demuxers that split a raw TCP bytestream into discrete encoded
//! video frames.
//!
//! * H.264 / H.265: Annex-B bytestream, split on access-unit delimiters.
//! * VP8 / VP9 / AV1: IVF container (gstreamer's `ivfmux` or `avmux_ivf`),
//!   optionally prefixed with a 32-byte DKIF file header.

use libwebrtc::video_source::VideoCodec;

use super::keyframe;

/// Upper bound on per-frame size we accept from the IVF reader before we
/// conclude we are byte-misaligned.
pub(super) const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Wire-format selector. Hides whether the underlying wire is Annex-B or
/// IVF.
pub(super) enum Demuxer {
    AnnexB(AuSplitter),
    Ivf(IvfReader),
}

impl Demuxer {
    pub(super) fn new(codec: VideoCodec) -> Self {
        match codec {
            VideoCodec::H264 | VideoCodec::H265 => Demuxer::AnnexB(AuSplitter::new(codec)),
            VideoCodec::Vp8 | VideoCodec::Vp9 | VideoCodec::Av1 => {
                Demuxer::Ivf(IvfReader::new(codec))
            }
        }
    }

    /// Feeds a raw byte chunk from the socket. Completed frames are
    /// appended to `out`.
    pub(super) fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        match self {
            Demuxer::AnnexB(s) => s.feed(chunk, out),
            Demuxer::Ivf(r) => r.feed(chunk, out),
        }
    }

    /// True if the demuxer has detected a byte misalignment it cannot
    /// recover from without a fresh TCP connection.
    pub(super) fn desynced(&self) -> bool {
        match self {
            Demuxer::AnnexB(_) => false,
            Demuxer::Ivf(r) => r.desynced,
        }
    }
}

/// Reads IVF-framed video off the wire. Format per libvpx:
///
/// * File header (32 bytes, optional): `"DKIF"`, u16 version, u16
///   header_len, 4-byte FOURCC, u16 width, u16 height, u32 tb_num,
///   u32 tb_den, u32 frame_count, u32 unused.
/// * Frame header (12 bytes each): u32 frame_size, u64 pts.
/// * Frame payload: `frame_size` bytes. All integers little-endian.
///
/// The file header is *optional* here: gstreamer's `avmux_ivf` on a
/// non-seekable `tcpserversink` emits only per-frame records (libavformat
/// writes `DKIF` at `write_header` time, but the ffmpeg AVIO wrapper in
/// gst-libav swallows it when the output is non-seekable). `ivfmux` (the
/// native gst-plugins-bad element) does emit `DKIF` and we parse it when
/// present. gstreamer's one-buffer-per-packet semantics keep new
/// `tcpserversink` clients on an IVF record boundary.
pub(super) struct IvfReader {
    codec: VideoCodec,
    buf: Vec<u8>,
    header_phase_done: bool,
    pub(super) desynced: bool,
}

impl IvfReader {
    fn new(codec: VideoCodec) -> Self {
        Self {
            codec,
            buf: Vec::with_capacity(256 * 1024),
            header_phase_done: false,
            desynced: false,
        }
    }

    fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.extend_from_slice(chunk);

        if !self.header_phase_done {
            if self.buf.len() < 4 {
                return;
            }
            if &self.buf[0..4] == b"DKIF" {
                if self.buf.len() < 32 {
                    return;
                }
                let fourcc = &self.buf[8..12];
                if let Some(expected) = ivf_fourcc(self.codec) {
                    if fourcc != expected {
                        log::warn!(
                            "ivf: expected FOURCC {:?} for {:?}, got {:?}",
                            std::str::from_utf8(expected).unwrap_or("?"),
                            self.codec,
                            std::str::from_utf8(fourcc).unwrap_or("?"),
                        );
                    }
                }
                log::info!(
                    "ivf: file header OK (codec fourcc={})",
                    std::str::from_utf8(fourcc).unwrap_or("?")
                );
                self.buf.drain(..32);
            } else {
                log::info!(
                    "ivf: no DKIF file header on this stream (typical for gstreamer avmux_ivf \
                     on tcpserversink); parsing per-frame records directly"
                );
            }
            self.header_phase_done = true;
        }

        loop {
            if self.buf.len() < 12 {
                return;
            }
            let size =
                u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
            if size == 0 || size > MAX_FRAME_BYTES {
                log::warn!(
                    "ivf: implausible frame_size={size} bytes — byte stream is misaligned. \
                     Dropping connection so the ingest loop can reconnect and re-anchor on the \
                     next gstreamer buffer boundary."
                );
                self.desynced = true;
                self.buf.clear();
                return;
            }
            if self.buf.len() < 12 + size {
                return;
            }
            let frame = self.buf[12..12 + size].to_vec();
            self.buf.drain(..12 + size);
            out.push(frame);
        }
    }
}

/// IVF FOURCC expected on the wire. Only meaningful for codecs delivered
/// via `ivfmux` / `avmux_ivf`.
fn ivf_fourcc(codec: VideoCodec) -> Option<&'static [u8; 4]> {
    match codec {
        VideoCodec::Vp8 => Some(b"VP80"),
        VideoCodec::Vp9 => Some(b"VP90"),
        VideoCodec::Av1 => Some(b"AV01"),
        _ => None,
    }
}

/// Splits an incoming Annex-B bytestream into access units on AUD
/// boundaries. The AUD NAL type and NAL-type extraction are codec
/// specific.
///
/// Relies on the upstream parser emitting an AUD at the start of every AU
/// (`x264enc aud=true` for H.264, `x265enc option-string="aud=1"` plumbed
/// through `h265parse` for H.265). Bytes before the first AUD are
/// discarded; each subsequent AU is emitted when the *next* AU's AUD
/// arrives (one AU of buffering lag, bounded by the frame interval).
pub(super) struct AuSplitter {
    codec: VideoCodec,
    buf: Vec<u8>,
    au_start: Option<usize>,
    scan_pos: usize,
}

impl AuSplitter {
    fn new(codec: VideoCodec) -> Self {
        Self { codec, buf: Vec::with_capacity(256 * 1024), au_start: None, scan_pos: 0 }
    }

    fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.extend_from_slice(chunk);

        let Some(aud) = keyframe::aud_nal_type(self.codec) else {
            return;
        };

        while self.scan_pos + 3 < self.buf.len() {
            let i = self.scan_pos;
            let (sc_start, sc_len) = if i + 4 <= self.buf.len()
                && self.buf[i] == 0
                && self.buf[i + 1] == 0
                && self.buf[i + 2] == 0
                && self.buf[i + 3] == 1
            {
                if i + 5 > self.buf.len() {
                    break;
                }
                (i, 4)
            } else if self.buf[i] == 0 && self.buf[i + 1] == 0 && self.buf[i + 2] == 1 {
                (i, 3)
            } else {
                self.scan_pos += 1;
                continue;
            };

            let nal_off = sc_start + sc_len;
            if keyframe::nal_type(self.codec, self.buf[nal_off]) == aud {
                if let Some(start) = self.au_start.take() {
                    out.push(self.buf[start..sc_start].to_vec());
                }
                self.au_start = Some(sc_start);
            }
            self.scan_pos = nal_off + 1;
        }

        let drain_before = self.au_start.unwrap_or_else(|| self.buf.len().saturating_sub(3));
        if drain_before > 0 {
            self.buf.drain(..drain_before);
            self.scan_pos = self.scan_pos.saturating_sub(drain_before);
            if self.au_start.is_some() {
                self.au_start = Some(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ivf_frame(size: u32, payload: &[u8]) -> Vec<u8> {
        let mut rec = Vec::with_capacity(12 + payload.len());
        rec.extend_from_slice(&size.to_le_bytes());
        rec.extend_from_slice(&0u64.to_le_bytes());
        rec.extend_from_slice(payload);
        rec
    }

    fn make_dkif_header(fourcc: &[u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"DKIF");
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(fourcc);
        bytes.extend_from_slice(&[0; 20]);
        bytes
    }

    #[test]
    fn ivf_without_dkif_emits_frames() {
        let mut r = IvfReader::new(VideoCodec::Vp8);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&make_ivf_frame(4, &[1, 2, 3, 4]));
        bytes.extend_from_slice(&make_ivf_frame(2, &[9, 9]));
        let mut out = Vec::new();
        r.feed(&bytes, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], vec![1, 2, 3, 4]);
        assert_eq!(out[1], vec![9, 9]);
        assert!(!r.desynced);
    }

    #[test]
    fn ivf_with_dkif_skips_header() {
        let mut r = IvfReader::new(VideoCodec::Vp8);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&make_dkif_header(b"VP80"));
        bytes.extend_from_slice(&make_ivf_frame(3, &[7, 8, 9]));
        let mut out = Vec::new();
        r.feed(&bytes, &mut out);
        assert_eq!(out, vec![vec![7, 8, 9]]);
    }

    #[test]
    fn ivf_header_and_frame_can_arrive_across_reads() {
        let mut r = IvfReader::new(VideoCodec::Vp8);
        let mut bytes = make_dkif_header(b"VP80");
        bytes.extend_from_slice(&make_ivf_frame(4, &[1, 3, 5, 7]));
        bytes.extend_from_slice(&make_ivf_frame(2, &[8, 13]));

        let mut out = Vec::new();
        for chunk in bytes.chunks(5) {
            r.feed(chunk, &mut out);
        }

        assert_eq!(out, vec![vec![1, 3, 5, 7], vec![8, 13]]);
        assert!(!r.desynced);
    }

    #[test]
    fn ivf_absurd_size_triggers_desync() {
        let mut r = IvfReader::new(VideoCodec::Vp8);
        // Size larger than MAX_FRAME_BYTES
        let bogus = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        let mut bytes = bogus.to_vec();
        bytes.extend_from_slice(&[0u8; 8]);
        let mut out = Vec::new();
        r.feed(&bytes, &mut out);
        assert!(out.is_empty());
        assert!(r.desynced);
    }

    #[test]
    fn ivf_zero_size_triggers_desync_and_drops_buffered_bytes() {
        let mut r = IvfReader::new(VideoCodec::Vp9);
        let mut bytes = make_ivf_frame(0, &[]);
        bytes.extend_from_slice(&make_ivf_frame(3, &[1, 2, 3]));

        let mut out = Vec::new();
        r.feed(&bytes, &mut out);

        assert!(out.is_empty());
        assert!(r.desynced);
    }

    #[test]
    fn ivf_frame_header_can_arrive_across_reads_without_dkif() {
        let mut r = IvfReader::new(VideoCodec::Av1);
        let bytes = make_ivf_frame(5, &[0x0A, 0x00, 0x22, 0x00, 0x55]);

        let mut out = Vec::new();
        for chunk in bytes.chunks(2) {
            r.feed(chunk, &mut out);
        }

        assert_eq!(out, vec![vec![0x0A, 0x00, 0x22, 0x00, 0x55]]);
        assert!(!r.desynced);
    }

    #[test]
    fn au_splitter_emits_completed_aus() {
        let mut s = AuSplitter::new(VideoCodec::H264);
        // AUD NAL header for H.264: type 9, first byte = 0x09.
        // IDR slice header: type 5 => 0x65 (nal_ref_idc=3).
        let mut bytes = Vec::new();
        // AU 1: AUD + IDR
        bytes.extend_from_slice(&[0, 0, 0, 1, 0x09, 0xF0]);
        bytes.extend_from_slice(&[0, 0, 0, 1, 0x65, 0x88, 0x84]);
        // AU 2 starts with a new AUD -> AU 1 should be emitted.
        bytes.extend_from_slice(&[0, 0, 0, 1, 0x09, 0xF0]);
        bytes.extend_from_slice(&[0, 0, 0, 1, 0x41, 0x9A]);
        let mut out = Vec::new();
        s.feed(&bytes, &mut out);
        assert_eq!(out.len(), 1);
        // The first emitted AU should begin at the first AUD start code
        // and end before the second AUD.
        assert_eq!(&out[0][..5], &[0, 0, 0, 1, 0x09]);
        assert!(out[0].windows(5).any(|w| w == [0, 0, 0, 1, 0x65]));
    }

    #[test]
    fn au_splitter_discards_prefix_and_handles_split_start_codes() {
        let mut s = AuSplitter::new(VideoCodec::H264);
        let mut out = Vec::new();

        s.feed(&[0xAA, 0xBB, 0x00, 0x00], &mut out);
        s.feed(&[0x00], &mut out);
        s.feed(&[0x01, 0x09, 0xF0, 0x00, 0x00, 0x01, 0x65, 0x88], &mut out);
        assert!(out.is_empty());

        s.feed(&[0x00, 0x00], &mut out);
        s.feed(&[0x00, 0x01, 0x09, 0xF0, 0x00, 0x00, 0x01, 0x41, 0x9A], &mut out);

        assert_eq!(out.len(), 1);
        assert_eq!(&out[0][..5], &[0, 0, 0, 1, 0x09]);
        assert!(out[0].windows(5).any(|w| w == [0, 0, 1, 0x65, 0x88]));
        assert!(!out[0].starts_with(&[0xAA, 0xBB]));
    }

    #[test]
    fn au_splitter_handles_h265_aud_boundaries() {
        let mut s = AuSplitter::new(VideoCodec::H265);
        let mut out = Vec::new();

        // H.265 AUD NAL type 35 => first header byte is (35 << 1) = 0x46.
        // IDR_W_RADL NAL type 19 => first header byte is (19 << 1) = 0x26.
        s.feed(&[0, 0, 1, 0x46, 0x01, 0x50, 0, 0, 1, 0x26, 0x01, 0x88], &mut out);
        assert!(out.is_empty());

        s.feed(&[0, 0, 0, 1, 0x46, 0x01, 0x50, 0, 0, 1, 0x02, 0x01], &mut out);

        assert_eq!(out.len(), 1);
        assert_eq!(&out[0][..5], &[0, 0, 1, 0x46, 0x01]);
        assert!(out[0].windows(5).any(|w| w == [0, 0, 1, 0x26, 0x01]));
    }
}
